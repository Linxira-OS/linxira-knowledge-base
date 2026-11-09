import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import { App } from './App'
import { ApiError } from './api/client'
import { I18nProvider } from './lib/i18n'
import { SessionProvider } from './lib/session'
import './index.css'

/**
 * TanStack Query owns server state: it gives every panel one cache, one retry
 * policy and cache invalidation after mutations. 4xx answers are never retried
 * (they are deterministic); 429/503 retry once because the backend sets
 * `Retry-After` and these are transient by design.
 */
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 15_000,
      refetchOnWindowFocus: false,
      retry: (failureCount, error) => {
        if (error instanceof ApiError) {
          if (error.status === 401 || error.status === 403 || error.status === 404) return false
          if (error.status === 429 || error.status === 503) return failureCount < 1
          if (error.status >= 400 && error.status < 500) return false
        }
        return failureCount < 2
      },
    },
    mutations: { retry: false },
  },
})

const container = document.getElementById('root')
if (!container) throw new Error('#root missing')

// Session refresh runs before the tree mounts so the first render already knows
// whether this deployment needs a sign-in.
createRoot(container).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <SessionProvider>
          <BrowserRouter>
            <App />
          </BrowserRouter>
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>
  </StrictMode>,
)
