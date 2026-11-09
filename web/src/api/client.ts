import { asJsonObject } from '../lib/json'
import type { ErrorBody } from './types'

/**
 * One fetch wrapper for every backend call.
 *
 * - same-origin credentials so the cookie session and `lkb_csrf` are sent;
 * - `X-CSRF-Token` on writes, bootstrapped lazily via `GET /api/v1/auth/csrf`;
 * - non-2xx never throws raw: it produces an `ApiError` carrying the stable
 *   backend `code`, the server-localised `message`, `x-request-id` and
 *   `Retry-After`, so callers can branch on `code`, not on prose.
 */

export interface ApiErrorInit {
  status: number
  code: string
  serverMessage: string
  requestId: string | null
  retryAfterSeconds: number | null
  method: string
  path: string
  body?: string
}

export class ApiError extends Error {
  readonly status: number
  readonly code: string
  readonly serverMessage: string
  readonly requestId: string | null
  readonly retryAfterSeconds: number | null
  readonly method: string
  readonly path: string
  readonly body?: string

  constructor(init: ApiErrorInit) {
    super(`${init.method} ${init.path} → ${init.status} ${init.code}`)
    this.name = 'ApiError'
    this.status = init.status
    this.code = init.code
    this.serverMessage = init.serverMessage
    this.requestId = init.requestId
    this.retryAfterSeconds = init.retryAfterSeconds
    this.method = init.method
    this.path = init.path
    this.body = init.body
  }
}

let apiLocale = 'en'
let csrfMemo: string | null = null
const unauthorizedListeners = new Set<(error: ApiError) => void>()

export function setApiLocale(locale: string): void {
  apiLocale = locale
}

/** Auth state change hook (401) — the session provider subscribes. */
export function onUnauthorized(listener: (error: ApiError) => void): () => void {
  unauthorizedListeners.add(listener)
  return () => unauthorizedListeners.delete(listener)
}

export function readCookie(name: string): string | null {
  const prefix = `${name}=`
  for (const part of document.cookie.split(';')) {
    const trimmed = part.trim()
    if (trimmed.startsWith(prefix)) return decodeURIComponent(trimmed.slice(prefix.length))
  }
  return null
}

function parseRetryAfter(response: Response): number | null {
  const raw = response.headers.get('retry-after')
  if (!raw) return null
  const seconds = Number(raw)
  return Number.isFinite(seconds) && seconds >= 0 ? seconds : null
}

function parseErrorBody(text: string): { code: string; message: string } | null {
  if (!text) return null
  try {
    const parsed: unknown = JSON.parse(text)
    if (!parsed || typeof parsed !== 'object') return null
    const body = parsed as Record<string, unknown>
    // Current backend: {"error": "<code>", "message": "<text>"}.
    if (typeof body.error === 'string') {
      return { code: body.error, message: typeof body.message === 'string' ? body.message : '' }
    }
    // Tolerated alternative envelope: {"error": {"code": …, "message": …}}.
    if (body.error && typeof body.error === 'object') {
      const nested = body.error as Record<string, unknown>
      if (typeof nested.code === 'string') {
        return { code: nested.code, message: typeof nested.message === 'string' ? nested.message : '' }
      }
    }
    return null
  } catch {
    return null
  }
}

function codeForStatus(status: number): string {
  switch (status) {
    case 400:
      return 'http.unknown'
    case 401:
      return 'error.unauthorized'
    case 403:
      return 'error.forbidden'
    case 404:
      return 'error.not_found'
    case 409:
      return 'error.conflict'
    case 413:
      return 'error.space_quota_exceeded'
    case 415:
      return 'http.unknown'
    case 422:
      return 'error.bad_request'
    case 429:
      return 'error.rate_limited'
    case 503:
      return 'error.queue_full'
    default:
      return status >= 500 ? 'error.internal' : 'http.unknown'
  }
}

/** Ensures the double-submit CSRF cookie exists before a write request. */
export async function ensureCsrfToken(force = false): Promise<string> {
  if (!force && csrfMemo) {
    const fromCookie = readCookie('lkb_csrf')
    if (fromCookie) return fromCookie
  }
  const existing = readCookie('lkb_csrf')
  if (existing) {
    csrfMemo = existing
    return existing
  }
  // JSON bootstrap: the endpoint sets `lkb_csrf` and returns the token.
  let response: Response
  try {
    response = await fetch('/api/v1/auth/csrf', {
      method: 'GET',
      credentials: 'include',
      headers: { accept: 'application/json', 'accept-language': apiLocale },
    })
  } catch {
    throw new ApiError({
      status: 0,
      code: 'http.csrf',
      serverMessage: '',
      requestId: null,
      retryAfterSeconds: null,
      method: 'GET',
      path: '/api/v1/auth/csrf',
    })
  }
  const body = await response.json().catch(() => null)
  const tokenFromBody = asJsonObject(body)?.csrf_token
  const issued = typeof tokenFromBody === 'string' ? tokenFromBody : readCookie('lkb_csrf')
  if (!issued) {
    throw new ApiError({
      status: response.status,
      code: 'http.csrf',
      serverMessage: '',
      requestId: response.headers.get('x-request-id'),
      retryAfterSeconds: null,
      method: 'GET',
      path: '/api/v1/auth/csrf',
    })
  }
  csrfMemo = issued
  return issued
}

export function resetCsrfMemo(): void {
  csrfMemo = null
}

export interface RequestOptions {
  method?: string
  body?: unknown
  signal?: AbortSignal
  headers?: Record<string, string>
}

async function dispatch(path: string, options: RequestOptions): Promise<Response> {
  const method = (options.method ?? 'GET').toUpperCase()
  const headers: Record<string, string> = {
    accept: 'application/json',
    'accept-language': apiLocale,
    ...options.headers,
  }

  let body: BodyInit | undefined
  if (options.body !== undefined) {
    headers['content-type'] = 'application/json'
    body = JSON.stringify(options.body)
  }

  const needsCsrf = method !== 'GET' && method !== 'HEAD'
  if (needsCsrf) {
    headers['x-csrf-token'] = await ensureCsrfToken()
  }

  let response: Response
  try {
    response = await fetch(path, {
      method,
      body,
      headers,
      credentials: 'include',
      signal: options.signal,
    })
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') throw error
    throw new ApiError({
      status: 0,
      code: 'http.network',
      serverMessage: '',
      requestId: null,
      retryAfterSeconds: null,
      method,
      path,
    })
  }

  if (!response.ok) {
    const text = await response.text().catch(() => '')
    const parsed = parseErrorBody(text)
    const csrfRetryable =
      response.status === 403 && /csrf/i.test(parsed?.message ?? text) && needsCsrf
    if (csrfRetryable) {
      // Cookie rotated (another tab, or an expired session): refresh once.
      resetCsrfMemo()
      const token = await ensureCsrfToken(true).catch(() => null)
      if (token) headers['x-csrf-token'] = token
      const retried = await fetch(path, { method, body, headers, credentials: 'include', signal: options.signal })
      if (retried.ok) return retried
      return await fail(retried, method, path)
    }
    return await fail(response, method, path, text, parsed)
  }

  return response
}

async function fail(
  response: Response,
  method: string,
  path: string,
  cachedText?: string,
  cachedParsed?: { code: string; message: string } | null,
): Promise<never> {
  const text = cachedText ?? (await response.text().catch(() => ''))
  const parsed = cachedParsed ?? parseErrorBody(text)
  const error = new ApiError({
    status: response.status,
    code: parsed?.code && parsed.code.length > 0 ? parsed.code : codeForStatus(response.status),
    serverMessage: parsed?.message ?? '',
    requestId: response.headers.get('x-request-id'),
    retryAfterSeconds: parseRetryAfter(response),
    method,
    path,
    body: text.slice(0, 2000),
  })
  if (response.status === 401) {
    for (const listener of unauthorizedListeners) listener(error)
  }
  throw error
}

/** JSON request. 204/empty bodies resolve to `undefined`. */
export async function api<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const response = await dispatch(path, options)
  if (response.status === 204) return undefined as T
  const text = await response.text()
  if (!text) return undefined as T
  try {
    return JSON.parse(text) as T
  } catch {
    throw new ApiError({
      status: response.status,
      code: 'http.invalidReply',
      serverMessage: '',
      requestId: response.headers.get('x-request-id'),
      retryAfterSeconds: null,
      method: options.method ?? 'GET',
      path,
      body: text.slice(0, 500),
    })
  }
}

export function queryString(params: Record<string, string | number | boolean | null | undefined>): string {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === undefined || value === '') continue
    search.set(key, String(value))
  }
  const text = search.toString()
  return text ? `?${text}` : ''
}

export function encodePathSegment(value: string): string {
  return encodeURIComponent(value)
}

export type { ErrorBody }
