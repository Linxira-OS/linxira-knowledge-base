import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import {
  addGroupMember,
  createGroup,
  listGroupMembers,
  patchGroupMember,
  removeGroupMember,
} from '../../api/spaces'
import { listAdminGroups } from '../../api/admin'
import type { Group } from '../../api/types'
import { Badge, Banner, Button, Card, EmptyState, ErrorState, Table, TableWrap, TBody, TD, TH, THead, TR, statusTone } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { AdminQueryGate, SelectInput, TextInput, adminKeys, useAdminInvalidate } from './shared'

const GROUP_ROLES = ['group_member', 'group_admin'] as const
const GROUP_ROLE_OPTIONS = GROUP_ROLES.map((role) => ({ value: role, label: role }))

export function GroupsPanel() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const [notice, setNotice] = useState<string | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')

  const query = useQuery({ queryKey: adminKeys.groups, queryFn: listAdminGroups })
  const create = useMutation({
    mutationFn: () => createGroup({ name: name.trim(), description: description.trim() || undefined }),
    onSuccess: () => {
      setNotice(t('admin.groups.created'))
      setName('')
      setDescription('')
      invalidate(adminKeys.groups)
    },
  })

  const groups = query.data?.groups ?? []
  const selectedGroup = groups.find((group) => group.group_id === selected) ?? null

  return (
    <div className="space-y-4">
      {notice && <Banner tone="accent">{notice}</Banner>}
      {create.isError && <ErrorState error={create.error} compact />}
      <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
        {groups.length === 0 ? (
          <EmptyState title={t('admin.groups.empty')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('label.name')}</TH>
                  <TH>{t('label.description')}</TH>
                  <TH>{t('label.status')}</TH>
                  <TH>{t('admin.groups.createdBy')}</TH>
                  <TH>{t('label.createdAt')}</TH>
                  <TH>{t('label.actions')}</TH>
                </TR>
              </THead>
              <TBody>
                {groups.map((group) => (
                  <TR key={group.group_id} className={selected === group.group_id ? 'bg-panel' : undefined}>
                    <TD className="text-fg">{group.name}</TD>
                    <TD>{group.description || t('label.none')}</TD>
                    <TD>
                      <Badge tone={statusTone(group.status)}>{group.status}</Badge>
                    </TD>
                    <TD className="font-mono text-[11.5px]">{group.created_by}</TD>
                    <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(group.created_at)}</TD>
                    <TD>
                      <Button
                        size="sm"
                        onClick={() => setSelected(selected === group.group_id ? null : group.group_id)}
                      >
                        {t('admin.groups.viewMembers')}
                      </Button>
                    </TD>
                  </TR>
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </AdminQueryGate>
      <Card title={t('admin.groups.create')}>
        <form
          className="grid gap-3 sm:grid-cols-2"
          onSubmit={(event) => {
            event.preventDefault()
            if (name.trim() !== '') create.mutate()
          }}
        >
          <TextInput label={t('label.name')} value={name} onChange={setName} />
          <TextInput label={t('label.description')} value={description} onChange={setDescription} />
          <div>
            <Button type="submit" disabled={name.trim() === '' || create.isPending}>
              {t('action.create')}
            </Button>
          </div>
        </form>
      </Card>
      {selectedGroup && (
        <GroupMembersCard
          key={selectedGroup.group_id}
          group={selectedGroup}
          onClose={() => setSelected(null)}
        />
      )}
    </div>
  )
}

function GroupMembersCard({ group, onClose }: { group: Group; onClose: () => void }) {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const membersKey = [...adminKeys.groups, group.group_id, 'members'] as const
  const [notice, setNotice] = useState<string | null>(null)
  const [principalId, setPrincipalId] = useState('')
  const [role, setRole] = useState<string>(GROUP_ROLES[0])

  const query = useQuery({ queryKey: membersKey, queryFn: () => listGroupMembers(group.group_id) })
  const add = useMutation({
    mutationFn: () => addGroupMember(group.group_id, { principal_id: principalId.trim(), role }),
    onSuccess: () => {
      setNotice(t('admin.groups.memberAdded'))
      setPrincipalId('')
      invalidate(membersKey, adminKeys.groups)
    },
  })
  const patch = useMutation({
    mutationFn: ({ memberId, nextRole }: { memberId: string; nextRole: string }) =>
      patchGroupMember(group.group_id, memberId, nextRole),
    onSuccess: () => {
      setNotice(t('admin.groups.memberUpdated'))
      invalidate(membersKey, adminKeys.groups)
    },
  })
  const remove = useMutation({
    mutationFn: (memberId: string) => removeGroupMember(group.group_id, memberId),
    onSuccess: () => {
      setNotice(t('admin.groups.memberRemoved'))
      invalidate(membersKey, adminKeys.groups)
    },
  })

  const members = query.data?.members ?? []
  const rowPending = (memberId: string) =>
    (patch.isPending && patch.variables?.memberId === memberId) ||
    (remove.isPending && remove.variables === memberId)

  return (
    <Card
      title={t('admin.groups.members')}
      description={`${group.name} · ${group.group_id}`}
      actions={
        <Button size="sm" variant="ghost" onClick={onClose}>
          {t('action.close')}
        </Button>
      }
    >
      <div className="space-y-3">
        {notice && <p className="text-[11.5px] text-accent">{notice}</p>}
        {add.isError && <ErrorState error={add.error} compact />}
        {patch.isError && <ErrorState error={patch.error} compact />}
        {remove.isError && <ErrorState error={remove.error} compact />}
        <form
          className="grid items-end gap-2 sm:grid-cols-[1fr_160px_auto]"
          onSubmit={(event) => {
            event.preventDefault()
            if (principalId.trim() !== '') add.mutate()
          }}
        >
          <TextInput label={t('admin.groups.principalId')} value={principalId} onChange={setPrincipalId} />
          <SelectInput label={t('label.role')} value={role} onChange={setRole} options={GROUP_ROLE_OPTIONS} />
          <Button type="submit" size="sm" disabled={principalId.trim() === '' || add.isPending}>
            {t('admin.groups.addMember')}
          </Button>
        </form>
        <AdminQueryGate pending={query.isPending} error={query.error} onRetry={() => void query.refetch()}>
          {members.length === 0 ? (
            <EmptyState title={t('state.empty')} />
          ) : (
            <TableWrap>
              <Table>
                <THead>
                  <TR>
                    <TH>{t('admin.groups.principalId')}</TH>
                    <TH>{t('label.role')}</TH>
                    <TH>{t('admin.groups.addedBy')}</TH>
                    <TH>{t('label.createdAt')}</TH>
                    <TH>{t('label.actions')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {members.map((member) => (
                    <TR key={member.principal_id}>
                      <TD className="font-mono text-[11.5px]">{member.principal_id}</TD>
                      <TD>
                        <select
                          aria-label={t('label.role')}
                          className="w-auto py-[2px] text-[11.5px]"
                          value={member.role}
                          disabled={rowPending(member.principal_id)}
                          onChange={(event) => {
                            setNotice(null)
                            patch.mutate({ memberId: member.principal_id, nextRole: event.target.value })
                          }}
                        >
                          {GROUP_ROLES.map((memberRole) => (
                            <option key={memberRole} value={memberRole}>
                              {memberRole}
                            </option>
                          ))}
                        </select>
                      </TD>
                      <TD className="font-mono text-[11.5px]">{member.added_by}</TD>
                      <TD className="whitespace-nowrap text-[11.5px] text-fg-3">{format.dateTime(member.created_at)}</TD>
                      <TD>
                        <Button
                          size="sm"
                          variant="danger"
                          disabled={rowPending(member.principal_id)}
                          onClick={() => {
                            setNotice(null)
                            remove.mutate(member.principal_id)
                          }}
                        >
                          {t('action.remove')}
                        </Button>
                      </TD>
                    </TR>
                  ))}
                </TBody>
              </Table>
            </TableWrap>
          )}
        </AdminQueryGate>
      </div>
    </Card>
  )
}
