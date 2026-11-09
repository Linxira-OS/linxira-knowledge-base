import { useEffect, useState, type FormEvent } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router-dom'
import { ensureCsrfToken } from '../api/client'
import { Banner, Button, Card, Field } from '../components/ui'
import { useI18n } from '../lib/i18n'
import { ISSUES_URL, REPOSITORY_URL, SECURITY_URL } from '../lib/repo'
import { useLoginErrorMessage, useSession } from '../lib/session'

const inputClass =
  'w-full rounded-lg border border-line bg-canvas px-2.5 py-[5px] text-[13px] text-fg placeholder:text-fg-3'

/**
 * Sign-in for session deployments. Solo mode (`auth.mode = none`) usually
 * never reaches this page; when the backend reports it anyway, the banner
 * points visitors back to the Wiki.
 */
export function LoginPage() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const { status, features, expired, login } = useSession()
  const errorText = useLoginErrorMessage()

  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<unknown>(null)
  const [csrfPending, setCsrfPending] = useState(true)

  const rawRedirect = searchParams.get('redirect')
  const redirect = rawRedirect && rawRedirect.startsWith('/') && !rawRedirect.startsWith('//') ? rawRedirect : '/search'

  // Prime the double-submit cookie the login helper needs (it also fetches one
  // lazily); showing the hint keeps the form honest about what it is waiting for.
  useEffect(() => {
    let active = true
    void ensureCsrfToken()
      .catch(() => undefined)
      .finally(() => {
        if (active) setCsrfPending(false)
      })
    return () => {
      active = false
    }
  }, [])

  useEffect(() => {
    if (status === 'authenticated') navigate(redirect, { replace: true })
  }, [status, redirect, navigate])

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (submitting) return
    setError(null)
    setSubmitting(true)
    void login(username, password)
      .catch((failure: unknown) => setError(failure))
      .finally(() => setSubmitting(false))
  }

  return (
    <div className="flex min-h-full items-center justify-center bg-canvas px-6 py-10">
      <div className="w-full max-w-[360px] space-y-3">
        {expired && <Banner tone="warn" title={t('account.sessionExpired')} />}
        {features?.mode === 'solo' && (
          <Banner
            tone="neutral"
            actions={
              <Link to="/search" className="text-[11.5px] font-medium text-accent">
                {t('account.login.backToWiki')}
              </Link>
            }
          >
            {t('account.login.soloMode')}
          </Banner>
        )}

        <Card title={t('app.name')} description={t('app.tagline')}>
          <form onSubmit={onSubmit} className="space-y-3">
            <Field label={t('account.login.username')} htmlFor="login-username">
              <input
                id="login-username"
                name="username"
                type="text"
                autoComplete="username"
                required
                value={username}
                onChange={(event) => {
                  setUsername(event.target.value)
                  setError(null)
                }}
                className={inputClass}
              />
            </Field>
            <Field label={t('account.login.password')} htmlFor="login-password">
              <input
                id="login-password"
                name="password"
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(event) => {
                  setPassword(event.target.value)
                  setError(null)
                }}
                className={inputClass}
              />
            </Field>

            {error !== null && (
              <p role="alert" className="text-[11.5px] leading-5 text-danger">
                {errorText(error)}
              </p>
            )}

            <Button type="submit" variant="primary" className="w-full" disabled={submitting || csrfPending}>
              {submitting ? t('account.login.submitting') : t('account.login.submit')}
            </Button>

            {csrfPending && <p className="text-[11px] text-fg-3">{t('account.login.needCsrf')}</p>}
            {status === 'authenticated' && <p className="text-[11px] text-fg-3">{t('account.login.continue')}</p>}
          </form>
        </Card>

        <p className="flex flex-wrap items-center justify-center gap-x-2 text-[11px] text-fg-3">
          <a
            className="text-fg-2 transition-colors hover:text-fg"
            href={REPOSITORY_URL}
            target="_blank"
            rel="noopener noreferrer"
          >
            GitHub
          </a>
          <span aria-hidden="true">·</span>
          <a
            className="text-fg-2 transition-colors hover:text-fg"
            href={ISSUES_URL}
            target="_blank"
            rel="noopener noreferrer"
          >
            Issues
          </a>
          <span aria-hidden="true">·</span>
          <a
            className="text-fg-2 transition-colors hover:text-fg"
            href={SECURITY_URL}
            target="_blank"
            rel="noopener noreferrer"
          >
            Security
          </a>
        </p>
      </div>
    </div>
  )
}
