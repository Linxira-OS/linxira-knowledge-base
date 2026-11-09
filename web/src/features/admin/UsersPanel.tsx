import { useState } from 'react'
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query'
import { disableAdminUser, listAdminUsers, patchAdminUser } from '../../api/admin'
import type { AdminUser } from '../../api/types'
import { Badge, Banner, Button, EmptyState, ErrorState, Table, TableWrap, TBody, TD, TH, THead, TR } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { ADMIN_PAGE_SIZE, AdminQueryGate, OffsetPager, adminKeys, useAdminInvalidate } from './shared'

const USER_ROLES = ['owner', 'admin', 'user'] as const

export function UsersPanel() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const [offset, setOffset] = useState(0)
  const [notice, setNotice] = useState<string | null>(null)

  const query = useQuery({
    queryKey: [...adminKeys.users, offset, ADMIN_PAGE_SIZE],
    queryFn: () => listAdminUsers({ offset, limit: ADMIN_PAGE_SIZE }),
    placeholderData: keepPreviousData,
  })

  const onChanged = () => {
    setNotice(t('admin.users.updated'))
    invalidate(adminKeys.users)
  }
  const edit = useMutation({
    mutationFn: ({ userId, body }: { userId: string; body: { role?: string; disabled?: boolean } }) =>
      patchAdminUser(userId, body),
    onSuccess: onChanged,
  })
  const disable = useMutation({ mutationFn: disableAdminUser, onSuccess: onChanged })

  const users = query.data?.users ?? []
  const total = query.data?.total ?? 0
  const rowPending = (user: AdminUser) =>
    (edit.isPending && edit.variables?.userId === user.user_id) ||
    (disable.isPending && disable.variables === user.user_id)

  return (
    <div className="space-y-3">
      {notice && <Banner tone="accent">{notice}</Banner>}
      {edit.isError && <ErrorState error={edit.error} compact />}
      {disable.isError && <ErrorState error={disable.error} compact />}
      <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        {users.length === 0 ? (
          <EmptyState title={t('admin.users.empty')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('admin.users.username')}</TH>
                  <TH>{t('admin.users.displayName')}</TH>
                  <TH>{t('admin.users.role')}</TH>
                  <TH>{t('admin.users.disabled')}</TH>
                  <TH>{t('label.createdAt')}</TH>
                  <TH>{t('label.actions')}</TH>
                </TR>
              </THead>
              <TBody>
                {users.map((user) => (
                  <TR key={user.user_id}>
                    <TD className="font-mono text-[11.5px]">{user.username}</TD>
                    <TD>{user.display_name}</TD>
                    <TD>
                      <select
                        aria-label={t('admin.users.role')}
                        className="w-auto py-[2px] text-[11.5px]"
                        value={user.role}
                        disabled={rowPending(user)}
                        onChange={(event) => {
                          setNotice(null)
                          edit.mutate({ userId: user.user_id, body: { role: event.target.value } })
                        }}
                      >
                        {USER_ROLES.map((role) => (
                          <option key={role} value={role}>
                            {role}
                          </option>
                        ))}
                      </select>
                    </TD>
                    <TD>
                      {user.disabled ? (
                        <Badge tone="warn">{t('admin.users.disabled')}</Badge>
                      ) : (
                        <Badge tone="accent">{t('admin.users.enabled')}</Badge>
                      )}
                    </TD>
                    <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(user.created_at)}</TD>
                    <TD>
                      {user.disabled ? (
                        <Button
                          size="sm"
                          disabled={rowPending(user)}
                          onClick={() => {
                            setNotice(null)
                            edit.mutate({ userId: user.user_id, body: { disabled: false } })
                          }}
                        >
                          {t('admin.users.enable')}
                        </Button>
                      ) : (
                        <Button
                          size="sm"
                          variant="danger"
                          disabled={rowPending(user)}
                          onClick={() => {
                            setNotice(null)
                            disable.mutate(user.user_id)
                          }}
                        >
                          {t('admin.users.disable')}
                        </Button>
                      )}
                    </TD>
                  </TR>
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
        {query.data && (
          <OffsetPager
            offset={offset}
            limit={ADMIN_PAGE_SIZE}
            total={total}
            loading={query.isFetching}
            onChange={setOffset}
            label={t('admin.users.total', { total: format.number(total) })}
          />
        )}
      </AdminQueryGate>
    </div>
  )
}
