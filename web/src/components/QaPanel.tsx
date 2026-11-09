import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { AnswerMeta, CitationList, DegradedNote } from '../features/query/AnswerParts'
import { useQueryState, modeLabelKey } from '../features/query/QueryProvider'
import { Badge, Button, ErrorState, Banner, cx } from './ui'
import { useI18n } from '../lib/i18n'
import { useStickToBottom } from '../lib/hooks'

/**
 * Right-hand QA panel. It renders the KB's own transcript (refetched after each
 * turn) plus the in-flight stream, and it shares query/conversation state with
 * the top bar and the search page through `useQueryState()`.
 */
export function QaPanel({ width, onCollapse }: { width: number; onCollapse: () => void }) {
  const { t, format } = useI18n()
  const navigate = useNavigate()
  const {
    messages,
    stream,
    ask,
    askError,
    searching,
    searchError,
    conversationId,
    conversationLoading,
    startNewConversation,
    currentSpace,
    contentLang,
    spaceId,
  } = useQueryState()

  const [draft, setDraft] = useState('')
  const { ref, onScroll } = useStickToBottom<HTMLDivElement>(messages.length + stream.text.length)

  const submitAsk = () => {
    const value = draft.trim()
    if (!value || !spaceId) return
    setDraft('')
    void ask(value)
  }

  const submitSearch = () => {
    const value = draft.trim()
    if (!value) return
    setDraft('')
    navigate(`/search?q=${encodeURIComponent(value)}`)
  }

  return (
    <aside style={{ width }} className="flex min-h-0 shrink-0 flex-col border-l border-line bg-panel">
      <div className="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-line px-3.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />
          <span className="truncate text-[12px] font-medium text-fg">{t('query.panelTitle')}</span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Badge tone={conversationId ? 'accent' : 'neutral'}>
            {currentSpace ? currentSpace.space_id : t('query.spaceUnknown')}
          </Badge>
          <Button size="sm" variant="ghost" onClick={startNewConversation} disabled={!conversationId}>
            {t('nav.newConversation')}
          </Button>
          <button
            type="button"
            onClick={onCollapse}
            aria-label={t('layout.collapseRight')}
            title={t('layout.collapseRight')}
            className="rounded p-0.5 text-fg-3 transition-colors hover:bg-raise hover:text-fg"
          >
            <svg viewBox="0 0 16 16" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
              <path d="M6 4l4 4-4 4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        </div>
      </div>

      <div ref={ref} onScroll={onScroll} className="thin-scroll min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
        {conversationLoading && <p className="px-1 text-[11.5px] text-fg-3">{t('state.loading')}</p>}

        {messages.map(({ message, citations, degraded, usage }) => {
          if (message.role === 'user') {
            return (
              <div key={message.message_id} className="flex justify-end">
                <div className="max-w-[92%] rounded-xl rounded-br-sm border border-accent-line bg-accent-weak px-3 py-2 text-[12.5px] leading-5 text-fg">
                  {message.content}
                </div>
              </div>
            )
          }
          return (
            <div key={message.message_id} className="rounded-xl border border-line bg-raise p-3">
              <p className="whitespace-pre-wrap text-[12.5px] leading-[1.8] text-fg">
                {message.content || t('query.answerEmpty')}
              </p>
              {message.rewritten_query && (
                <p className="mt-1.5 text-[10.5px] text-fg-3">
                  {t('query.rewritten')}: <code className="break-all">{message.rewritten_query}</code>
                </p>
              )}
              <DegradedNote items={degraded} />
              <CitationList citations={citations} />
              <AnswerMeta
                mode={t(modeLabelKey((message.mode as 'search' | 'answer' | 'both') ?? 'both'))}
                model={message.model ?? undefined}
                usage={usage}
              />
              <p className="mt-1 text-[10px] text-fg-3">{format.dateTime(message.created_at)}</p>
            </div>
          )
        })}

        {(stream.active || stream.text.length > 0) && (
          <div className="rounded-xl border border-accent-line bg-raise p-3">
            <div className="mb-1 flex items-center gap-1.5 text-[10px] uppercase tracking-[0.12em] text-fg-3">
              <span className="h-1.5 w-1.5 rounded-full bg-accent" />
              {stream.active ? t('query.streaming') : t('query.answer')}
            </div>
            <p className={cx('whitespace-pre-wrap text-[12.5px] leading-[1.8] text-fg', stream.active && 'streaming-caret')}>
              {stream.text || (stream.active ? '' : t('query.answerEmpty'))}
            </p>
            <DegradedNote items={stream.degraded} />
            <CitationList citations={stream.citations} />
            <AnswerMeta usage={stream.usage} rewrittenQuery={stream.rewrittenQuery} />
          </div>
        )}

        {askError !== null && <ErrorState error={askError} compact />}
        {searchError !== null && !searching && (
          <Banner tone="warn" title={t('query.searchFailed')}>
            <ErrorState error={searchError} compact />
          </Banner>
        )}

        {messages.length === 0 && !stream.active && stream.text.length === 0 && (
          <div className="rounded-xl border border-dashed border-line px-3 py-4">
            <p className="text-[12px] leading-5 text-fg-2">{t('query.panelSubtitle')}</p>
            <p className="mt-1.5 text-[11px] leading-5 text-fg-3">
              {currentSpace ? t('query.spaceScoped', { space: currentSpace.name }) : t('query.spaceUnknown')}
              {contentLang ? ` · lang=${contentLang}` : ''}
            </p>
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-line p-2.5">
        <div className="flex items-end gap-1.5 rounded-lg border border-line bg-surface px-2 py-1.5 transition-colors focus-within:border-accent-line">
          <input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault()
                submitAsk()
              }
            }}
            placeholder={t('query.askPlaceholder')}
            aria-label={t('query.panelTitle')}
            className="min-w-0 flex-1 border-0 bg-transparent p-0 text-[12.5px] text-fg outline-none placeholder:text-fg-3"
          />
          <Button
            size="sm"
            variant="ghost"
            onClick={submitSearch}
            disabled={draft.trim().length === 0}
            title={t('query.searchOnly')}
          >
            {t('query.searchOnly')}
          </Button>
          <Button size="sm" variant="primary" onClick={submitAsk} disabled={draft.trim().length === 0 || !spaceId}>
            {t('query.send')}
          </Button>
        </div>
        <div className="mt-1.5 flex items-center justify-between px-1 text-[10px] text-fg-3">
          <span>{t('query.askHint')}</span>
          {spaceId === null && <span className="text-warn">{t('query.noSpace')}</span>}
        </div>
      </div>
    </aside>
  )
}
