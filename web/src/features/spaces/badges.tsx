import { Badge, statusTone } from '../../components/ui'
import { useI18n, type MessageKey } from '../../lib/i18n'
import type { SpaceRole } from '../../api/types'

/** Labels for the four `space.role` values the backend can return. */
export const SPACE_ROLE_KEYS: Record<SpaceRole, MessageKey> = {
  viewer: 'spaces.role.viewer',
  editor: 'spaces.role.editor',
  space_admin: 'spaces.role.admin',
  space_owner: 'spaces.role.owner',
}

/** Ascending strength, matching `SPACE_ROLE_RANK`. */
export const SPACE_ROLE_ORDER: readonly SpaceRole[] = ['viewer', 'editor', 'space_admin', 'space_owner']

export function spaceRoleLabelKey(role: string | null | undefined): MessageKey | null {
  if (role && Object.prototype.hasOwnProperty.call(SPACE_ROLE_KEYS, role)) {
    return SPACE_ROLE_KEYS[role as SpaceRole]
  }
  return null
}

export function spaceStatusLabelKey(status: string): MessageKey | null {
  switch (status) {
    case 'active':
      return 'state.active'
    case 'archived':
      return 'state.archived'
    case 'deleted':
      return 'state.deleted'
    default:
      return null
  }
}

export function spaceVisibilityLabelKey(visibility: string): MessageKey | null {
  switch (visibility) {
    case 'private':
      return 'spaces.visibility.private'
    case 'internal':
      return 'spaces.visibility.internal'
    default:
      return null
  }
}

const PRINCIPAL_KEYS: Record<string, MessageKey> = {
  user: 'spaces.principal.user',
  group: 'spaces.principal.group',
  agent: 'spaces.principal.agent',
}

export function spacePrincipalLabelKey(principalType: string): MessageKey | null {
  return Object.prototype.hasOwnProperty.call(PRINCIPAL_KEYS, principalType) ? PRINCIPAL_KEYS[principalType] : null
}

const SCOPE_KEYS: Record<string, MessageKey> = {
  tenant: 'spaces.scope.tenant',
  user: 'spaces.scope.user',
  group: 'spaces.scope.group',
  space: 'spaces.scope.space',
}

export function spaceScopeLabelKey(scopeType: string): MessageKey | null {
  return Object.prototype.hasOwnProperty.call(SCOPE_KEYS, scopeType) ? SCOPE_KEYS[scopeType] : null
}

export function SpaceStatusBadge({ status }: { status: string }) {
  const { t } = useI18n()
  const key = spaceStatusLabelKey(status)
  return <Badge tone={statusTone(status)}>{key ? t(key) : status}</Badge>
}

export function SpaceRoleBadge({ role }: { role: string | null | undefined }) {
  const { t } = useI18n()
  const key = spaceRoleLabelKey(role)
  return <Badge>{key ? t(key) : t('label.none')}</Badge>
}

export function SpaceVisibilityBadge({ visibility }: { visibility: string }) {
  const { t } = useI18n()
  const key = spaceVisibilityLabelKey(visibility)
  return <Badge>{key ? t(key) : visibility}</Badge>
}
