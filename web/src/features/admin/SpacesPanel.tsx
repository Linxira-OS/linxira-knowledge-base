import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { listAdminSpaces } from '../../api/admin'
import { encodePathSegment } from '../../api/client'
import { Badge, EmptyState, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, TextInput, adminKeys } from './shared'

export function SpacesPanel() {
  const { t, format } = useI18n()
  const [filter, setFilter] = useState('')
  const query = useQuery({ queryKey: adminKeys.spaces, queryFn: listAdminSpaces })
  const spaces = query.data?.spaces ?? []
  const needle = filter.trim().toLowerCase()
  const visible =
    needle === ''
      ? spaces
      : spaces.filter(
          (space) =>
            space.space_id.toLowerCase().includes(needle) || space.name.toLowerCase().includes(needle),
        )

  return (
    <div className="space-y-3">
      <p className="text-[11.5px] text-fg-3">{t('admin.spaces.mutationsHint')}</p>
      <div className="sm:max-w-[320px]">
        <TextInput
          label={t('admin.spaces.filter')}
          value={filter}
          onChange={setFilter}
          hint={t('admin.spaces.filterHint')}
        />
      </div>
      <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        {visible.length === 0 ? (
          <EmptyState title={t('admin.spaces.empty')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('admin.spaces.spaceId')}</TH>
                  <TH>{t('label.name')}</TH>
                  <TH>{t('admin.spaces.visibility')}</TH>
                  <TH>{t('admin.spaces.owner')}</TH>
                  <TH>{t('label.status')}</TH>
                  <TH>{t('label.createdAt')}</TH>
                  <TH>{t('label.actions')}</TH>
                </TR>
              </THead>
              <TBody>
                {spaces.map((space) => {
                  const base = `/spaces/${encodePathSegment(space.space_id)}`
                  return (
                    <TR key={space.space_id}>
                      <TD className="font-mono text-[11.5px]">
                        <Link className="text-accent hover:underline" to={base}>
                          {space.space_id}
                        </Link>
                      </TD>
                      <TD className="text-fg">{space.name}</TD>
                      <TD>{space.visibility}</TD>
                      <TD className="font-mono text-[11.5px]">
                        {space.owner_type}/{space.owner_id}
                      </TD>
                      <TD>
                        <Badge tone={statusTone(space.status)}>{space.status}</Badge>
                      </TD>
                      <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(space.created_at)}</TD>
                      <TD>
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[11.5px]">
                          <Link className="text-accent" to={base}>
                            {t('admin.spaces.manage')}
                          </Link>
                          <Link className="text-accent" to={`${base}?tab=members`}>
                            {t('admin.spaces.membersLink')}
                          </Link>
                          <Link className="text-accent" to={`${base}?tab=quota`}>
                            {t('admin.spaces.quotaLink')}
                          </Link>
                          <Link className="text-accent" to={`/admin/audit?space=${encodeURIComponent(space.space_id)}`}>
                            {t('admin.row.audit')}
                          </Link>
                        </div>
                      </TD>
                    </TR>
                  )
                })}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </AdminQueryGate>
    </div>
  )
}
