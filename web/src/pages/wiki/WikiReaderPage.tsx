import { useCallback, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { ApiError, encodePathSegment } from '../../api/client'
import { deletePage, getPage, restorePage } from '../../api/wiki'
import { hasSpaceRole } from '../../api/types'
import { Badge, Banner, Button, EmptyState, ErrorState, LoadingState, SectionTitle } from '../../components/ui'
import { PageSummaryCard } from '../../features/wiki/PageSummaryCard'
import { useQueryState } from '../../features/query/QueryProvider'
import { useI18n } from '../../lib/i18n'
import { annotateHeadings, highlightPassage } from '../../lib/markdown'

function MetaItem({ label, value }: { label: string; value: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="text-[10.5px] uppercase tracking-[0.1em] text-fg-3">{label}</span>
      <span className="text-[12px] text-fg-2">{value}</span>
    </span>
  )
}

/**
 * Reader. Content and metadata come from `GET /api/v1/pages/{space}/{slug}`
 * (the backend renders and sanitises the HTML); the static summary sits above
 * the article. Owners can soft-delete and restore the page in place.
 */
export function WikiReaderPage() {
  const { space = '', slug = '' } = useParams<{ space: string; slug: string }>()
  const [searchParams] = useSearchParams()
  const lang = searchParams.get('lang')
  const highlight = searchParams.get('hl')
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { t, format } = useI18n()
  const { readableSpaces } = useQueryState()
  const [highlightState, setHighlightState] = useState<'none' | 'found' | 'missing'>('none')
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [archived, setArchived] = useState(false)
  const articleRef = useRef<HTMLElement | null>(null)

  const role = readableSpaces.find((entry) => entry.space_id === space)?.role ?? null
  const canEdit = hasSpaceRole(role, 'editor')
  const canAdminister = hasSpaceRole(role, 'space_owner')

  const pageQuery = useQuery({
    queryKey: ['wiki', 'page', space, slug, lang],
    queryFn: () => getPage(space, slug, lang),
    enabled: space !== '' && slug !== '' && !archived,
    retry: false,
  })

  const invalidateIndex = useCallback(() => {
    // The sidebar tree is the `['pages']` query (see PageTree).
    void queryClient.invalidateQueries({ queryKey: ['pages'] })
  }, [queryClient])

  const deleteMutation = useMutation({
    mutationFn: () => deletePage(space, slug, lang),
    onSuccess: () => {
      setArchived(true)
      setConfirmDelete(false)
      invalidateIndex()
    },
  })

  const restoreMutation = useMutation({
    mutationFn: () => restorePage(space, slug, lang),
    onSuccess: () => {
      setArchived(false)
      invalidateIndex()
      void pageQuery.refetch()
    },
  })

  const annotated = useMemo(() => (pageQuery.data ? annotateHeadings(pageQuery.data.html) : null), [pageQuery.data])

  // Highlight the citation passage after every render of new content.
  useLayoutEffect(() => {
    setHighlightState('none')
    if (!highlight || !annotated || !articleRef.current) return
    const found = highlightPassage(articleRef.current, highlight)
    setHighlightState(found ? 'found' : 'missing')
    if (found) {
      articleRef.current.querySelector('.lkb-hit')?.scrollIntoView({ block: 'center' })
    }
  }, [highlight, annotated])

  if (archived) {
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        <SectionTitle>{slug}</SectionTitle>
        <div className="mt-4 space-y-3">
          <Banner
            tone="warn"
            title={t('wiki.reader.deleted')}
            actions={
              <Button
                variant="primary"
                onClick={() => restoreMutation.mutate()}
                disabled={restoreMutation.isPending}
              >
                {t('wiki.reader.restore')}
              </Button>
            }
          >
            {t('wiki.reader.deletedHint')}
          </Banner>
          {restoreMutation.error && <ErrorState error={restoreMutation.error} />}
          <Button variant="ghost" onClick={() => navigate('/search')}>
            {t('action.back')}
          </Button>
        </div>
      </div>
    )
  }

  if (pageQuery.isLoading) {
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        <LoadingState />
      </div>
    )
  }

  if (pageQuery.isError) {
    const error = pageQuery.error
    if (error instanceof ApiError && error.status === 404) {
      return (
        <div className="mx-auto w-full max-w-[900px] px-6 py-5">
          <EmptyState
            title={t('wiki.reader.notFound')}
            description={`${space}/${slug}${lang ? ` · lang=${lang}` : ''}`}
            actions={
              canEdit ? (
                <Link to={`/create/${encodePathSegment(space)}?slug=${encodePathSegment(slug)}${lang ? `&lang=${lang}` : ''}`}>
                  <Button variant="primary">{t('wiki.editor.newPage')}</Button>
                </Link>
              ) : undefined
            }
          />
        </div>
      )
    }
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        <ErrorState error={error} onRetry={() => void pageQuery.refetch()} />
      </div>
    )
  }

  const page = pageQuery.data
  if (!page) return null

  return (
    <div className="mx-auto flex w-full max-w-[1180px] gap-8 px-6 py-5">
      <div className="min-w-0 flex-1">
        <SectionTitle
          actions={
            <>
              <Badge tone="accent">JSON API</Badge>
              {canEdit && (
                <Link
                  to={`/w/${encodePathSegment(space)}/${encodePathSegment(slug)}/edit${lang ? `?lang=${lang}` : ''}`}
                  className="rounded-lg border border-line px-2.5 py-1 text-[11.5px] text-fg-2 transition-colors hover:text-fg"
                >
                  {t('wiki.reader.edit')}
                </Link>
              )}
              <Link
                to={`/w/${encodePathSegment(space)}/${encodePathSegment(slug)}/history${lang ? `?lang=${lang}` : ''}`}
                className="rounded-lg border border-line px-2.5 py-1 text-[11.5px] text-fg-2 transition-colors hover:text-fg"
              >
                {t('wiki.reader.history')}
              </Link>
              {canAdminister && (
                <Button variant="danger" size="sm" onClick={() => setConfirmDelete(true)}>
                  {t('action.delete')}
                </Button>
              )}
            </>
          }
        >
          {page.title}
        </SectionTitle>

        <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
          <MetaItem label={t('wiki.reader.meta.space')} value={page.space} />
          <MetaItem label={t('wiki.reader.meta.lang')} value={page.lang} />
          <MetaItem label={t('wiki.reader.meta.version')} value={format.number(page.version)} />
          <MetaItem label={t('wiki.reader.meta.author')} value={page.author} />
          <MetaItem label={t('wiki.reader.meta.updatedAt')} value={format.dateTime(page.updated_at)} />
        </div>

        {(highlightState !== 'none' || deleteMutation.error || confirmDelete) && (
          <div className="mt-3 space-y-2">
            {deleteMutation.error && <ErrorState error={deleteMutation.error} compact />}
            {confirmDelete && (
              <Banner
                tone="warn"
                title={t('wiki.reader.delete')}
                actions={
                  <>
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={() => deleteMutation.mutate()}
                      disabled={deleteMutation.isPending}
                    >
                      {t('action.confirm')}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setConfirmDelete(false)}>
                      {t('action.cancel')}
                    </Button>
                  </>
                }
              >
                {t('wiki.reader.deleteConfirm')}
              </Banner>
            )}
            {highlightState === 'found' && <Banner tone="accent">{t('wiki.reader.highlighted')}</Banner>}
            {highlightState === 'missing' && <Banner tone="warn">{t('wiki.reader.highlightMissing')}</Banner>}
          </div>
        )}

        <div className="mt-4">
          <PageSummaryCard space={space} slug={slug} lang={page.lang} revisionId={page.current_revision_id} />
        </div>

        <article
          ref={articleRef}
          className="prose-lkb mt-5"
          dangerouslySetInnerHTML={{ __html: annotated?.html ?? page.html }}
        />
      </div>

      {annotated && annotated.toc.length > 0 && (
        <aside className="hidden w-56 shrink-0 xl:block">
          <div className="sticky top-5">
            <div className="mb-2 text-[10.5px] font-medium uppercase tracking-[0.12em] text-fg-3">
              {t('wiki.reader.toc')}
            </div>
            <ul className="space-y-0.5">
              {annotated.toc.map((entry) => (
                <li key={entry.id} style={{ paddingLeft: `${(entry.level - 1) * 10}px` }}>
                  <a
                    href={`#${entry.id}`}
                    className="block truncate rounded-md px-1.5 py-[3px] text-[11.5px] text-fg-3 transition-colors hover:bg-panel hover:text-fg"
                  >
                    {entry.text}
                  </a>
                </li>
              ))}
            </ul>
          </div>
        </aside>
      )}
    </div>
  )
}
