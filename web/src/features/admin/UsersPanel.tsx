import { useState } from 'react'
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import {
  createAdminUser,
  deleteAdminUser,
  listAdminUsers,
  patchAdminUser,
  resetAdminUserPassword,
} from '../../api/admin'
import { ApiError } from '../../api/client'
import type { AdminUser } from '../../api/types'
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  ErrorState,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { useUnsavedWarning } from '../../lib/hooks'
import { useSession } from '../../lib/session'
import {
  ADMIN_PAGE_SIZE,
  AdminQueryGate,
  OffsetPager,
  SelectInput,
  TextInput,
  adminKeys,
  useAdminInvalidate,
} from './shared'
import { ConfirmDialog, FormDialog, useAdminToast } from './kit'

const USER_ROLES = ['owner', 'admin', 'user'] as const
/** Backend requires `password.chars().count() >= 8`. */
const MIN_PASSWORD = 8

export function UsersPanel() {
  const { t, format } = useI18n()
  const session = useSession()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const [offset, setOffset] = useState(0)
  const [filter, setFilter] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [confirmDisable, setConfirmDisable] = useState<AdminUser | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<AdminUser | null>(null)
  const [resetUser, setResetUser] = useState<AdminUser | null>(null)

  const query = useQuery({
    queryKey: [...adminKeys.users, offset, ADMIN_PAGE_SIZE],
    queryFn: () => listAdminUsers({ offset, limit: ADMIN_PAGE_SIZE }),
    placeholderData: keepPreviousData,
  })

  const edit = useMutation({
    mutationFn: ({ userId, body }: { userId: string; body: { role?: string; disabled?: boolean } }) =>
      patchAdminUser(userId, body),
    onSuccess: () => {
      toast.success(`${t('admin.users.updated')} · ${t('admin.toast.audited')}`)
      setConfirmDisable(null)
      invalidate(adminKeys.users)
    },
  })
  const remove = useMutation({
    mutationFn: deleteAdminUser,
    onSuccess: (_result, userId) => {
      toast.success(`${t('admin.users.deleted')} · ${t('admin.toast.audited')}`)
      setConfirmDelete((current) => (current?.user_id === userId ? null : current))
      invalidate(adminKeys.users)
    },
  })

  const users = query.data?.users ?? []
  const total = query.data?.total ?? 0
  const needle = filter.trim().toLowerCase()
  const visible =
    needle === ''
      ? users
      : users.filter(
          (user) =>
            user.username.toLowerCase().includes(needle) || user.display_name.toLowerCase().includes(needle),
        )
  const selfId = session.principal?.user_id
  const rowPending = (user: AdminUser) =>
    (edit.isPending && edit.variables?.userId === user.user_id) ||
    (remove.isPending && remove.variables === user.user_id)

  return (
    <div className="space-y-3">
      {edit.isError && <ErrorState error={edit.error} compact />}
      {remove.isError && <ErrorState error={remove.error} compact />}
      <div className="flex flex-wrap items-end gap-3">
        <div className="w-full sm:max-w-[320px]">
          <TextInput
            label={t('admin.users.filter')}
            value={filter}
            onChange={setFilter}
            hint={t('admin.users.filterHint')}
          />
        </div>
        <Button size="sm" variant="primary" onClick={() => setCreateOpen(true)}>
          {t('admin.users.addUser')}
        </Button>
      </div>
      <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        {visible.length === 0 ? (
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
                  <TH>{t('admin.users.source')}</TH>
                  <TH>{t('admin.users.lastLogin')}</TH>
                  <TH>{t('admin.users.lastSso')}</TH>
                  <TH>{t('label.createdAt')}</TH>
                  <TH>{t('label.actions')}</TH>
                </TR>
              </THead>
              <TBody>
                {visible.map((user) => (
                  <TR key={user.user_id}>
                    <TD className="font-mono text-[11.5px]">{user.username}</TD>
                    <TD>{user.display_name}</TD>
                    <TD>
                      <select
                        aria-label={t('admin.users.role')}
                        className="w-auto py-[2px] text-[11.5px]"
                        value={user.role}
                        disabled={rowPending(user) || user.user_id === selfId}
                        onChange={(event) => edit.mutate({ userId: user.user_id, body: { role: event.target.value } })}
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
                    <TD className="text-[11.5px] text-fg-3">{user.source || '—'}</TD>
                    <TD className="whitespace-nowrap text-[11.5px] text-fg-3">
                      {user.last_login_at ? format.dateTime(user.last_login_at) : '—'}
                    </TD>
                    <TD className="whitespace-nowrap text-[11.5px] text-fg-3">
                      {user.last_sso_at ? format.dateTime(user.last_sso_at) : '—'}
                    </TD>
                    <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(user.created_at)}</TD>
                    <TD>
                      <div className="flex flex-wrap items-center gap-1.5">
                        {user.disabled ? (
                          <Button
                            size="sm"
                            disabled={rowPending(user)}
                            onClick={() => edit.mutate({ userId: user.user_id, body: { disabled: false } })}
                          >
                            {t('admin.users.enable')}
                          </Button>
                        ) : (
                          <Button
                            size="sm"
                            disabled={rowPending(user) || user.user_id === selfId}
                            onClick={() => setConfirmDisable(user)}
                          >
                            {t('admin.users.disable')}
                          </Button>
                        )}
                        <Button size="sm" variant="ghost" onClick={() => setResetUser(user)}>
                          {t('admin.users.resetPassword')}
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          disabled={rowPending(user) || user.user_id === selfId}
                          onClick={() => setConfirmDelete(user)}
                        >
                          {t('action.delete')}
                        </Button>
                        <Link
                          to={`/admin/audit?actor=${encodeURIComponent(user.user_id)}`}
                          className="text-[11.5px] text-accent"
                        >
                          {t('admin.row.audit')}
                        </Link>
                      </div>
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

      {createOpen && <UserCreateDialog onClose={() => setCreateOpen(false)} />}

      <ConfirmDialog
        open={confirmDisable !== null}
        title={t('admin.users.disable')}
        description={t('admin.users.disableConfirm', { user: confirmDisable?.username ?? '' })}
        confirmLabel={t('admin.users.disable')}
        pending={edit.isPending}
        onConfirm={() => {
          if (confirmDisable !== null) edit.mutate({ userId: confirmDisable.user_id, body: { disabled: true } })
        }}
        onCancel={() => setConfirmDisable(null)}
      />
      <ConfirmDialog
        open={confirmDelete !== null}
        title={t('admin.users.deleteTitle')}
        description={t('admin.users.deleteConfirm', { user: confirmDelete?.username ?? '' })}
        confirmLabel={t('action.delete')}
        pending={remove.isPending}
        onConfirm={() => {
          if (confirmDelete !== null) remove.mutate(confirmDelete.user_id)
        }}
        onCancel={() => setConfirmDelete(null)}
      />
      {resetUser !== null && (
        <ResetPasswordDialog key={resetUser.user_id} user={resetUser} onClose={() => setResetUser(null)} />
      )}
    </div>
  )
}

function UserCreateDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n()
  const invalidate = useAdminInvalidate()
  const toast = useAdminToast()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [role, setRole] = useState<string>('user')

  const passwordValid = password.length >= MIN_PASSWORD
  const canSubmit = username.trim() !== '' && passwordValid
  useUnsavedWarning(username !== '' || password !== '' || displayName !== '')

  const create = useMutation({
    mutationFn: () =>
      createAdminUser({
        username: username.trim(),
        password,
        ...(displayName.trim() !== '' ? { display_name: displayName.trim() } : {}),
        role,
      }),
    onSuccess: (result) => {
      toast.success(`${t('admin.users.created', { user: result.user.username })} · ${t('admin.toast.audited')}`)
      invalidate(adminKeys.users)
      onClose()
    },
  })
  const conflict = create.error instanceof ApiError && create.error.code === 'conflict'

  return (
    <FormDialog
      open
      title={t('admin.users.addUser')}
      pending={create.isPending}
      submitLabel={t('action.create')}
      submitDisabled={!canSubmit}
      onSubmit={() => create.mutate()}
      onClose={onClose}
    >
      <TextInput label={t('admin.users.username')} value={username} onChange={setUsername} />
      <TextInput
        label={t('admin.users.password')}
        value={password}
        onChange={setPassword}
        type="password"
        hint={t('admin.users.passwordHint')}
        error={password !== '' && !passwordValid ? t('admin.users.passwordShort') : undefined}
      />
      <TextInput label={t('admin.users.displayName')} value={displayName} onChange={setDisplayName} />
      <SelectInput
        label={t('admin.users.role')}
        value={role}
        onChange={setRole}
        options={USER_ROLES.map((value) => ({ value, label: value }))}
      />
      {conflict && <Banner tone="warn" title={t('admin.users.usernameTaken')} />}
      {create.error != null && !conflict && <ErrorState error={create.error} compact />}
    </FormDialog>
  )
}

function ResetPasswordDialog({ user, onClose }: { user: AdminUser; onClose: () => void }) {
  const { t } = useI18n()
  const toast = useAdminToast()
  const [password, setPassword] = useState('')
  const passwordValid = password.length >= MIN_PASSWORD
  useUnsavedWarning(password !== '')

  const reset = useMutation({
    mutationFn: () => resetAdminUserPassword(user.user_id, password),
    onSuccess: () => {
      toast.success(`${t('admin.users.passwordReset')} · ${t('admin.toast.audited')}`)
      onClose()
    },
  })

  return (
    <FormDialog
      open
      title={`${t('admin.users.resetPassword')} · ${user.username}`}
      pending={reset.isPending}
      submitLabel={t('admin.users.resetPassword')}
      submitDisabled={!passwordValid}
      onSubmit={() => reset.mutate()}
      onClose={onClose}
    >
      <p className="text-[12px] leading-5 text-fg-2">{t('admin.users.resetHint')}</p>
      <TextInput
        label={t('admin.users.password')}
        value={password}
        onChange={setPassword}
        type="password"
        hint={t('admin.users.passwordHint')}
        error={password !== '' && !passwordValid ? t('admin.users.passwordShort') : undefined}
      />
      {reset.error != null && <ErrorState error={reset.error} compact />}
    </FormDialog>
  )
}
