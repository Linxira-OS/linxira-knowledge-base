import { ApiError, api, ensureCsrfToken } from './client'
import { asJsonObject } from '../lib/json'
import type {
  Answer,
  AnswerUsage,
  Citation,
  Conversation,
  ConversationMessage,
  Degraded,
  QueryRequest,
  QueryResponse,
  SearchResponse,
} from './types'

export function postQuery(body: QueryRequest, signal?: AbortSignal): Promise<QueryResponse> {
  return api<QueryResponse>('/api/v1/query', { method: 'POST', body, signal })
}

export interface StreamCallbacks {
  onMeta?: (meta: { conversation_id: string; message_id: string; mode: string; rewritten_query: string }) => void
  onResults?: (results: SearchResponse | null) => void
  onToken?: (text: string) => void
  onCitation?: (citation: Citation) => void
  onUsage?: (usage: AnswerUsage | null) => void
  onDegraded?: (degraded: Degraded[]) => void
  onDone?: (payload: { conversation_id: string; message_id: string }) => void
}

/**
 * POST /api/v1/query/stream — the backend emits the whole answer as a burst of
 * SSE frames (meta → results → token* → citation* → usage → degraded → done).
 * Frames are parsed incrementally, so a genuinely streaming backend needs no
 * client change; a JSON error body is surfaced as an `ApiError`.
 */
export async function streamQuery(
  body: QueryRequest,
  callbacks: StreamCallbacks,
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch('/api/v1/query/stream', {
    method: 'POST',
    credentials: 'include',
    headers: {
      'content-type': 'application/json',
      accept: 'text/event-stream',
      'x-csrf-token': await ensureCsrfToken(),
      'accept-language': document.documentElement.lang || 'en',
    },
    body: JSON.stringify(body),
    signal,
  })

  const contentType = response.headers.get('content-type') ?? ''
  if (!response.ok) {
    const text = await response.text().catch(() => '')
    throw errorFromBody(response, text)
  }
  if (!contentType.includes('text/event-stream')) {
    const text = await response.text()
    if (text) parseSseBlock(text, callbacks)
    return
  }

  const reader = response.body?.getReader()
  if (!reader) {
    const text = await response.text()
    if (text) parseSseBlock(text, callbacks)
    return
  }

  const decoder = new TextDecoder()
  let buffer = ''
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })
    let index = buffer.indexOf('\n\n')
    while (index !== -1) {
      const block = buffer.slice(0, index)
      buffer = buffer.slice(index + 2)
      if (block.trim()) parseSseBlock(block, callbacks)
      index = buffer.indexOf('\n\n')
    }
  }
  buffer += decoder.decode()
  if (buffer.trim()) parseSseBlock(buffer, callbacks)
}

function errorFromBody(response: Response, text: string): ApiError {
  let code = 'http.unknown'
  let message = ''
  try {
    const parsed: unknown = JSON.parse(text)
    if (parsed && typeof parsed === 'object') {
      const body = parsed as Record<string, unknown>
      if (typeof body.error === 'string') code = body.error
      if (typeof body.message === 'string') message = body.message
    }
  } catch {
    /* keep defaults */
  }
  return new ApiError({
    status: response.status,
    code,
    serverMessage: message,
    requestId: response.headers.get('x-request-id'),
    retryAfterSeconds: null,
    method: 'POST',
    path: '/api/v1/query/stream',
    body: text.slice(0, 2000),
  })
}

function parseSseBlock(block: string, callbacks: StreamCallbacks): void {
  let event = 'message'
  const dataLines: string[] = []
  for (const rawLine of block.split('\n')) {
    const line = rawLine.trimEnd()
    if (!line || line.startsWith(':')) continue
    if (line.startsWith('event:')) {
      event = line.slice(6).trim()
    } else if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trimStart())
    }
  }
  if (dataLines.length === 0) return
  let data: unknown
  try {
    data = JSON.parse(dataLines.join('\n'))
  } catch {
    return
  }
  // Frames are typed by their `event:` name; each branch below checks the
  // fields it actually reads before handing them to a callback.
  const object = asJsonObject(data)
  switch (event) {
    case 'meta':
      if (object && typeof object.conversation_id === 'string') {
        callbacks.onMeta?.({
          conversation_id: object.conversation_id,
          message_id: typeof object.message_id === 'string' ? object.message_id : '',
          mode: typeof object.mode === 'string' ? object.mode : '',
          rewritten_query: typeof object.rewritten_query === 'string' ? object.rewritten_query : '',
        })
      }
      break
    case 'results':
      if (data === null || object) callbacks.onResults?.(data as SearchResponse | null)
      break
    case 'token':
      if (object && typeof object.content === 'string') callbacks.onToken?.(object.content)
      break
    case 'citation':
      if (object && typeof object.doc_id === 'string') callbacks.onCitation?.(data as unknown as Citation)
      break
    case 'usage':
      if (data === null) callbacks.onUsage?.(null)
      else if (object) callbacks.onUsage?.(data as unknown as AnswerUsage)
      break
    case 'degraded':
      if (Array.isArray(data)) callbacks.onDegraded?.(data as Degraded[])
      break
    case 'done':
      if (object && typeof object.conversation_id === 'string') {
        callbacks.onDone?.({
          conversation_id: object.conversation_id,
          message_id: typeof object.message_id === 'string' ? object.message_id : '',
        })
      }
      break
    default:
      break
  }
}

/* ------------------------------------------------------------- conversations */

export function listConversations(params: {
  limit?: number
  cursor?: string
  external_session_id?: string
}): Promise<{ conversations: Conversation[]; next_cursor: string | null }> {
  const search = new URLSearchParams()
  if (params.limit) search.set('limit', String(params.limit))
  if (params.cursor) search.set('cursor', params.cursor)
  if (params.external_session_id) search.set('external_session_id', params.external_session_id)
  const suffix = search.toString() ? `?${search.toString()}` : ''
  return api(`/api/v1/conversations${suffix}`)
}

export function createConversation(body: { space?: string; title?: string }): Promise<{ conversation: Conversation }> {
  return api('/api/v1/conversations', { method: 'POST', body })
}

export function getConversation(
  conversationId: string,
): Promise<{ conversation: Conversation; messages: ConversationMessage[] }> {
  return api(`/api/v1/conversations/${encodeURIComponent(conversationId)}`)
}

export function deleteConversation(conversationId: string): Promise<{ conversation_id: string; status: string }> {
  return api(`/api/v1/conversations/${encodeURIComponent(conversationId)}`, { method: 'DELETE' })
}

/** `POST /api/v1/conversations/{id}/restore` — undoes the soft delete. */
export function restoreConversation(
  conversationId: string,
): Promise<{ restored: boolean; status: string }> {
  return api(`/api/v1/conversations/${encodeURIComponent(conversationId)}/restore`, { method: 'POST' })
}

export function bindExternalSession(
  conversationId: string,
  externalSessionId: string,
): Promise<{ conversation_id: string; external_session_id: string }> {
  return api(`/api/v1/conversations/${encodeURIComponent(conversationId)}/external-session`, {
    method: 'POST',
    body: { external_session_id: externalSessionId },
  })
}

/* ----------------------------------------------------------------- decoding */

function parseArray<T>(value: string | null | undefined): T[] {
  if (!value) return []
  try {
    const parsed: unknown = JSON.parse(value)
    return Array.isArray(parsed) ? (parsed as T[]) : []
  } catch {
    return []
  }
}

/** `messages[*].citations_json` is a JSON-encoded string inside JSON. */
export function decodeCitations(value: string | null | undefined): Citation[] {
  return parseArray<Citation>(value)
}

export function decodeDegraded(value: string | null | undefined): Degraded[] {
  return parseArray<Degraded>(value)
}

export function decodeUsage(value: string | null | undefined): AnswerUsage | null {
  if (!value || value === 'null') return null
  try {
    const parsed: unknown = JSON.parse(value)
    return asJsonObject(parsed) ? (parsed as unknown as AnswerUsage) : null
  } catch {
    return null
  }
}

export interface DecodedMessage {
  message: ConversationMessage
  citations: Citation[]
  degraded: Degraded[]
  usage: AnswerUsage | null
}

export function decodeMessage(message: ConversationMessage): DecodedMessage {
  return {
    message,
    citations: decodeCitations(message.citations_json),
    degraded: decodeDegraded(message.degraded_json),
    usage: decodeUsage(message.usage_json),
  }
}

export type { Answer, Citation, Conversation, ConversationMessage, SearchResponse }
