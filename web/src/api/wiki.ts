/**
 * Wiki data access — JSON only.
 *
 * Reads, the page list, saves, history and lifecycle all use the JSON API
 * (`/api/v1/pages/...`): the tree comes from `GET /api/v1/pages`
 * (space/lang/limit/cursor/q), revisions from `GET .../history`
 * (`include_markdown=true`) or `GET .../revisions/{revision_id}`, and
 * delete/restore are plain JSON writes. Nothing here parses backend HTML.
 */
import { ApiError, api, encodePathSegment } from './client'
import type { PageSummaryResponse, SearchHit } from './types'

/* ------------------------------------------------------------------- reads */

export interface WikiPage {
  space: string
  slug: string
  lang: string
  title: string
  summary: string
  version: number
  current_revision_id: string
  content_hash: string
  created_at: string
  updated_at: string
  markdown: string
  /** Backend-rendered HTML (pulldown-cmark + ammonia). */
  html: string
  author: string
  revision_version: number
}

/**
 * Revision metadata as returned by `GET .../history` (the endpoint returns a
 * slim view: version/author/time/hash — no body).
 */
export interface WikiRevision {
  revision_id: string
  version: number
  author: string
  created_at: string
  content_hash: string
  valid_from: string
  /** Only populated if a future backend starts returning revision bodies. */
  markdown?: string
}

function langSuffix(lang?: string | null, extra?: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams()
  if (lang) search.set('lang', lang)
  for (const [key, value] of Object.entries(extra ?? {})) {
    if (value !== undefined && value !== '') search.set(key, String(value))
  }
  const text = search.toString()
  return text ? `?${text}` : ''
}

function pagePath(space: string, slug: string, suffix = ''): string {
  return `/api/v1/pages/${encodePathSegment(space)}/${encodePathSegment(slug)}${suffix}`
}

/** `GET /api/v1/pages/{space}/{slug}` — 404 when absent or archived. */
export function getPage(space: string, slug: string, lang?: string | null): Promise<WikiPage> {
  return api<WikiPage>(`${pagePath(space, slug)}${langSuffix(lang)}`)
}

/** `GET /api/v1/pages/{space}/{slug}/history?include_markdown=`. */
export async function getPageHistory(
  space: string,
  slug: string,
  lang?: string | null,
  includeMarkdown = false,
): Promise<WikiRevision[]> {
  const query = langSuffix(lang) + (includeMarkdown ? `${lang ? '&' : '?'}include_markdown=true` : '')
  const response = await api<{ revisions?: WikiRevision[] }>(`${pagePath(space, slug, '/history')}${query}`)
  return response.revisions ?? []
}

/* ------------------------------------------------------------------- writes */

export interface SavePageInput {
  space: string
  slug: string
  title: string
  markdown: string
  lang: string
  /**
   * `If-Match` value: the revision the editor loaded, or `null` to create
   * (`*`). A stale value yields 409 `revision_conflict`.
   */
  baseRevisionId: string | null
  summary?: string
}

export interface SavePageResult {
  space: string
  slug: string
  lang: string
  version: number
  revision_id: string
  content_hash: string
  title: string
}

/** `PUT /api/v1/pages/{space}/{slug}` with `If-Match`. */
export function savePage(input: SavePageInput): Promise<SavePageResult> {
  return api<SavePageResult>(pagePath(input.space, input.slug), {
    method: 'PUT',
    body: {
      title: input.title,
      markdown: input.markdown,
      lang: input.lang,
      ...(input.summary !== undefined ? { summary: input.summary } : {}),
    },
    headers: { 'if-match': input.baseRevisionId ?? '*' },
  })
}

/** `DELETE /api/v1/pages/{space}/{slug}` — soft delete (archives page + index). */
export function deletePage(space: string, slug: string, lang?: string | null): Promise<{ archived: boolean }> {
  return api<{ archived: boolean }>(`${pagePath(space, slug)}${langSuffix(lang)}`, { method: 'DELETE' })
}

/** `POST /api/v1/pages/{space}/{slug}/restore`. */
export function restorePage(space: string, slug: string, lang?: string | null): Promise<{ restored: boolean }> {
  return api<{ restored: boolean }>(`${pagePath(space, slug, '/restore')}${langSuffix(lang)}`, { method: 'POST' })
}

/** `GET /api/v1/pages/{space}/{slug}/summary` — static, per-revision summary. */
export function getPageSummary(space: string, slug: string, lang?: string | null): Promise<PageSummaryResponse> {
  return api<PageSummaryResponse>(`${pagePath(space, slug, '/summary')}${langSuffix(lang)}`)
}

export function isRevisionConflictError(error: unknown): boolean {
  return error instanceof ApiError && (error.code === 'revision_conflict' || error.status === 409)
}

/* --------------------------------------------------------------- page list */

export interface PageListEntry {
  slug: string
  title: string
  lang: string
  order_index: number
  updated_at: string
  current_revision_id: string
  version: number
  summary: string
}

export interface PageList {
  pages: PageListEntry[]
  next_cursor: string | null
}

/** `GET /api/v1/pages` — scoped to one space; `q` filters slug/title server-side. */
export async function listPages(params: {
  space: string
  lang?: string | null
  q?: string
  limit?: number
  cursor?: string | null
}): Promise<PageList> {
  const search = new URLSearchParams({ space: params.space })
  if (params.lang) search.set('lang', params.lang)
  if (params.q) search.set('q', params.q)
  if (params.limit) search.set('limit', String(params.limit))
  if (params.cursor) search.set('cursor', params.cursor)
  const response = await api<PageList>(`/api/v1/pages?${search.toString()}`)
  return { pages: response.pages ?? [], next_cursor: response.next_cursor ?? null }
}

/** Full revision body — `GET /api/v1/pages/{space}/{slug}/revisions/{id}`. */
export interface WikiRevisionBody {
  revision_id: string
  space: string
  slug: string
  lang: string
  title: string
  markdown: string
  author: string
  version: number
  created_at: string
  content_hash: string
}

export function getRevision(space: string, slug: string, revisionId: string): Promise<WikiRevisionBody> {
  return api<WikiRevisionBody>(
    `${pagePath(space, slug, `/revisions/${encodePathSegment(revisionId)}`)}`,
  )
}

/** True when a search/citation `path` addresses a Wiki page. */
export function parseWikiPath(path: string): { space: string; lang: string; slug: string } | null {
  const match = /^wiki\/([^/]+)\/([^/]+)\/(.+)\.md$/.exec(path)
  if (!match) return null
  return { space: match[1], lang: match[2], slug: match[3] }
}

/** Maps a search hit / citation to a reader URL when it points at a Wiki page. */
export function hitReaderPath(hit: Pick<SearchHit, 'path' | 'quote'>): string | null {
  const parsed = parseWikiPath(hit.path)
  if (!parsed) return null
  const params = new URLSearchParams({ lang: parsed.lang })
  if (hit.quote) params.set('hl', hit.quote.slice(0, 180))
  return `/w/${encodePathSegment(parsed.space)}/${encodePathSegment(parsed.slug)}?${params.toString()}`
}
