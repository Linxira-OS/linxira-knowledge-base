import { useQuery } from '@tanstack/react-query'
import { ApiError } from '../../api/client'
import { getPageSummary } from '../../api/wiki'
import { Badge, Banner, Card, ErrorState, LoadingState } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

const REVISION_DISPLAY_LENGTH = 8

/**
 * Static article summary (`GET /api/v1/pages/{space}/{slug}/summary`).
 *
 * The payload is backend-generated and cached per revision; it is never
 * presented as a live answer (the “static summary” badge is always shown).
 */
export function PageSummaryCard({
  space,
  slug,
  lang,
  revisionId,
}: {
  space: string
  slug: string
  lang: string | null
  revisionId?: string | null
}) {
  const { t, format } = useI18n()

  const summaryQuery = useQuery({
    queryKey: ['wiki', 'summary', space, slug, lang],
    queryFn: () => getPageSummary(space, slug, lang),
  })

  const title = (
    <span className="flex flex-wrap items-center gap-2">
      {t('wiki.summary.title')}
      <Badge tone="neutral">{t('wiki.summary.static')}</Badge>
    </span>
  )

  if (summaryQuery.isLoading) {
    return (
      <Card title={title}>
        <LoadingState />
      </Card>
    )
  }

  if (summaryQuery.isError) {
    // 404 = the summary route/capability does not exist for this page → stay silent.
    if (summaryQuery.error instanceof ApiError && summaryQuery.error.status === 404) return null
    return <ErrorState compact error={summaryQuery.error} onRetry={() => void summaryQuery.refetch()} />
  }

  const data = summaryQuery.data
  if (!data) return null

  // The wire union is discriminated by `status`/`summary`: the pending variant
  // has `status: 'pending'` and no summary payload.
  if ('status' in data || data.summary === null) {
    return (
      <Card title={title}>
        <Banner tone="neutral">{t('wiki.summary.pending')}</Banner>
      </Card>
    )
  }

  const summary = data.summary
  const payload = summary.summary
  const extractive = summary.model === 'extractive' || payload?.fallback === true
  // `model_unavailable` means no model is configured at all: a designed
  // extractive fallback, not a generation bug — render it neutrally.
  const noModel = summary.status === 'failed' && /model_unavailable/.test(summary.error ?? '')
  const shortRevision = (summary.revision_id || revisionId || '').slice(0, REVISION_DISPLAY_LENGTH)
  const bullets = payload?.bullets ?? []
  const keywords = payload?.keywords ?? []
  const entities = payload?.entities ?? []

  return (
    <Card
      title={
        <span className="flex flex-wrap items-center gap-2">
          {t('wiki.summary.title')}
          <Badge tone="neutral">{t('wiki.summary.static')}</Badge>
          <Badge tone={noModel ? 'neutral' : extractive ? 'warn' : 'accent'}>
            {t(noModel ? 'wiki.summary.noModel' : extractive ? 'wiki.summary.source.extractive' : 'wiki.summary.source.model')}
          </Badge>
        </span>
      }
    >
      <div className="space-y-2">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[10.5px] text-fg-3">
          {shortRevision !== '' && <span>{t('wiki.summary.revision', { revision: shortRevision })}</span>}
          <span>
            {t('wiki.summary.model')}: <code className="text-fg-2">{summary.model}</code>
          </span>
          <span>
            {t('wiki.summary.promptVersion')}: <code className="text-fg-2">{summary.prompt_version}</code>
          </span>
          <span>
            {t('wiki.summary.generatedAt')}: {format.dateTime(summary.generated_at)}
          </span>
          <span>
            {t('wiki.summary.lang')}: <code className="text-fg-2">{summary.lang}</code>
          </span>
        </div>

        {summary.status === 'failed' &&
          (noModel ? (
            <Banner tone="neutral" title={t('wiki.summary.noModel')}>
              {t('wiki.summary.noModelHint')}
            </Banner>
          ) : (
            <Banner tone="warn" title={t('wiki.summary.realFailure')}>
              {t('wiki.summary.error')}: {summary.error ?? t('label.none')}
            </Banner>
          ))}
        {extractive && !noModel && <Banner tone="warn">{t('wiki.summary.fallbackNotice')}</Banner>}

        {payload === null ? (
          summary.status === 'ready' && <Banner tone="neutral">{t('wiki.summary.empty')}</Banner>
        ) : (
          <div className="space-y-2">
            {payload.tldr && <p className="text-[13.5px] leading-6 text-fg">{payload.tldr}</p>}
            {bullets.length > 0 && (
              <div>
                <div className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{t('wiki.summary.bullets')}</div>
                <ul className="mt-1 list-disc space-y-0.5 pl-4 text-[12.5px] leading-5 text-fg-2">
                  {bullets.map((bullet, index) => (
                    <li key={index}>{bullet}</li>
                  ))}
                </ul>
              </div>
            )}
            {keywords.length > 0 && (
              <div>
                <div className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{t('wiki.summary.keywords')}</div>
                <div className="mt-1 flex flex-wrap gap-1">
                  {keywords.map((keyword) => (
                    <span
                      key={keyword}
                      className="inline-flex items-center rounded-md border border-line bg-panel px-1.5 py-[2px] text-[10.5px] leading-none text-fg-2"
                    >
                      {keyword}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {entities.length > 0 && (
              <div>
                <div className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{t('wiki.summary.entities')}</div>
                <div className="mt-1 flex flex-wrap gap-1">
                  {entities.map((entity, index) => {
                    const label = [entity.name, entity.type].filter(Boolean).join(' · ')
                    if (!label) return null
                    return (
                      <span
                        key={index}
                        className="inline-flex items-center rounded-md border border-line bg-panel px-1.5 py-[2px] text-[10.5px] leading-none text-fg-2"
                      >
                        {label}
                      </span>
                    )
                  })}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </Card>
  )
}
