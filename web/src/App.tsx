// SPDX-License-Identifier: MIT OR Apache-2.0
import { Suspense, lazy } from 'react'
import { Navigate, Outlet, Route, Routes, useLocation } from 'react-router-dom'
import { AppShell } from './components/AppShell'
import { Button, EmptyState, LoadingState } from './components/ui'
import { QueryProvider } from './features/query/QueryProvider'
import { useI18n } from './lib/i18n'
import { useSession } from './lib/session'

/**
 * Route-level code splitting: the shell, providers and search entry stay in the
 * initial chunk; the heavier secondary views (wiki, spaces, admin,
 * conversations, login) load on demand.
 */
const SearchPage = lazy(async () => ({ default: (await import('./pages/SearchPage')).SearchPage }))
const LoginPage = lazy(async () => ({ default: (await import('./pages/LoginPage')).LoginPage }))
const ConversationsPage = lazy(async () => ({ default: (await import('./pages/ConversationsPage')).ConversationsPage }))
const NotesPage = lazy(async () => ({ default: (await import('./pages/NotesPage')).NotesPage }))
const SpacesListPage = lazy(async () => ({ default: (await import('./pages/spaces/SpacesListPage')).SpacesListPage }))
const SpaceDetailPage = lazy(async () => ({ default: (await import('./pages/spaces/SpaceDetailPage')).SpaceDetailPage }))
const WikiReaderPage = lazy(async () => ({ default: (await import('./pages/wiki/WikiReaderPage')).WikiReaderPage }))
const WikiEditorPage = lazy(async () => ({ default: (await import('./pages/wiki/WikiEditorPage')).WikiEditorPage }))
const WikiCreatePage = lazy(async () => ({ default: (await import('./pages/wiki/WikiEditorPage')).WikiCreatePage }))
const WikiHistoryPage = lazy(async () => ({ default: (await import('./pages/wiki/WikiHistoryPage')).WikiHistoryPage }))
const AdminPage = lazy(async () => ({ default: (await import('./pages/admin/AdminPage')).AdminPage }))

function NotFound() {
  const { t } = useI18n()
  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-10">
      <EmptyState
        title={t('state.notFound')}
        actions={
          <Button variant="primary" onClick={() => window.history.back()}>
            {t('action.back')}
          </Button>
        }
      />
    </div>
  )
}

/** Gates the application routes when the deployment requires credentials. */
function RequireAuth() {
  const session = useSession()
  const location = useLocation()

  if (!session.ready) {
    return (
      <div className="grid h-full place-items-center bg-canvas">
        <LoadingState />
      </div>
    )
  }

  if (session.authRequired && session.status === 'anonymous') {
    const redirect = `${location.pathname}${location.search}`
    return <Navigate to={`/login?redirect=${encodeURIComponent(redirect)}`} replace />
  }

  return (
    <AppShell>
      <Outlet />
    </AppShell>
  )
}

export function App() {
  return (
    <QueryProvider>
      <Suspense
        fallback={
          <div className="grid h-full place-items-center bg-canvas">
            <LoadingState />
          </div>
        }
      >
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route element={<RequireAuth />}>
            <Route path="/" element={<Navigate to="/search" replace />} />
            <Route path="/search" element={<SearchPage />} />
            <Route path="/conversations" element={<ConversationsPage />} />
            <Route path="/notes" element={<NotesPage />} />
            <Route path="/spaces" element={<SpacesListPage />} />
            <Route path="/spaces/:spaceId" element={<SpaceDetailPage />} />
            <Route path="/w/:space/:slug" element={<WikiReaderPage />} />
            <Route path="/w/:space/:slug/edit" element={<WikiEditorPage />} />
            <Route path="/w/:space/:slug/history" element={<WikiHistoryPage />} />
            <Route path="/create/:space" element={<WikiCreatePage />} />
            <Route path="/admin" element={<AdminPage />} />
            <Route path="/admin/:tab" element={<AdminPage />} />
            <Route path="*" element={<NotFound />} />
          </Route>
        </Routes>
      </Suspense>
    </QueryProvider>
  )
}
