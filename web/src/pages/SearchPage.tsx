import { useEffect, useMemo } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { AnswerMeta, CitationList, DegradedNote, HitList } from '../features/query/AnswerParts'
import { modeLabelKey, useQueryState } from '../features/query/QueryProvider'
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorState,
  LoadingState,
  SectionTitle,
  StatusDot,
} from '../components/ui'
import { useI18n } from '../lib/i18n'

/**
 * Unified entry: one query drives the debounced hit list, the streamed answer
 * and the conversation recorded by the backend. `?q=` and `?conversation=` are
 * the deep links promised by the design (search + QA share citation state).
 */
export function SearchPage() {
  const { t, format } = useI18n()
  const [searchParams] = useSearchParams()
  const urlQuery = searchParams.get('q') ?? ''
  const urlConversation = searchParams.get('conversation')

  const {
    input,
    setInput,
    runSearch,
    ask,
    results,
    searching,
    searchError,
    lastQuery,
    stream,
    askError,
    cancelStream,
    conversationId,
    conversation,
    messages,
    openConversation,
    currentSpace,
    contentLang,
  } = useQueryState()

  // Deep links: `?q=` prefills and searches, `?conversation=` loads the thread.
  useEffect(() => {
    if (!urlQuery) return
    setInput(urlQuery)
    runSearch(urlQuery)
  }, [urlQuery, setInput, runSearch])

  useEffect(() => {
    if (!urlConversation) return
    if (urlConversation === conversationId) return
    void openConversation(urlConversation)
  }, [urlConversation, conversationId, openConversation])

  const transcriptAnswer = useMemo(() => {
    const last = [...messages].reverse().find((entry) => entry.message.role === 'assistant')
    return last ?? null
  }, [messages])

  const activeQuery = lastQuery ?? input.trim()
  const streaming = stream.active || stream.text.length > 0

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <SectionTitle
        actions={
          <>
            {conversationId && (
              <Link
                to="/conversations"
                className="rounded-lg border border-line px-2.5 py-1 text-[11.5px] text-fg-2 transition-colors hover:text-fg"
              >
                {t('nav.conversations')}
              </Link>
            )}
            <Button variant="secondary" onClick={() => activeQuery && runSearch(activeQuery)} disabled={!activeQuery}>
              {t('query.searchOnly')}
            </Button>
            {streaming && stream.active ? (
              <Button variant="ghost" onClick={cancelStream}>
                {t('query.stop')}
              </Button>
            ) : (
              <Button variant="primary" onClick={() => activeQuery && void ask(activeQuery)} disabled={!activeQuery}>
                {t('query.askAi')}
              </Button>
            )}
          </>
        }
      >
        {t('query.searchPageTitle')}
      </SectionTitle>

      <div className="mt-2 flex flex-wrap items-center gap-2 text-[11.5px] text-fg-3">
        <span className="inline-flex items-center gap-1.5">
          <StatusDot tone={currentSpace ? 'accent' : 'warn'} />
          {currentSpace ? t('query.spaceScoped', { space: currentSpace.name }) : t('query.spaceUnknown')}
        </span>
        <span>lang={contentLang ?? '—'}</span>
        {stream.rewrittenQuery && (
          <span>
            {t('query.rewritten')}: <code className="break-all">{stream.rewrittenQuery}</code>
          </span>
        )}
        {conversation && <Badge tone="neutral">{conversation.conversation_id}</Badge>}
      </div>

      <div className="mt-4 space-y-4">
        <Card
          title={t('query.queryLabel')}
          description={activeQuery || t('query.placeholder')}
          actions={
            <>
              <Badge tone="accent">{t(modeLabelKey(stream.mode))}</Badge>
              {results && <Badge>{results.mode_used}</Badge>}
            </>
          }
        >
          {askError !== null && <ErrorState error={askError} compact onRetry={() => activeQuery && void ask(activeQuery)} />}
          {!askError && streaming && (
            <div className="rounded-xl border border-line bg-raise p-3">
              <div className="mb-1 flex items-center gap-1.5 text-[10px] uppercase tracking-[0.12em] text-fg-3">
                <span className="h-1.5 w-1.5 rounded-full bg-accent" />
                {stream.active ? t('query.streaming') : t('query.answer')}
              </div>
              <p className="whitespace-pre-wrap text-[13px] leading-[1.8] text-fg">
                {stream.text || (stream.active ? '' : t('query.answerEmpty'))}
              </p>
              <DegradedNote items={stream.degraded} />
              <CitationList citations={stream.citations} />
              <AnswerMeta usage={stream.usage} rewrittenQuery={stream.rewrittenQuery} />
            </div>
          )}
          {!askError && !streaming && transcriptAnswer && (
            <div className="rounded-xl border border-line bg-raise p-3">
              <p className="whitespace-pre-wrap text-[13px] leading-[1.8] text-fg">
                {transcriptAnswer.message.content || t('query.answerEmpty')}
              </p>
              {transcriptAnswer.message.rewritten_query && (
                <p className="mt-1.5 text-[10.5px] text-fg-3">
                  {t('query.rewritten')}: <code className="break-all">{transcriptAnswer.message.rewritten_query}</code>
                </p>
              )}
              <DegradedNote items={transcriptAnswer.degraded} />
              <CitationList citations={transcriptAnswer.citations} />
              <AnswerMeta
                mode={t(modeLabelKey((transcriptAnswer.message.mode as 'search' | 'answer' | 'both') ?? 'both'))}
                model={transcriptAnswer.message.model ?? undefined}
                usage={transcriptAnswer.usage}
              />
              <p className="mt-1 text-[10px] text-fg-3">{format.dateTime(transcriptAnswer.message.created_at)}</p>
            </div>
          )}
          {!askError && !streaming && !transcriptAnswer && (
            <p className="text-[12.5px] leading-6 text-fg-2">{t('query.askHint')}</p>
          )}
        </Card>

        <Card
          title={t('query.results')}
          description={results ? t('query.hits', { count: results.hits.length }) : undefined}
          actions={
            <>
              {results && <Badge>{results.keyword_engine}</Badge>}
              {results && <Badge>{format.dateTime(results.as_of)}</Badge>}
            </>
          }
        >
          {searchError !== null && !searching ? (
            <ErrorState error={searchError} onRetry={() => activeQuery && runSearch(activeQuery)} />
          ) : searching && !results ? (
            <LoadingState label={t('query.searching')} />
          ) : results ? (
            <>
              {results.hits.length === 0 && (
                <EmptyState title={t('query.resultsEmpty')} description={t('query.resultsEmptyHint')} />
              )}
              {results.hits.length > 0 && <HitList results={results} />}
              {results.degraded.length > 0 && (
                <div className="mt-3">
                  <DegradedNote items={results.degraded} />
                </div>
              )}
            </>
          ) : (
            <EmptyState title={t('query.enterHint')} description={t('query.panelSubtitle')} />
          )}
        </Card>
      </div>
    </div>
  )
}
