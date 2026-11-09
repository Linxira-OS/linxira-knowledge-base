import { api } from './client'
import type { Space } from './types'

export interface NotesSpaceResponse {
  space: Space
}

/**
 * `POST /api/v1/notes/space` — idempotently ensures the signed-in user's
 * private notes space. The backend names it `notes_<principal>` and returns
 * the same space on every call.
 */
export function ensureMyNotesSpace(): Promise<NotesSpaceResponse> {
  return api<NotesSpaceResponse>('/api/v1/notes/space', { method: 'POST' })
}
