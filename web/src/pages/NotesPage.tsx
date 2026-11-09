import { Link, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { encodePathSegment } from '../api/client'
import { ensureMyNotesSpace } from '../api/notes'
import { listPages } from '../api/wiki'
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  ErrorState,
  LoadingState,
  SectionTitle,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from '../components/ui'
import { useQueryState } from '../features/query/QueryProvider'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'

/**
 * `/notes` — the signed-in user's private notes.
 *
 * The backend ensures a per-user private space (`notes_<principal>`) via
 * `POST /api/v1/notes/space`; pages inside it are the notes. While that
 * endpoint is not deployed (404), the page falls back to the currently
 * selected space and says so explicitly. Every tier uses the same flow.
 */
export function NotesPage() {
  const { t, format } = useI18n()
  const session = useSession()
  const navigate = useNavigate()
  const { currentSpace, readableSpaces } = useQueryState()

  const notesSpaceQuery = useQuery({
    // The principal is part of the key so a different account can never read
    // the previous account's cached private space after an in-app login.
    queryKey: ['notes', 'space', session.principal?.user_id ?? 'anonymous'],
    queryFn: ensureMyNotesSpace,
    staleTime: 60_000,
    retry: false,
  })

  const privateSpace = notesSpaceQuery.data?.space ?? null
  const fallback = notesSpaceQuery.isError
  const fallbackSpace = currentSpace ?? readableSpaces[0] ?? null
  const space = privateSpace ?? (fallback ? fallbackSpace : null)

  const pagesQuery = useQuery({
    queryKey: ['notes', 'pages', space?.space_id ?? 'none'],
    queryFn: () => listPages({ space: space?.space_id ?? '', limit: 50 }),
    enabled: space !== null,
    retry: false,
  })

  const pages = pagesQuery.data?.pages ?? []
  const newNotePath = space ? `/create/${encodePathSegment(space.space_id)}` : null
  const subtitle = privateSpace
    ? t('notes.privateSubtitle')
    : fallback
      ? fallbackSpace
        ? t('notes.fallbackSubtitle', { space: fallbackSpace.name })
        : t('notes.noSpaceHint')
      : t('notes.privateSubtitle')

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <SectionTitle
        actions={
          <div className="flex items-center gap-1.5">
            <Button size="sm" onClick={() => navigate('/spaces')}>
              {t('notes.manageSpaces')}
            </Button>
            {newNotePath && (
              <Button size="sm" variant="primary" onClick={() => navigate(newNotePath)}>
                {t('notes.new')}
              </Button>
            )}
          </div>
        }
      >
        {t('notes.title')}
      </SectionTitle>
      <p className="mt-1 text-[11.5px] text-fg-3">{subtitle}</p>
      {privateSpace && (
        <div className="mt-1.5">
          <Badge tone="accent">{t('notes.privateSpace', { space: privateSpace.name })}</Badge>
        </div>
      )}

      {fallback && (
        <div className="mt-3">
          <Banner
            tone="warn"
            title={fallbackSpace ? t('notes.fallbackBanner') : t('notes.fallbackNoSpace')}
            actions={
              <Button
                size="sm"
                disabled={notesSpaceQuery.isFetching}
                onClick={() => void notesSpaceQuery.refetch()}
              >
                {t('action.retry')}
              </Button>
            }
          >
            {fallbackSpace ? t('notes.fallbackHint') : t('notes.fallbackHintNoSpace')}
          </Banner>
        </div>
      )}

      <div className="mt-4">
        {notesSpaceQuery.isPending ? (
          <LoadingState />
        ) : !space ? (
          <EmptyState
            title={t('notes.noSpace')}
            description={t('notes.noSpaceHint')}
            actions={
              <Button size="sm" variant="primary" onClick={() => navigate('/spaces')}>
                {t('notes.manageSpaces')}
              </Button>
            }
          />
        ) : pagesQuery.isPending ? (
          <LoadingState />
        ) : pagesQuery.isError ? (
          <ErrorState error={pagesQuery.error} onRetry={() => void pagesQuery.refetch()} />
        ) : pages.length === 0 ? (
          <EmptyState
            title={t('notes.empty')}
            description={fallback ? t('notes.emptyHintFallback') : t('notes.emptyHint')}
            actions={
              newNotePath && (
                <Button size="sm" variant="primary" onClick={() => navigate(newNotePath)}>
                  {t('notes.new')}
                </Button>
              )
            }
          />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('notes.col.title')}</TH>
                  <TH>{t('notes.col.summary')}</TH>
                  <TH>{t('notes.col.version')}</TH>
                  <TH>{t('notes.col.updated')}</TH>
                </TR>
              </THead>
              <TBody>
                {pages.map((page) => {
                  const path = `/w/${encodePathSegment(space.space_id)}/${encodePathSegment(page.slug)}?lang=${encodePathSegment(page.lang)}`
                  return (
                    <tr
                      key={`${page.lang}-${page.slug}`}
                      className="cursor-pointer align-top transition-colors hover:bg-panel"
                      onClick={() => navigate(path)}
                    >
                      <TD>
                        <Link
                          to={path}
                          className="font-medium text-fg hover:text-accent"
                          onClick={(event) => event.stopPropagation()}
                        >
                          {page.title || page.slug}
                        </Link>
                        <div className="mt-0.5 font-mono text-[10.5px] text-fg-3">{page.slug}</div>
                      </TD>
                      <TD className="max-w-[360px]">
                        <span className="line-clamp-2 text-[11.5px] leading-5 text-fg-2">
                          {page.summary || t('notes.noSummary')}
                        </span>
                      </TD>
                      <TD className="whitespace-nowrap font-mono text-[11.5px] text-fg-2">v{page.version}</TD>
                      <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(page.updated_at)}</TD>
                    </tr>
                  )
                })}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </div>
    </div>
  )
}
