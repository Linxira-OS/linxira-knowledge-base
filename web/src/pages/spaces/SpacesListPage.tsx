import { Link, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { encodePathSegment } from '../../api/client'
import { listSpaces } from '../../api/spaces'
import {
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
} from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { SpaceCreateCard } from '../../features/spaces/SpaceCreateCard'
import { spacePrincipalLabelKey, SpaceRoleBadge, SpaceStatusBadge, SpaceVisibilityBadge } from '../../features/spaces/badges'
import { spaceKeys } from '../../features/spaces/keys'

/** `/spaces` — accessible spaces plus self-service creation. */
export function SpacesListPage() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { features } = useSession()
  const solo = features?.mode === 'solo'

  const spacesQuery = useQuery({ queryKey: spaceKeys.list, queryFn: listSpaces })
  const spaces = spacesQuery.data?.spaces ?? []

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <SectionTitle>{solo ? t('spaces.solo.title') : t('spaces.title')}</SectionTitle>
      <p className="mt-1 text-[11.5px] text-fg-3">{solo ? t('spaces.solo.subtitle') : t('spaces.subtitle')}</p>

      <div className="mt-4">
        <SpaceCreateCard />
      </div>

      <div className="mt-4">
        {spacesQuery.isPending ? (
          <LoadingState />
        ) : spacesQuery.isError ? (
          <ErrorState error={spacesQuery.error} onRetry={() => void spacesQuery.refetch()} />
        ) : spaces.length === 0 ? (
          <EmptyState title={solo ? t('spaces.solo.empty') : t('spaces.empty')} description={solo ? t('spaces.solo.emptyHint') : undefined} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('label.name')}</TH>
                  <TH>{t('spaces.col.id')}</TH>
                  <TH>{t('spaces.visibility')}</TH>
                  <TH>{t('spaces.owner')}</TH>
                  <TH>{t('spaces.status')}</TH>
                  <TH>{t('spaces.myRole')}</TH>
                </TR>
              </THead>
              <TBody>
                {spaces.map((space) => {
                  const ownerKey = spacePrincipalLabelKey(space.owner_type)
                  const path = `/spaces/${encodePathSegment(space.space_id)}`
                  return (
                    <tr
                      key={space.space_id}
                      className="cursor-pointer align-top transition-colors hover:bg-panel"
                      onClick={() => navigate(path)}
                    >
                      <TD>
                        <Link
                          to={path}
                          className="font-medium text-fg hover:text-accent"
                          onClick={(event) => event.stopPropagation()}
                        >
                          {space.name}
                        </Link>
                      </TD>
                      <TD>
                        <code className="font-mono text-[11.5px] text-fg-2">{space.space_id}</code>
                      </TD>
                      <TD>
                        <SpaceVisibilityBadge visibility={space.visibility} />
                      </TD>
                      <TD>
                        <div className="flex flex-col">
                          <span className="text-[10.5px] uppercase tracking-[0.08em] text-fg-3">
                            {ownerKey ? t(ownerKey) : space.owner_type}
                          </span>
                          <code className="font-mono text-[11.5px] text-fg-2">{space.owner_id}</code>
                        </div>
                      </TD>
                      <TD>
                        <SpaceStatusBadge status={space.status} />
                      </TD>
                      <TD>
                        <SpaceRoleBadge role={space.role} />
                      </TD>
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
