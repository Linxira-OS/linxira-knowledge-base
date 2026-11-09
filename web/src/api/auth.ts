import { api } from './client'

/**
 * Session/auth surface (JSON).
 *
 * `GET /api/v1/auth/csrf` issues the double-submit cookie; `login`/`logout` are
 * ordinary writes, so `api()` attaches `X-CSRF-Token` automatically. The
 * session itself is an HttpOnly cookie the browser sends from then on.
 */

export interface Principal {
  user_id: string
  display_name: string
  tenant_id: string
  allowed_space_ids: string[]
  kind: string
  scope: string
  tenant_role: string
  token_id: string | null
}

export interface AuthEnvelope {
  principal: Principal
  /** Flat feature set (`FeatureSet::to_json`); mode/i18n come from /api/v1/features. */
  features: Record<string, boolean>
}

/** `GET /api/v1/auth/me` — 401 when the visitor is not authenticated. */
export function authMe(): Promise<AuthEnvelope> {
  return api<AuthEnvelope>('/api/v1/auth/me')
}

/** `POST /api/v1/auth/login` — sets the HttpOnly session cookie. */
export function authLogin(username: string, password: string): Promise<AuthEnvelope> {
  return api<AuthEnvelope>('/api/v1/auth/login', {
    method: 'POST',
    body: { username, password },
  })
}

/** `POST /api/v1/auth/logout`. */
export function authLogout(): Promise<{ logged_out: boolean }> {
  return api<{ logged_out: boolean }>('/api/v1/auth/logout', { method: 'POST' })
}
