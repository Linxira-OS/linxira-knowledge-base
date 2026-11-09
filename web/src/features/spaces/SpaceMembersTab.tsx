import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { addSpaceMember, listSpaceMembers, patchSpaceMember, removeSpaceMember } from '../../api/spaces'
import { hasSpaceRole, type PrincipalType, type SpaceMember, type SpaceRole } from '../../api/types'
import {
  Banner,
  Button,
  Card,
  EmptyState,
  ErrorState,
  Field,
  LoadingState,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
} from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { SPACE_ROLE_KEYS, SPACE_ROLE_ORDER, spacePrincipalLabelKey } from './badges'
import { invalidateSpaceQueries, spaceKeys } from './keys'

const PRINCIPAL_TYPES: readonly PrincipalType[] = ['user', 'group', 'agent']

interface SpaceMembersTabProps {
  spaceId: string
  role: string | null | undefined
  /** False for archived/deleted spaces: the table stays readable, writes are off. */
  active: boolean
}

/**
 * Member management is a space_admin+ capability (the backend requires
 * `SpaceAdmin` even to list members), so lower roles only see why.
 */
export function SpaceMembersTab({ spaceId, role, active }: SpaceMembersTabProps) {
  const { t } = useI18n()
  if (!hasSpaceRole(role, 'space_admin')) {
    return <Banner tone="warn">{t('spaces.members.readOnly')}</Banner>
  }
  return <MembersManager spaceId={spaceId} writable={active} />
}

function MembersManager({ spaceId, writable }: { spaceId: string; writable: boolean }) {
  const { t } = useI18n()
  const queryClient = useQueryClient()

  const membersQuery = useQuery({
    queryKey: spaceKeys.members(spaceId),
    queryFn: () => listSpaceMembers(spaceId),
    retry: false,
  })

  const [principalType, setPrincipalType] = useState<PrincipalType>('user')
  const [principalId, setPrincipalId] = useState('')
  const [role, setRole] = useState<SpaceRole>('viewer')

  const addMutation = useMutation({
    mutationFn: () =>
      addSpaceMember(spaceId, {
        principal_type: principalType,
        principal_id: principalId.trim(),
        role,
      }),
    onSuccess: () => {
      setPrincipalId('')
      invalidateSpaceQueries(queryClient, spaceId)
    },
  })

  const members = membersQuery.data?.members ?? []

  return (
    <div className="space-y-3">
      <Card title={t('spaces.members.add')} description={t('spaces.members.groupHint')}>
        <form
          className="grid gap-3 sm:grid-cols-3"
          onSubmit={(event) => {
            event.preventDefault()
            if (!writable || principalId.trim() === '' || addMutation.isPending) return
            addMutation.mutate()
          }}
        >
          <Field label={t('spaces.members.principalType')}>
            <select
              value={principalType}
              onChange={(event) => setPrincipalType(event.target.value as PrincipalType)}
              disabled={!writable || addMutation.isPending}
            >
              {PRINCIPAL_TYPES.map((value) => {
                const key = spacePrincipalLabelKey(value)
                return (
                  <option key={value} value={value}>
                    {key ? t(key) : value}
                  </option>
                )
              })}
            </select>
          </Field>
          <Field label={t('spaces.members.principalId')}>
            <input
              value={principalId}
              onChange={(event) => setPrincipalId(event.target.value)}
              disabled={!writable || addMutation.isPending}
              autoComplete="off"
              spellCheck={false}
              className="font-mono"
            />
          </Field>
          <Field label={t('spaces.members.role')}>
            <select
              value={role}
              onChange={(event) => setRole(event.target.value as SpaceRole)}
              disabled={!writable || addMutation.isPending}
            >
              {SPACE_ROLE_ORDER.map((value) => (
                <option key={value} value={value}>
                  {t(SPACE_ROLE_KEYS[value])}
                </option>
              ))}
            </select>
          </Field>
          <div className="sm:col-span-3">
            <Button
              variant="primary"
              type="submit"
              disabled={!writable || principalId.trim() === '' || addMutation.isPending}
            >
              {t('spaces.members.add')}
            </Button>
          </div>
          {addMutation.error && (
            <div className="sm:col-span-3">
              <ErrorState error={addMutation.error} compact />
            </div>
          )}
        </form>
      </Card>

      <Card title={t('spaces.members.title')}>
        {membersQuery.isPending ? (
          <LoadingState />
        ) : membersQuery.isError ? (
          <ErrorState error={membersQuery.error} onRetry={() => void membersQuery.refetch()} />
        ) : members.length === 0 ? (
          <EmptyState title={t('spaces.members.empty')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('spaces.members.principalType')}</TH>
                  <TH>{t('spaces.members.principalId')}</TH>
                  <TH>{t('spaces.members.role')}</TH>
                  <TH>{t('spaces.members.addedBy')}</TH>
                  <TH>{t('spaces.members.addedAt')}</TH>
                  <TH>{t('label.actions')}</TH>
                </TR>
              </THead>
              <TBody>
                {members.map((member) => (
                  <MemberRow
                    key={`${member.principal_type}:${member.principal_id}`}
                    spaceId={spaceId}
                    member={member}
                    writable={writable}
                  />
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </Card>
    </div>
  )
}

function MemberRow({ spaceId, member, writable }: { spaceId: string; member: SpaceMember; writable: boolean }) {
  const { t, format } = useI18n()
  const queryClient = useQueryClient()
  const [role, setRole] = useState(member.role)
  const [confirmRemove, setConfirmRemove] = useState(false)

  useEffect(() => {
    setRole(member.role)
  }, [member.role])

  const patchMutation = useMutation({
    mutationFn: (next: string) => patchSpaceMember(spaceId, member.principal_type, member.principal_id, next),
    onSuccess: () => {
      invalidateSpaceQueries(queryClient, spaceId)
    },
    onError: () => {
      setRole(member.role)
    },
  })

  const removeMutation = useMutation({
    mutationFn: () => removeSpaceMember(spaceId, member.principal_type, member.principal_id),
    onSuccess: () => {
      setConfirmRemove(false)
      invalidateSpaceQueries(queryClient, spaceId)
    },
  })

  const principalKey = spacePrincipalLabelKey(member.principal_type)

  return (
    <TR>
      <TD>{principalKey ? t(principalKey) : member.principal_type}</TD>
      <TD>
        <code className="font-mono text-[11.5px]">{member.principal_id}</code>
      </TD>
      <TD>
        <select
          aria-label={t('spaces.members.updateRole')}
          value={role}
          onChange={(event) => {
            setRole(event.target.value)
            patchMutation.mutate(event.target.value)
          }}
          disabled={!writable || patchMutation.isPending}
          className="w-auto py-[3px] text-[11.5px]"
        >
          {SPACE_ROLE_ORDER.map((value) => (
            <option key={value} value={value}>
              {t(SPACE_ROLE_KEYS[value])}
            </option>
          ))}
        </select>
      </TD>
      <TD>
        <code className="font-mono text-[11.5px]">{member.added_by}</code>
      </TD>
      <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(member.created_at)}</TD>
      <TD>
        {confirmRemove ? (
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-[11px] text-fg-2">
              {t('spaces.members.removeConfirm', { principal: member.principal_id })}
            </span>
            <Button size="sm" variant="danger" onClick={() => removeMutation.mutate()} disabled={removeMutation.isPending}>
              {t('action.confirm')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirmRemove(false)}>
              {t('action.cancel')}
            </Button>
          </div>
        ) : (
          <Button size="sm" variant="ghost" onClick={() => setConfirmRemove(true)} disabled={!writable}>
            {t('spaces.members.remove')}
          </Button>
        )}
        {removeMutation.error && (
          <div className="mt-1.5">
            <ErrorState error={removeMutation.error} compact />
          </div>
        )}
        {patchMutation.error && (
          <div className="mt-1.5">
            <ErrorState error={patchMutation.error} compact />
          </div>
        )}
      </TD>
    </TR>
  )
}
