import { Link } from 'react-router-dom'
import type { AnswerUsage, Citation, Degraded, SearchHit, SearchResponse } from '../../api/types'
import { hitReaderPath } from '../../api/wiki'
import { Badge, DegradedList, cx } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

/** Result list shared by the QA panel and the search page. */
export function HitList({ results, compact }: { results: SearchResponse; compact?: boolean }) {
  const { t } = useI18n()
  if (results.hits.length === 0) {
    return (
      <div className="px-1 py-3">
        <p className="text-[12.5px] text-fg-2">{t('query.resultsEmpty')}</p>
        <p className="mt-0.5 text-[11px] text-fg-3">{t('query.resultsEmptyHint')}</p>
      </div>
    )
  }
  return (
    <ul className={cx('divide-y divide-line', compact && 'rounded-xl border border-line bg-surface')}>
      {results.hits.map((hit) => (
        <HitRow key={hit.chunk_id} hit={hit} />
      ))}
    </ul>
  )
}

export function HitRow({ hit }: { hit: SearchHit }) {
  const { t, format } = useI18n()
  const readerPath = hitReaderPath(hit)
  const jumpable = readerPath !== null
  return (
    <li className="px-1 py-2.5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="truncate text-[12.5px] font-medium text-fg">{hit.title}</span>
            <Badge>{t('query.resultDocType')}</Badge>
            <span className="font-mono text-[10px] text-fg-3">
              {t('query.lines', { start: hit.line_start, end: hit.line_end })}
            </span>
          </div>
          <p className="mt-1 line-clamp-3 text-[11.5px] leading-5 text-fg-2">{hit.quote}</p>
          <p className="mt-0.5 truncate font-mono text-[10px] text-fg-3">{hit.path}</p>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <span className="text-[10px] text-fg-3">{t('query.score', { score: format.decimal(hit.score) })}</span>
          {jumpable && (
            <Link
              to={readerPath}
              className="rounded-md border border-line px-1.5 py-[2px] text-[10.5px] text-fg-2 transition-colors hover:border-line-strong hover:text-fg"
            >
              {t('query.citationJump')}
            </Link>
          )}
        </div>
      </div>
    </li>
  )
}

export function CitationList({ citations }: { citations: Citation[] }) {
  const { t } = useI18n()
  if (citations.length === 0) return null
  return (
    <div className="mt-2.5 border-t border-line pt-2">
      <div className="mb-1 flex items-center gap-2 text-[10px] font-medium uppercase tracking-[0.12em] text-fg-3">
        {t('query.citations')}
        <span className="normal-case tracking-normal">{t('query.citationsCount', { count: citations.length })}</span>
      </div>
      <ul className="space-y-0.5">
        {citations.map((citation, index) => (
          <CitationRow key={`${citation.chunk_id || citation.doc_id}-${index}`} citation={citation} index={index} />
        ))}
      </ul>
    </div>
  )
}

function CitationRow({ citation, index }: { citation: Citation; index: number }) {
  const { t } = useI18n()
  const readerPath = hitReaderPath(citation)
  const body = (
    <>
      <span className="mt-px grid h-4 w-4 shrink-0 place-items-center rounded-md border border-line text-[9px] text-fg-3 group-hover:border-accent-line group-hover:text-accent">
        {index + 1}
      </span>
      <span className="min-w-0">
        <span className="block truncate font-mono text-[10.5px] text-fg-3">{citation.path}</span>
        <span className="line-clamp-2 text-[11px] leading-4 text-fg-2">{citation.quote}</span>
        <span className="mt-0.5 block text-[10px] text-fg-3">
          {t('query.lines', { start: citation.line_start, end: citation.line_end })}
          {readerPath === null && <> · {t('query.citationNoPage')}</>}
        </span>
      </span>
    </>
  )
  if (readerPath === null) {
    return <li className="flex items-start gap-1.5 px-1.5 py-1 opacity-80">{body}</li>
  }
  return (
    <li>
      <Link
        to={readerPath}
        title={t('query.citationJump')}
        className="group flex items-start gap-1.5 rounded-lg px-1.5 py-1 transition-colors hover:bg-panel"
      >
        {body}
      </Link>
    </li>
  )
}

export function AnswerMeta({
  mode,
  model,
  usage,
  knowledgeAsOf,
  confidence,
  rewrittenQuery,
  cacheHit,
}: {
  mode?: string
  model?: string
  usage?: AnswerUsage | null
  knowledgeAsOf?: string | null
  confidence?: { score: number; band: string }
  rewrittenQuery?: string | null
  cacheHit?: boolean
}) {
  const { t, format } = useI18n()
  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10.5px] text-fg-3">
      {mode && <span>{mode}</span>}
      {model && <span>{t('query.model')}: <code>{model}</code></span>}
      {confidence && (
        <span>
          {t('query.confidence')}: {t(confidenceBandKey(confidence.band))} ({format.decimal(confidence.score)})
        </span>
      )}
      {usage && (
        <span>
          {t('query.usageTokens', { input: format.number(usage.input_tokens), output: format.number(usage.output_tokens) })}
          {usage.cost_micros > 0 && <> · {t('query.usageCost', { cost: format.currency(usage.cost_micros, usage.currency) })}</>}
        </span>
      )}
      {knowledgeAsOf && <span>{t('query.knowledgeAsOf')}: {format.dateTime(knowledgeAsOf)}</span>}
      {cacheHit && <span>{t('query.cacheHit')}</span>}
      {rewrittenQuery && (
        <span className="min-w-0">
          {t('query.rewritten')}: <code className="break-all">{rewrittenQuery}</code>
        </span>
      )}
    </div>
  )
}

export function DegradedNote({ items }: { items: Degraded[] }) {
  return <DegradedList items={items} />
}

function confidenceBandKey(band: string): 'query.confidence.high' | 'query.confidence.medium' | 'query.confidence.low' {
  switch (band) {
    case 'high':
      return 'query.confidence.high'
    case 'medium':
      return 'query.confidence.medium'
    default:
      return 'query.confidence.low'
  }
}
