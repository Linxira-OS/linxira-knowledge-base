import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { listAdminSpaces } from '../../api/admin'
import { encodePathSegment } from '../../api/client'
import { Badge, EmptyState, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, adminKeys } from './shared'

export function SpacesPanel() {
  const { t, format } = useI18n()
  const query = useQuery({ queryKey: adminKeys.spaces, queryFn: listAdminSpaces })
  const spaces = query.data?.spaces ?? []

  return (
    <div className="space-y-3">
      <p className="text-[11.5px] text-fg-3">{t('admin.spaces.mutationsHint')}</p>
      <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        {spaces.length === 0 ? (
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
                </TR>
              </THead>
              <TBody>
                {spaces.map((space) => (
                  <TR key={space.space_id}>
                    <TD className="font-mono text-[11.5px]">
                      <Link className="text-accent hover:underline" to={`/spaces/${encodePathSegment(space.space_id)}`}>
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
                  </TR>
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </AdminQueryGate>
    </div>
  )
}
