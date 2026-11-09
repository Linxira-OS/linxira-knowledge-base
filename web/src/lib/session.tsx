import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { ApiError, onUnauthorized } from '../api/client'
import { authLogin, authLogout, authMe, type Principal } from '../api/auth'
import { getFeatures } from '../api/system'
import type { FeaturesResponse } from '../api/types'
import { useI18n } from './i18n'

export type SessionStatus = 'loading' | 'anonymous' | 'authenticated'

interface SessionValue {
  status: SessionStatus
  features: FeaturesResponse | null
  principal: Principal | null
  /** Display label the backend reports for the signed-in principal. */
  principalLabel: string | null
  /** False while the session state is still unknown. */
  ready: boolean
  /**
   * True when `/api/v1/auth/me` answered 401 — i.e. this deployment protects
   * its API with credentials and the visitor must sign in. Solo
   * (`auth.mode = none`) never sets it.
   */
  authRequired: boolean
  login: (username: string, password: string) => Promise<void>
  logout: () => Promise<void>
  refresh: () => Promise<void>
  /** Set when a 401 arrived after a real session (session expired mid-flight). */
  expired: boolean
}

const SessionContext = createContext<SessionValue | null>(null)

export function SessionProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<SessionStatus>('loading')
  const [features, setFeatures] = useState<FeaturesResponse | null>(null)
  const [principal, setPrincipal] = useState<Principal | null>(null)
  const [expired, setExpired] = useState(false)
  const [authRequired, setAuthRequired] = useState(false)
  // "Expired" must only be reported for a visitor whose session actually ended,
  // not for a first, unauthenticated visit.
  const wasAuthenticated = useRef(false)
  const queryClient = useQueryClient()

  const refresh = useCallback(async () => {
    try {
      const envelope = await authMe()
      setPrincipal(envelope.principal)
      setStatus('authenticated')
      setExpired(false)
      setAuthRequired(false)
      wasAuthenticated.current = true
      try {
        setFeatures(await getFeatures())
      } catch {
        // The session is valid but the mode/i18n probe failed independently;
        // the session must not be downgraded because of it.
        setFeatures(null)
      }
    } catch (error) {
      setPrincipal(null)
      setFeatures(null)
      if (error instanceof ApiError && error.status === 401) {
        setAuthRequired(true)
        setExpired(wasAuthenticated.current)
      }
      setStatus('anonymous')
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(
    () =>
      onUnauthorized(() => {
        setExpired(wasAuthenticated.current)
        setPrincipal(null)
        setStatus('anonymous')
      }),
    [],
  )

  const login = useCallback(
    async (username: string, password: string) => {
      // `api()` attaches X-CSRF-Token (bootstrapped from /api/v1/auth/csrf).
      await authLogin(username, password)
      await refresh()
      // Anything that failed while anonymous (e.g. the space list) must refetch
      // under the new session without a page reload.
      void queryClient.invalidateQueries()
    },
    [refresh, queryClient],
  )

  const logout = useCallback(async () => {
    try {
      await authLogout()
    } finally {
      wasAuthenticated.current = false
      setPrincipal(null)
      setExpired(false)
      await refresh()
    }
  }, [refresh])

  const principalLabel = principal ? principal.display_name || principal.user_id : null

  const value = useMemo<SessionValue>(
    () => ({
      status,
      features,
      principal,
      principalLabel,
      ready: status !== 'loading',
      authRequired,
      login,
      logout,
      refresh,
      expired,
    }),
    [status, features, principal, principalLabel, authRequired, login, logout, refresh, expired],
  )

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>
}

export function useSession(): SessionValue {
  const value = useContext(SessionContext)
  if (!value) throw new Error('useSession must be used inside <SessionProvider>')
  return value
}

/** Localised message for an auth failure (rate limit vs. bad credentials). */
export function useLoginErrorMessage(): (error: unknown) => string {
  const { t } = useI18n()
  return useCallback(
    (error: unknown) => {
      if (error instanceof ApiError) {
        if (error.status === 429) return t('account.login.rateLimited')
        if (error.code === 'error.unauthorized' || error.status === 401) {
          return error.serverMessage || t('account.login.failed')
        }
        if (error.serverMessage) return error.serverMessage
        return t('account.login.failed')
      }
      return t('account.login.failed')
    },
    [t],
  )
}
