import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { archiveSpace, deleteSpace, patchSpace, restoreSpace, transferSpace } from '../../api/spaces'
import type { Space } from '../../api/types'
import { Banner, Button, Card, ErrorState, Field, KeyValue } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { invalidateSpaceQueries } from './keys'
import { spaceStatusLabelKey, spaceVisibilityLabelKey } from './badges'

interface SpaceGeneralTabProps {
  space: Space
  /** Effective status — `deleted` also after a successful delete on this page. */
  status: string
  canAdmin: boolean
  canOwner: boolean
  onDeleted: () => void
}

/**
 * Overview tab: settings, lifecycle (archive/restore) and owner-only
 * delete/transfer. Archived and deleted spaces keep the information visible
 * but every write is disabled (the backend answers 409 `space_not_active`).
 */
export function SpaceGeneralTab({ space, status, canAdmin, canOwner, onDeleted }: SpaceGeneralTabProps) {
  const { t, format } = useI18n()
  const queryClient = useQueryClient()
  const active = status === 'active'

  const [name, setName] = useState(space.name)
  const [visibility, setVisibility] = useState(space.visibility === 'internal' ? 'internal' : 'private')
  const [settingsSaved, setSettingsSaved] = useState(false)
  const settingsMutation = useMutation({
    mutationFn: () => patchSpace(space.space_id, { name: name.trim(), visibility }),
    onSuccess: (data) => {
      setSettingsSaved(true)
      setName(data.space.name)
      setVisibility(data.space.visibility === 'internal' ? 'internal' : 'private')
      invalidateSpaceQueries(queryClient, space.space_id)
    },
  })

  const [confirmArchive, setConfirmArchive] = useState(false)
  const [confirmRestore, setConfirmRestore] = useState(false)
  const archiveMutation = useMutation({
    mutationFn: () => archiveSpace(space.space_id),
    onSuccess: () => {
      setConfirmArchive(false)
      invalidateSpaceQueries(queryClient, space.space_id)
    },
  })
  const restoreMutation = useMutation({
    mutationFn: () => restoreSpace(space.space_id),
    onSuccess: () => {
      setConfirmRestore(false)
      invalidateSpaceQueries(queryClient, space.space_id)
    },
  })

  const [confirmDelete, setConfirmDelete] = useState(false)
  const deleteMutation = useMutation({
    mutationFn: () => deleteSpace(space.space_id),
    onSuccess: () => {
      onDeleted()
      invalidateSpaceQueries(queryClient, space.space_id)
    },
  })

  const [transferType, setTransferType] = useState<'user' | 'group'>('user')
  const [transferId, setTransferId] = useState('')
  const [transferDone, setTransferDone] = useState(false)
  const transferMutation = useMutation({
    mutationFn: () => transferSpace(space.space_id, { owner_type: transferType, owner_id: transferId.trim() }),
    onSuccess: () => {
      setTransferDone(true)
      setTransferId('')
      invalidateSpaceQueries(queryClient, space.space_id)
    },
  })

  const visibilityKey = spaceVisibilityLabelKey(space.visibility)
  const statusKey = spaceStatusLabelKey(status)

  return (
    <div className="space-y-3">
      <Card title={t('spaces.settings')}>
        {canAdmin ? (
          <form
            className="grid gap-3 sm:grid-cols-2"
            onSubmit={(event) => {
              event.preventDefault()
              if (!active || name.trim() === '' || settingsMutation.isPending) return
              settingsMutation.mutate()
            }}
          >
            <Field label={t('spaces.settings.name')}>
              <input
                value={name}
                onChange={(event) => {
                  setName(event.target.value)
                  setSettingsSaved(false)
                }}
                disabled={!active || settingsMutation.isPending}
                maxLength={200}
              />
            </Field>
            <Field label={t('spaces.settings.visibility')}>
              <select
                value={visibility}
                onChange={(event) => {
                  setVisibility(event.target.value === 'internal' ? 'internal' : 'private')
                  setSettingsSaved(false)
                }}
                disabled={!active || settingsMutation.isPending}
              >
                <option value="private">{t('spaces.create.visibility.private')}</option>
                <option value="internal">{t('spaces.create.visibility.internal')}</option>
              </select>
            </Field>
            <div className="flex items-center gap-2 sm:col-span-2">
              <Button
                variant="primary"
                type="submit"
                disabled={!active || name.trim() === '' || settingsMutation.isPending}
              >
                {t('action.save')}
              </Button>
              {settingsSaved && <span className="text-[11.5px] text-accent">{t('spaces.settings.saved')}</span>}
            </div>
            {settingsMutation.error && (
              <div className="sm:col-span-2">
                <ErrorState error={settingsMutation.error} compact />
              </div>
            )}
          </form>
        ) : (
          <KeyValue
            items={[
              [t('spaces.settings.name'), space.name],
              [t('spaces.settings.visibility'), visibilityKey ? t(visibilityKey) : space.visibility],
              [t('spaces.owner'), `${space.owner_type}:${space.owner_id}`],
              [t('label.status'), statusKey ? t(statusKey) : status],
              [t('label.createdAt'), format.dateTime(space.created_at)],
            ]}
          />
        )}
      </Card>

      {canOwner && (status === 'active' || status === 'archived') && (
        <Card title={t('spaces.settings.lifecycle')}>
          {status === 'active' &&
            (confirmArchive ? (
              <Banner
                tone="warn"
                title={t('spaces.settings.archive')}
                actions={
                  <>
                    <Button size="sm" onClick={() => archiveMutation.mutate()} disabled={archiveMutation.isPending}>
                      {t('action.confirm')}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setConfirmArchive(false)}>
                      {t('action.cancel')}
                    </Button>
                  </>
                }
              >
                {t('spaces.settings.archiveConfirm')}
              </Banner>
            ) : (
              <Button onClick={() => setConfirmArchive(true)}>{t('spaces.settings.archive')}</Button>
            ))}
          {status === 'archived' &&
            (confirmRestore ? (
              <Banner
                tone="warn"
                title={t('spaces.settings.restore')}
                actions={
                  <>
                    <Button size="sm" onClick={() => restoreMutation.mutate()} disabled={restoreMutation.isPending}>
                      {t('action.confirm')}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setConfirmRestore(false)}>
                      {t('action.cancel')}
                    </Button>
                  </>
                }
              >
                {t('spaces.settings.restoreConfirm')}
              </Banner>
            ) : (
              <Button onClick={() => setConfirmRestore(true)}>{t('spaces.settings.restore')}</Button>
            ))}
          {archiveMutation.error && (
            <div className="mt-2">
              <ErrorState error={archiveMutation.error} compact />
            </div>
          )}
          {restoreMutation.error && (
            <div className="mt-2">
              <ErrorState error={restoreMutation.error} compact />
            </div>
          )}
        </Card>
      )}

      {canOwner && status !== 'deleted' && (
        <Card title={t('spaces.settings.delete')}>
          <label className="flex items-start gap-2 text-[12px] leading-5 text-fg-2">
            <input
              type="checkbox"
              className="mt-[3px]"
              checked={confirmDelete}
              onChange={(event) => setConfirmDelete(event.target.checked)}
              disabled={!active || deleteMutation.isPending}
            />
            <span>{t('spaces.settings.deleteConfirm')}</span>
          </label>
          <div className="mt-2">
            <Button
              variant="danger"
              disabled={!confirmDelete || !active || deleteMutation.isPending}
              onClick={() => deleteMutation.mutate()}
            >
              {t('spaces.settings.delete')}
            </Button>
          </div>
          {deleteMutation.error && (
            <div className="mt-2">
              <ErrorState error={deleteMutation.error} compact />
            </div>
          )}
        </Card>
      )}

      {canOwner && status !== 'deleted' && (
        <Card title={t('spaces.settings.transfer')}>
          <form
            className="grid gap-3 sm:grid-cols-2"
            onSubmit={(event) => {
              event.preventDefault()
              if (!active || transferId.trim() === '' || transferMutation.isPending) return
              setTransferDone(false)
              transferMutation.mutate()
            }}
          >
            <Field label={t('spaces.settings.transferOwnerType')}>
              <select
                value={transferType}
                onChange={(event) => {
                  setTransferType(event.target.value === 'group' ? 'group' : 'user')
                  setTransferDone(false)
                }}
                disabled={!active || transferMutation.isPending}
              >
                <option value="user">{t('spaces.principal.user')}</option>
                <option value="group">{t('spaces.principal.group')}</option>
              </select>
            </Field>
            <Field label={t('spaces.settings.transferOwnerId')}>
              <input
                value={transferId}
                onChange={(event) => {
                  setTransferId(event.target.value)
                  setTransferDone(false)
                }}
                disabled={!active || transferMutation.isPending}
                autoComplete="off"
                spellCheck={false}
                className="font-mono"
              />
            </Field>
            <div className="flex items-center gap-2 sm:col-span-2">
              <Button
                variant="primary"
                type="submit"
                disabled={!active || transferId.trim() === '' || transferMutation.isPending}
              >
                {t('action.transfer')}
              </Button>
              {transferDone && <span className="text-[11.5px] text-accent">{t('spaces.settings.transferDone')}</span>}
            </div>
          </form>
          {transferMutation.error && (
            <div className="mt-2">
              <ErrorState error={transferMutation.error} compact />
            </div>
          )}
        </Card>
      )}
    </div>
  )
}
