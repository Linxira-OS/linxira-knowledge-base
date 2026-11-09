import { useQuery } from '@tanstack/react-query'
import { listAdminRoles } from '../../api/admin'
import { EmptyState, Table, TableWrap, TBody, TD, TH, THead, TR } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, adminKeys } from './shared'

export function RolesPanel() {
  const { t } = useI18n()
  const query = useQuery({ queryKey: adminKeys.roles, queryFn: listAdminRoles })
  const roles = query.data?.roles ?? []

  return (
    <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
      <div className="space-y-3">
        <p className="text-[11.5px] text-fg-3">{t('admin.roles.readOnly')}</p>
        {roles.length === 0 ? (
          <EmptyState title={t('state.empty')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('label.role')}</TH>
                  <TH>{t('admin.roles.description')}</TH>
                </TR>
              </THead>
              <TBody>
                {roles.map((role) => (
                  <TR key={role.role}>
                    <TD className="font-mono text-[11.5px]">{role.role}</TD>
                    <TD>{role.description}</TD>
                  </TR>
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </div>
    </AdminQueryGate>
  )
}
