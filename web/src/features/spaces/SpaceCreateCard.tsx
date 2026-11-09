import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ApiError, encodePathSegment } from '../../api/client'
import { createSpace } from '../../api/spaces'
import { Banner, Button, Card, ErrorState, Field } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { spaceKeys } from './keys'

const SPACE_ID_PATTERN = /^[A-Za-z0-9_-]{1,100}$/

/**
 * Self-service space creation. A 403 means the deployment disabled
 * `spaces.allow_user_creation`: the form is replaced by an explicit banner.
 */
export function SpaceCreateCard() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const [name, setName] = useState('')
  const [spaceId, setSpaceId] = useState('')
  const [visibility, setVisibility] = useState<'private' | 'internal'>('private')
  const [ownerType, setOwnerType] = useState<'user' | 'group'>('user')
  const [ownerId, setOwnerId] = useState('')

  const createMutation = useMutation({
    mutationFn: () =>
      createSpace({
        name: name.trim(),
        visibility,
        owner_type: ownerType,
        ...(spaceId.trim() ? { space_id: spaceId.trim() } : {}),
        ...(ownerId.trim() ? { owner_id: ownerId.trim() } : {}),
      }),
    onSuccess: (data) => {
      void queryClient.invalidateQueries({ queryKey: spaceKeys.list })
      navigate(`/spaces/${encodePathSegment(data.space.space_id)}`)
    },
  })

  const error = createMutation.error

  if (error instanceof ApiError && error.status === 403) {
    return (
      <Card title={t('spaces.create')}>
        <Banner tone="warn">{t('spaces.create.disabled')}</Banner>
      </Card>
    )
  }

  const idValue = spaceId.trim()
  const idError = idValue !== '' && !SPACE_ID_PATTERN.test(idValue) ? t('spaces.create.idInvalid') : undefined
  const ownerError = ownerType === 'group' && ownerId.trim() === '' ? t('spaces.create.ownerRequired') : undefined
  const invalid = name.trim() === '' || idError !== undefined || ownerError !== undefined

  return (
    <Card title={t('spaces.create')}>
      <form
        className="grid gap-3 sm:grid-cols-2"
        onSubmit={(event) => {
          event.preventDefault()
          if (invalid || createMutation.isPending) return
          createMutation.mutate()
        }}
      >
        <Field label={t('spaces.create.name')}>
          <input value={name} onChange={(event) => setName(event.target.value)} maxLength={200} required />
        </Field>
        <Field label={t('spaces.create.id')} hint={t('spaces.create.idHint')} error={idError}>
          <input
            value={spaceId}
            onChange={(event) => setSpaceId(event.target.value)}
            pattern="[A-Za-z0-9_-]{1,100}"
            autoComplete="off"
            spellCheck={false}
            className="font-mono"
          />
        </Field>
        <Field label={t('spaces.create.visibility')}>
          <select
            value={visibility}
            onChange={(event) => setVisibility(event.target.value === 'internal' ? 'internal' : 'private')}
          >
            <option value="private">{t('spaces.create.visibility.private')}</option>
            <option value="internal">{t('spaces.create.visibility.internal')}</option>
          </select>
        </Field>
        <Field label={t('spaces.create.ownerType')}>
          <select value={ownerType} onChange={(event) => setOwnerType(event.target.value === 'group' ? 'group' : 'user')}>
            <option value="user">{t('spaces.principal.user')}</option>
            <option value="group">{t('spaces.principal.group')}</option>
          </select>
        </Field>
        <Field label={t('spaces.create.ownerId')} hint={t('spaces.create.ownerHint')} error={ownerError}>
          <input
            value={ownerId}
            onChange={(event) => setOwnerId(event.target.value)}
            autoComplete="off"
            spellCheck={false}
            className="font-mono"
          />
        </Field>
        <div className="flex items-start sm:col-span-2">
          <Button variant="primary" type="submit" disabled={invalid || createMutation.isPending}>
            {t('action.create')}
          </Button>
        </div>
        {error && (
          <div className="sm:col-span-2">
            {error instanceof ApiError && error.status === 409 ? (
              <Banner tone="danger" title={t('spaces.create.conflict')} />
            ) : (
              <ErrorState error={error} compact />
            )}
          </div>
        )}
      </form>
    </Card>
  )
}
