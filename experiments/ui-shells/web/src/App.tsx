import { useCallback, useState } from 'react'
import { HashRouter, Navigate, Route, Routes } from 'react-router-dom'
import { pages } from './data/wiki'
import { PageView } from './components/PageView'
import { QaPanel } from './components/QaPanel'
import { SearchView } from './components/SearchView'
import { Sidebar } from './components/Sidebar'
import { TopBar } from './components/TopBar'
import { assistantMessage, greeting, userMessage, type QaMessage } from './lib/qa'

function Shell() {
  const [messages, setMessages] = useState<QaMessage[]>([greeting])

  const ask = useCallback((question: string) => {
    const trimmed = question.trim()
    if (!trimmed) return
    setMessages((prev) => [...prev, userMessage(trimmed), assistantMessage(trimmed)])
  }, [])

  return (
    <div className="flex h-full flex-col overflow-hidden bg-canvas text-fg">
      <TopBar onAsk={ask} />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="thin-scroll min-w-0 flex-1 overflow-y-auto bg-surface">
          <Routes>
            <Route path="/" element={<Navigate to={`/p/${pages[0].id}`} replace />} />
            <Route path="/p/:pageId" element={<PageView />} />
            <Route path="/search" element={<SearchView onAsk={ask} />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </main>
        <QaPanel messages={messages} onAsk={ask} />
      </div>
    </div>
  )
}

export default function App() {
  return (
    <HashRouter>
      <Shell />
    </HashRouter>
  )
}
