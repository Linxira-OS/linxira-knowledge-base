import { useMemo, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { encodePathSegment } from '../../api/client'
import { getPage, getPageHistory, getRevision, type WikiRevision } from '../../api/wiki'
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorState,
  LoadingState,
  NotProvided,
  SectionTitle,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from '../../components/ui'
import { DiffView } from '../../features/wiki/DiffView'
import { useI18n } from '../../lib/i18n'

const HASH_DISPLAY_LENGTH = 16

/**
 * Revision history. The list is slim by default; `include_markdown=true` asks
 * for bodies, and a single revision body can also be fetched on demand, so the
 * diff compares stored content whenever the backend exposes it — otherwise the
 * view states that no real diff can be produced.
 */
export function WikiHistoryPage() {
  const { space = '', slug = '' } = useParams<{ space: string; slug: string }>()
  const [searchParams] = useSearchParams()
  const lang = searchParams.get('lang')
  const { t, format } = useI18n()
  const [selectedRevisionId, setSelectedRevisionId] = useState<string | null>(null)

  const historyQuery = useQuery({
    queryKey: ['wiki', 'history', space, slug, lang, 'bodies'],
    queryFn: () => getPageHistory(space, slug, lang, true),
    retry: false,
  })

  const revisions = useMemo<WikiRevision[]>(
    () => [...(historyQuery.data ?? [])].sort((a, b) => b.version - a.version),
    [historyQuery.data],
  )
  const current = revisions[0] ?? null
  const selected =
    revisions.find((revision) => revision.revision_id === selectedRevisionId) ??
    revisions.find((revision) => revision.revision_id !== current?.revision_id) ??
    current

  // Fallbacks: bodies may be missing from the list, so fetch them individually.
  const selectedBodyQuery = useQuery({
    queryKey: ['wiki', 'revision', space, slug, selected?.revision_id],
    queryFn: () => getRevision(space, slug, (selected as WikiRevision).revision_id),
    enabled: Boolean(selected && (selected.markdown ?? '').length === 0),
    retry: false,
  })
  const currentBodyQuery = useQuery({
    queryKey: ['wiki', 'current-body', space, slug, lang],
    queryFn: () => getPage(space, slug, lang),
    enabled: Boolean(current && (current.markdown ?? '').length === 0),
    retry: false,
  })

  const before = selected?.markdown ?? selectedBodyQuery.data?.markdown ?? ''
  const after = current?.markdown ?? currentBodyQuery.data?.markdown ?? ''
  const bodiesAvailable = before.length > 0 && after.length > 0

  return (
    <div className="mx-auto w-full max-w-[1000px] px-6 py-5">
      <SectionTitle
        actions={
          <Link
            to={`/w/${encodePathSegment(space)}/${encodePathSegment(slug)}${lang ? `?lang=${lang}` : ''}`}
            className="rounded-lg border border-line px-2.5 py-1 text-[11.5px] text-fg-2 transition-colors hover:text-fg"
          >
            {t('wiki.back')}
          </Link>
        }
      >
        {t('wiki.history.title')}
      </SectionTitle>

      <p className="mt-1 text-[11.5px] text-fg-3">
        {space}/{slug}
        {lang ? ` · lang=${lang}` : ''} · {t('wiki.history.currentNote')}
      </p>

      <div className="mt-4 space-y-4">
        {historyQuery.isLoading && <LoadingState />}
        {historyQuery.isError && <ErrorState error={historyQuery.error} onRetry={() => void historyQuery.refetch()} />}

        {historyQuery.data && revisions.length === 0 && <EmptyState title={t('wiki.history.empty')} />}

        {revisions.length > 0 && (
          <Card title={t('wiki.history.selectRevision')} bodyClassName="p-0">
            <TableWrap className="border-0">
              <Table>
                <THead>
                  <TR>
                    <TH>{t('wiki.history.version')}</TH>
                    <TH>{t('wiki.history.author')}</TH>
                    <TH>{t('wiki.history.time')}</TH>
                    <TH>{t('wiki.history.hash')}</TH>
                    <TH>{t('label.actions')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {revisions.map((revision) => {
                    const isCurrent = revision.revision_id === current?.revision_id
                    const isSelected = revision.revision_id === selected?.revision_id
                    return (
                      <TR key={revision.revision_id} className={isSelected ? 'bg-accent-weak' : undefined}>
                        <TD className="font-mono text-fg">
                          {format.number(revision.version)}
                          {isCurrent && (
                            <Badge tone="accent" className="ml-2">
                              {t('wiki.history.current')}
                            </Badge>
                          )}
                        </TD>
                        <TD>{revision.author}</TD>
                        <TD>{format.dateTime(revision.created_at)}</TD>
                        <TD className="font-mono text-[11px]">{revision.content_hash.slice(0, HASH_DISPLAY_LENGTH)}…</TD>
                        <TD>
                          {!isCurrent && (
                            <Button size="sm" variant="ghost" onClick={() => setSelectedRevisionId(revision.revision_id)}>
                              {t('wiki.history.diffTitle')}
                            </Button>
                          )}
                        </TD>
                      </TR>
                    )
                  })}
                </TBody>
              </Table>
            </TableWrap>
          </Card>
        )}

        {selected && current && revisions.length > 1 && (
          <Card
            title={t('wiki.history.diffTitle')}
            description={t('wiki.history.diffAgainst', {
              from: `v${format.number(selected.version)}`,
              to: `v${format.number(current.version)}`,
            })}
          >
            {bodiesAvailable ? (
              <DiffView before={before} after={after} />
            ) : selectedBodyQuery.isLoading || currentBodyQuery.isLoading ? (
              <LoadingState />
            ) : (
              <NotProvided
                code="GET /api/v1/pages/{space}/{slug}/revisions/{revision_id}"
                title={t('wiki.history.diffTitle')}
                body={t('wiki.history.bodiesUnavailable')}
              />
            )}
          </Card>
        )}
      </div>
    </div>
  )
}
