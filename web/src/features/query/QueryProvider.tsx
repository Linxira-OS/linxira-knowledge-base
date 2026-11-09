import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ApiError } from '../../api/client'
import { assertNever } from '../../lib/assert'
import { listSpaces } from '../../api/spaces'
import {
  decodeMessage,
  getConversation,
  postQuery,
  streamQuery,
  type DecodedMessage,
} from '../../api/query'
import type {
  AnswerUsage,
  Citation,
  Conversation,
  Degraded,
  QueryMode,
  SearchResponse,
  Space,
} from '../../api/types'
import type { Locale } from '../../lib/i18n'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'

const SPACE_KEY = 'lkb-space'
const LANG_KEY = 'lkb-content-lang'

export interface QueryStreamState {
  active: boolean
  text: string
  citations: Citation[]
  usage: AnswerUsage | null
  degraded: Degraded[]
  mode: QueryMode
  rewrittenQuery: string | null
}

interface QueryValue {
  spaces: Space[]
  spacesLoading: boolean
  spacesError: unknown
  /** Spaces the principal can read (role is set) — used for pickers. */
  readableSpaces: Space[]
  spaceId: string | null
  setSpaceId: (id: string) => void
  currentSpace: Space | null

  contentLang: string | null
  setContentLang: (lang: string | null) => void

  input: string
  setInput: (value: string) => void

  results: SearchResponse | null
  searching: boolean
  searchError: unknown
  lastQuery: string | null
  runSearch: (query: string) => void

  conversationId: string | null
  conversation: Conversation | null
  messages: DecodedMessage[]
  conversationLoading: boolean
  conversationError: unknown
  startNewConversation: () => void
  openConversation: (conversationId: string) => Promise<void>

  ask: (query: string) => Promise<void>
  stream: QueryStreamState
  askError: unknown
  cancelStream: () => void
}

const QueryContext = createContext<QueryValue | null>(null)

function localeToLang(locale: Locale): string {
  return locale === 'zh-CN' ? 'zh' : 'en'
}

function initialContentLang(locale: Locale): string | null {
  try {
    const stored = localStorage.getItem(LANG_KEY)
    if (stored === '') return null
    if (stored) return stored
  } catch {
    /* ignore */
  }
  return localeToLang(locale)
}

function initialSpaceId(): string | null {
  try {
    return localStorage.getItem(SPACE_KEY)
  } catch {
    return null
  }
}

const EMPTY_STREAM: QueryStreamState = {
  active: false,
  text: '',
  citations: [],
  usage: null,
  degraded: [],
  mode: 'both',
  rewrittenQuery: null,
}

/**
 * Unified search/answer state shared by the top bar, the QA panel and the
 * search page: one query, one conversation, one result set.
 *
 * Debounced typing issues `mode=search` calls (which the backend records in the
 * conversation history, as designed); Enter / “Ask AI” streams `mode=both` and
 * refreshes the conversation so the panel always shows the KB's canonical
 * transcript rather than a local copy.
 */
export function QueryProvider({ children }: { children: ReactNode }) {
  const { locale } = useI18n()
  const queryClient = useQueryClient()

  const session = useSession()
  // Fetch the space list only once the session state is known, and only when
  // this visitor may read it: an eager anonymous call caches a 401 that a
  // client-side sign-in never clears, leaving the shell without a space. The
  // enable flip after login (plus the invalidation in SessionProvider) fetches
  // it under the fresh session.
  const spacesQuery = useQuery({
    queryKey: ['spaces'],
    queryFn: listSpaces,
    enabled: session.ready && (session.status === 'authenticated' || !session.authRequired),
  })

  const [spaceId, setSpaceIdState] = useState<string | null>(initialSpaceId)
  const [contentLang, setContentLangState] = useState<string | null>(() => initialContentLang(locale))
  const [input, setInput] = useState('')
  const [results, setResults] = useState<SearchResponse | null>(null)
  const [searching, setSearching] = useState(false)
  const [searchError, setSearchError] = useState<unknown>(null)
  const [lastQuery, setLastQuery] = useState<string | null>(null)
  const [conversationId, setConversationId] = useState<string | null>(null)
  const [stream, setStream] = useState<QueryStreamState>(EMPTY_STREAM)
  const [askError, setAskError] = useState<unknown>(null)

  const searchAbort = useRef<AbortController | null>(null)
  const streamAbort = useRef<AbortController | null>(null)
  const lastSearchKey = useRef<string | null>(null)

  const spaces = useMemo(() => spacesQuery.data?.spaces ?? [], [spacesQuery.data])
  const readableSpaces = useMemo(() => spaces.filter((space) => space.role !== null), [spaces])

  // Keep the selected space valid once the list arrives.
  useEffect(() => {
    if (readableSpaces.length === 0) return
    const known = spaceId !== null && readableSpaces.some((space) => space.space_id === spaceId)
    if (!known) setSpaceIdState(readableSpaces[0].space_id)
  }, [readableSpaces, spaceId])

  const setSpaceId = useCallback((id: string) => {
    setSpaceIdState(id)
    try {
      localStorage.setItem(SPACE_KEY, id)
    } catch {
      /* ignore */
    }
    // A conversation belongs to one space; switching space starts a new thread.
    setConversationId(null)
    setResults(null)
    setLastQuery(null)
    lastSearchKey.current = null
  }, [])

  const setContentLang = useCallback((lang: string | null) => {
    setContentLangState(lang)
    try {
      localStorage.setItem(LANG_KEY, lang ?? '')
    } catch {
      /* ignore */
    }
    lastSearchKey.current = null
  }, [])

  const currentSpace = useMemo(
    () => readableSpaces.find((space) => space.space_id === spaceId) ?? null,
    [readableSpaces, spaceId],
  )

  const conversationQuery = useQuery({
    queryKey: ['conversation', conversationId],
    queryFn: () => getConversation(conversationId as string),
    enabled: conversationId !== null,
    retry: false,
  })

  const messages = useMemo<DecodedMessage[]>(
    () => (conversationQuery.data?.messages ?? []).map(decodeMessage),
    [conversationQuery.data],
  )

  const conversation = conversationQuery.data?.conversation ?? null

  const invalidateConversations = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    if (conversationId) void queryClient.invalidateQueries({ queryKey: ['conversation', conversationId] })
  }, [queryClient, conversationId])

  const runSearch = useCallback(
    (query: string) => {
      const trimmed = query.trim()
      const space = spaceId
      if (!trimmed || !space) return
      const key = `${space}|${contentLang ?? ''}|${trimmed}|${conversationId ?? ''}`
      if (key === lastSearchKey.current) return
      lastSearchKey.current = key

      searchAbort.current?.abort()
      const controller = new AbortController()
      searchAbort.current = controller
      setSearching(true)
      setSearchError(null)
      setLastQuery(trimmed)

      void postQuery(
        {
          query: trimmed,
          space,
          mode: 'search',
          ...(contentLang ? { lang: contentLang } : {}),
          ...(conversationId ? { conversation_id: conversationId } : {}),
        },
        controller.signal,
      )
        .then((response) => {
          if (controller.signal.aborted) return
          setResults(response.results)
          if (!conversationId) setConversationId(response.conversation_id)
          else invalidateConversations()
        })
        .catch((error: unknown) => {
          if (controller.signal.aborted) return
          if (error instanceof DOMException && error.name === 'AbortError') return
          setSearchError(error)
        })
        .finally(() => {
          if (!controller.signal.aborted) setSearching(false)
        })
    },
    [spaceId, contentLang, conversationId, invalidateConversations],
  )

  // Debounce typing into searches while the user pauses.
  useEffect(() => {
    const trimmed = input.trim()
    if (trimmed.length < 2) return undefined
    const timer = window.setTimeout(() => runSearch(trimmed), 400)
    return () => window.clearTimeout(timer)
  }, [input, runSearch])

  const cancelStream = useCallback(() => {
    streamAbort.current?.abort()
    streamAbort.current = null
    setStream((current) => ({ ...current, active: false }))
  }, [])

  const ask = useCallback(
    async (query: string) => {
      const trimmed = query.trim()
      const space = spaceId
      if (!trimmed || !space) return
      streamAbort.current?.abort()
      const controller = new AbortController()
      streamAbort.current = controller
      setAskError(null)
      setLastQuery(trimmed)
      setStream({ ...EMPTY_STREAM, active: true, mode: 'both' })
      try {
        await streamQuery(
          {
            query: trimmed,
            space,
            mode: 'both',
            ...(contentLang ? { lang: contentLang } : {}),
            ...(conversationId ? { conversation_id: conversationId } : {}),
          },
          {
            onMeta: (meta) => {
              setStream((current) => ({
                ...current,
                mode: (meta.mode as QueryMode) ?? current.mode,
                rewrittenQuery: meta.rewritten_query || null,
              }))
            },
            onResults: (next) => setResults(next),
            onToken: (text) => setStream((current) => ({ ...current, text: current.text + text })),
            onCitation: (citation) => setStream((current) => ({ ...current, citations: [...current.citations, citation] })),
            onUsage: (usage) => setStream((current) => ({ ...current, usage })),
            onDegraded: (degraded) => setStream((current) => ({ ...current, degraded })),
            onDone: (payload) => {
              setConversationId(payload.conversation_id)
              setStream((current) => ({ ...current, active: false }))
            },
          },
          controller.signal,
        )
        invalidateConversations()
      } catch (error) {
        if (error instanceof DOMException && error.name === 'AbortError') return
        setAskError(error)
        setStream((current) => ({ ...current, active: false }))
      }
    },
    [spaceId, contentLang, conversationId, invalidateConversations],
  )

  const startNewConversation = useCallback(() => {
    cancelStream()
    setConversationId(null)
    setResults(null)
    setLastQuery(null)
    lastSearchKey.current = null
    setAskError(null)
    setStream(EMPTY_STREAM)
  }, [cancelStream])

  const openConversation = useCallback(
    async (id: string) => {
      cancelStream()
      setConversationId(id)
      setAskError(null)
      setStream(EMPTY_STREAM)
      try {
        const detail = await getConversation(id)
        const space = detail.conversation.space_id
        if (readableSpaces.some((entry) => entry.space_id === space)) setSpaceIdState(space)
        const lastUser = [...detail.messages].reverse().find((message) => message.role === 'user')
        if (lastUser) {
          setInput(lastUser.content)
          setLastQuery(lastUser.content)
          lastSearchKey.current = null
          if (space) {
            const response = await postQuery({
              query: lastUser.content,
              space,
              mode: 'search',
              conversation_id: id,
            })
            setResults(response.results)
          }
        }
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) {
          setConversationId(null)
        }
        setAskError(error)
      }
    },
    [cancelStream, readableSpaces],
  )

  const value = useMemo<QueryValue>(
    () => ({
      spaces,
      spacesLoading: spacesQuery.isLoading,
      spacesError: spacesQuery.error,
      readableSpaces,
      spaceId,
      setSpaceId,
      currentSpace,
      contentLang,
      setContentLang,
      input,
      setInput,
      results,
      searching,
      searchError,
      lastQuery,
      runSearch,
      conversationId,
      conversation,
      messages,
      conversationLoading: conversationQuery.isLoading,
      conversationError: conversationQuery.error,
      startNewConversation,
      openConversation,
      ask,
      stream,
      askError,
      cancelStream,
    }),
    [
      spaces,
      spacesQuery.isLoading,
      spacesQuery.error,
      readableSpaces,
      spaceId,
      setSpaceId,
      currentSpace,
      contentLang,
      setContentLang,
      input,
      results,
      searching,
      searchError,
      lastQuery,
      runSearch,
      conversationId,
      conversation,
      messages,
      conversationQuery.isLoading,
      conversationQuery.error,
      startNewConversation,
      openConversation,
      ask,
      stream,
      askError,
      cancelStream,
    ],
  )

  return <QueryContext.Provider value={value}>{children}</QueryContext.Provider>
}

export function useQueryState(): QueryValue {
  const value = useContext(QueryContext)
  if (!value) throw new Error('useQueryState must be used inside <QueryProvider>')
  return value
}

/** Exhaustiveness helper used by the query mode switch statements. */
export function modeLabelKey(mode: QueryMode): 'query.mode.search' | 'query.mode.answer' | 'query.mode.both' {
  switch (mode) {
    case 'search':
      return 'query.mode.search'
    case 'answer':
      return 'query.mode.answer'
    case 'both':
      return 'query.mode.both'
    default:
      return assertNever(mode)
  }
}
