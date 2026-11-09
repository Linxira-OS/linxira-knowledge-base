import { api, encodePathSegment, queryString } from './client'
import type {
  Group,
  GroupMember,
  Space,
  SpaceMember,
  SpaceQuota,
  StorageUsage,
} from './types'

/* --------------------------------------------------------------------- spaces */

export function listSpaces(): Promise<{ spaces: Space[] }> {
  return api('/api/v1/spaces')
}

export interface CreateSpaceBody {
  space_id?: string
  name?: string
  visibility?: string
  owner_type?: string
  owner_id?: string
  settings?: unknown
}

export function createSpace(body: CreateSpaceBody): Promise<{ space: Space; request_id: string }> {
  return api('/api/v1/spaces', { method: 'POST', body })
}

export function getSpace(spaceId: string): Promise<{ space: Space; usage: StorageUsage }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}`)
}

export interface UpdateSpaceBody {
  name?: string
  visibility?: string
  settings?: unknown
}

export function patchSpace(spaceId: string, body: UpdateSpaceBody): Promise<{ space: Space; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}`, { method: 'PATCH', body })
}

export function deleteSpace(spaceId: string): Promise<{ space_id: string; status: string; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}`, { method: 'DELETE' })
}

export function archiveSpace(spaceId: string): Promise<{ space_id: string; status: string; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/archive`, { method: 'POST' })
}

export function restoreSpace(spaceId: string): Promise<{ space_id: string; status: string; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/restore`, { method: 'POST' })
}

export function transferSpace(
  spaceId: string,
  body: { owner_type: string; owner_id: string },
): Promise<{ space: Space; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/transfer`, { method: 'POST', body })
}

export function getSpaceUsage(
  spaceId: string,
): Promise<{ space: Space; usage: StorageUsage; quotas: SpaceQuota[] }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/usage`)
}

/* -------------------------------------------------------------------- members */

export function listSpaceMembers(spaceId: string): Promise<{ space_id: string; members: SpaceMember[] }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/members`)
}

export function addSpaceMember(
  spaceId: string,
  body: { principal_type: string; principal_id: string; role: string },
): Promise<{ space_id: string; principal_type: string; principal_id: string; role: string; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/members`, { method: 'POST', body })
}

export function patchSpaceMember(
  spaceId: string,
  principalType: string,
  principalId: string,
  role: string,
): Promise<{ space_id: string; principal_type: string; principal_id: string; role: string; request_id: string }> {
  const path = `/api/v1/spaces/${encodePathSegment(spaceId)}/members/${encodePathSegment(principalType)}/${encodePathSegment(principalId)}`
  return api(path, { method: 'PATCH', body: { role } })
}

export function removeSpaceMember(
  spaceId: string,
  principalType: string,
  principalId: string,
): Promise<{ space_id: string; removed: boolean; request_id: string }> {
  const path = `/api/v1/spaces/${encodePathSegment(spaceId)}/members/${encodePathSegment(principalType)}/${encodePathSegment(principalId)}`
  return api(path, { method: 'DELETE' })
}

/* ---------------------------------------------------------------------- quota */

export interface SpaceQuotaBody {
  storage_bytes?: number
  documents?: number
  chunks?: number
  members?: number
  enforcement?: string
  reason?: string
}

export function setSpaceQuota(spaceId: string, body: SpaceQuotaBody): Promise<{ quota: SpaceQuota; request_id: string }> {
  return api(`/api/v1/spaces/${encodePathSegment(spaceId)}/quota`, { method: 'PUT', body })
}

/* --------------------------------------------------------------------- groups */

export function listGroups(): Promise<{ groups: Group[] }> {
  return api('/api/v1/groups')
}

export function createGroup(body: { name: string; description?: string }): Promise<{ group: Group; request_id: string }> {
  return api('/api/v1/groups', { method: 'POST', body })
}

export function getGroup(groupId: string): Promise<{ group: Group }> {
  return api(`/api/v1/groups/${encodePathSegment(groupId)}`)
}

export function patchGroup(
  groupId: string,
  body: { name?: string; description?: string },
): Promise<{ group: Group; request_id: string }> {
  return api(`/api/v1/groups/${encodePathSegment(groupId)}`, { method: 'PATCH', body })
}

export function deleteGroup(groupId: string): Promise<{ group_id: string; status: string; request_id: string }> {
  return api(`/api/v1/groups/${encodePathSegment(groupId)}`, { method: 'DELETE' })
}

export function listGroupMembers(groupId: string): Promise<{ group_id: string; members: GroupMember[] }> {
  return api(`/api/v1/groups/${encodePathSegment(groupId)}/members`)
}

export function addGroupMember(
  groupId: string,
  body: { principal_id: string; role?: string },
): Promise<{ group_id: string; principal_id: string; role: string; request_id: string }> {
  return api(`/api/v1/groups/${encodePathSegment(groupId)}/members`, { method: 'POST', body })
}

export function patchGroupMember(
  groupId: string,
  principalId: string,
  role: string,
): Promise<{ group_id: string; principal_id: string; role: string; request_id: string }> {
  const path = `/api/v1/groups/${encodePathSegment(groupId)}/members/${encodePathSegment(principalId)}`
  return api(path, { method: 'PATCH', body: { role } })
}

export function removeGroupMember(
  groupId: string,
  principalId: string,
): Promise<{ group_id: string; removed: boolean; request_id: string }> {
  const path = `/api/v1/groups/${encodePathSegment(groupId)}/members/${encodePathSegment(principalId)}`
  return api(path, { method: 'DELETE' })
}

/** Admin audit query (feature gate: `audit`). */
export function listSpaceAuditTrailPath(params: {
  actor?: string
  action?: string
  space?: string
  from?: string
  to?: string
  limit?: number
}): string {
  return `/api/v1/admin/audit${queryString(params)}`
}
