import { useState } from 'react'
import { useParams, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ApiError } from '../../api/client'
import { getSpace } from '../../api/spaces'
import { hasSpaceRole } from '../../api/types'
import { Banner, EmptyState, ErrorState, LoadingState, SectionTitle, Tabs, type TabItem } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { SpaceGeneralTab } from '../../features/spaces/SpaceGeneralTab'
import { SpaceMembersTab } from '../../features/spaces/SpaceMembersTab'
import { SpaceQuotaTab } from '../../features/spaces/SpaceQuotaTab'
import { SpaceRoleBadge, SpaceStatusBadge } from '../../features/spaces/badges'
import { spaceKeys } from '../../features/spaces/keys'

type SpaceTab = 'general' | 'members' | 'quota'

/** `/spaces/:spaceId` — overview / members / quota, tab kept in `?tab=`. */
export function SpaceDetailPage() {
  const { t } = useI18n()
  const { spaceId = '' } = useParams<{ spaceId: string }>()
  const [searchParams, setSearchParams] = useSearchParams()
  // The backend answers 404 once a space is deleted; keep the note visible.
  const [deletedFor, setDeletedFor] = useState<string | null>(null)

  const spaceQuery = useQuery({
    queryKey: spaceKeys.detail(spaceId),
    queryFn: () => getSpace(spaceId),
    retry: false,
  })

  const rawTab = searchParams.get('tab')
  const tab: SpaceTab = rawTab === 'members' || rawTab === 'quota' ? rawTab : 'general'

  const space = spaceQuery.data?.space
  const notFound = spaceQuery.error instanceof ApiError && spaceQuery.error.status === 404
  const deletedHere = deletedFor === spaceId

  if (!space || (notFound && !deletedHere)) {
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        {spaceQuery.isPending ? (
          <LoadingState />
        ) : notFound ? (
          <EmptyState title={t('state.notFound')} />
        ) : (
          <ErrorState error={spaceQuery.error} onRetry={() => void spaceQuery.refetch()} />
        )}
      </div>
    )
  }

  const status = deletedHere ? 'deleted' : space.status
  const canAdmin = hasSpaceRole(space.role, 'space_admin')
  const canOwner = hasSpaceRole(space.role, 'space_owner')

  const tabItems: TabItem<SpaceTab>[] = [
    { id: 'general', label: t('spaces.settings.tab.general') },
    { id: 'members', label: t('spaces.settings.tab.members') },
    { id: 'quota', label: t('spaces.settings.tab.quota') },
  ]

  const selectTab = (next: SpaceTab) => {
    const params = new URLSearchParams(searchParams)
    params.set('tab', next)
    setSearchParams(params, { replace: true })
  }

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <SectionTitle>
        <span className="inline-flex flex-wrap items-center gap-2">
          {space.name}
          <SpaceStatusBadge status={status} />
          <SpaceRoleBadge role={space.role} />
        </span>
      </SectionTitle>
      <div className="mt-1 font-mono text-[11.5px] text-fg-3">{space.space_id}</div>

      {status === 'archived' && (
        <div className="mt-3">
          <Banner tone="warn">{t('spaces.settings.archived')}</Banner>
        </div>
      )}
      {status === 'deleted' && (
        <div className="mt-3">
          <Banner tone="danger">{t('spaces.settings.deleted')}</Banner>
        </div>
      )}

      <div className="mt-4">
        <Tabs items={tabItems} active={tab} onChange={selectTab} />
      </div>

      <div className="mt-4">
        {tab === 'general' && (
          <SpaceGeneralTab
            key={spaceId}
            space={space}
            status={status}
            canAdmin={canAdmin}
            canOwner={canOwner}
            onDeleted={() => setDeletedFor(spaceId)}
          />
        )}
        {tab === 'members' && <SpaceMembersTab key={spaceId} spaceId={spaceId} role={space.role} active={status === 'active'} />}
        {tab === 'quota' && <SpaceQuotaTab key={spaceId} spaceId={spaceId} canManage={canAdmin} active={status === 'active'} />}
      </div>
    </div>
  )
}
