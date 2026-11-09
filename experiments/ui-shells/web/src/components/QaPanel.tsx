import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { suggestedQuestions } from '../data/wiki'
import type { QaMessage } from '../lib/qa'
import { useMockStream } from '../lib/useMockStream'

function AssistantBody({ message, onGrow }: { message: QaMessage; onGrow: () => void }) {
  const { shown, done } = useMockStream(message.text, { charsPerTick: 3, intervalMs: 18 })
  const navigate = useNavigate()

  useEffect(() => {
    onGrow()
  }, [shown, done, onGrow])

  return (
    <div className="rounded-xl border border-line bg-raise p-3">
      <p className={`whitespace-pre-wrap text-[12.5px] leading-[1.8] text-fg ${done ? '' : 'streaming-caret'}`}>{shown}</p>
      {done && message.degraded && (
        <div className="mt-2 rounded-lg border border-line bg-warn-weak px-2 py-1.5 text-[10.5px] leading-4 text-warn">
          降级：{message.degraded}
        </div>
      )}
      {done && message.citations && message.citations.length > 0 && (
        <div className="mt-2.5 border-t border-line pt-2">
          <div className="mb-1 text-[10px] font-medium uppercase tracking-[0.12em] text-fg-3">引用 {message.citations.length} 条</div>
          <ul className="space-y-0.5">
            {message.citations.map((citation, i) => (
              <li key={`${citation.pageId}-${i}`}>
                <button
                  type="button"
                  onClick={() => navigate(`/p/${citation.pageId}`)}
                  className="group flex w-full items-start gap-1.5 rounded-lg px-1.5 py-1 text-left transition-colors hover:bg-panel"
                >
                  <span className="mt-px grid h-4 w-4 shrink-0 place-items-center rounded-md border border-line text-[9px] text-fg-3 group-hover:border-accent-line group-hover:text-accent">
                    {i + 1}
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-[11.5px] font-medium text-fg-2 group-hover:text-fg">{citation.title}</span>
                    <span className="line-clamp-2 text-[10.5px] leading-4 text-fg-3">{citation.quote}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

export function QaPanel({ messages, onAsk }: { messages: QaMessage[]; onAsk: (question: string) => void }) {
  const [draft, setDraft] = useState('')
  const scrollRef = useRef<HTMLDivElement>(null)
  const pinnedRef = useRef(true)

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current
    if (!el || !pinnedRef.current) return
    el.scrollTop = el.scrollHeight
  }, [])

  useEffect(() => {
    pinnedRef.current = true
    scrollToBottom()
  }, [messages.length, scrollToBottom])

  const submit = () => {
    const question = draft.trim()
    if (!question) return
    setDraft('')
    onAsk(question)
  }

  return (
    <aside className="hidden w-[336px] shrink-0 flex-col border-l border-line bg-panel xl:flex">
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-line px-3.5">
        <div className="flex items-center gap-2">
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
          <span className="text-[12px] font-medium text-fg">知识库问答</span>
        </div>
        <span className="rounded-md border border-line px-1.5 py-0.5 text-[10px] text-fg-3">演示数据</span>
      </div>

      <div
        ref={scrollRef}
        onScroll={(e) => {
          const el = e.currentTarget
          pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48
        }}
        className="thin-scroll flex-1 space-y-3 overflow-y-auto px-3 py-3"
      >
        {messages.map((message) =>
          message.role === 'user' ? (
            <div key={message.id} className="flex justify-end">
              <div className="max-w-[92%] rounded-xl rounded-br-sm border border-accent-line bg-accent-weak px-3 py-2 text-[12.5px] leading-5 text-fg">
                {message.text}
              </div>
            </div>
          ) : (
            <AssistantBody key={message.id} message={message} onGrow={scrollToBottom} />
          ),
        )}

        {messages.length <= 1 && (
          <div className="space-y-1.5 pt-1">
            <div className="px-1 text-[10px] font-medium uppercase tracking-[0.12em] text-fg-3">推荐问题</div>
            {suggestedQuestions.map((question) => (
              <button
                key={question}
                type="button"
                onClick={() => onAsk(question)}
                className="block w-full rounded-lg border border-line bg-surface px-2.5 py-2 text-left text-[11.5px] leading-4 text-fg-2 transition-colors hover:border-line-strong hover:text-fg"
              >
                {question}
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-line p-2.5">
        <div className="flex items-end gap-1.5 rounded-lg border border-line bg-surface px-2 py-1.5 transition-colors focus-within:border-accent-line">
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submit()
            }}
            placeholder="继续追问…"
            aria-label="提问"
            className="min-w-0 flex-1 bg-transparent text-[12.5px] text-fg outline-none placeholder:text-fg-3"
          />
          <button
            type="button"
            onClick={submit}
            disabled={draft.trim().length === 0}
            className="rounded-md bg-accent px-2.5 py-1 text-[11.5px] font-medium text-on-accent transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:bg-line disabled:text-fg-3"
          >
            发送
          </button>
        </div>
        <div className="mt-1.5 px-1 text-[10px] text-fg-3">回答仅来自库内 7 个页面 · 带引用</div>
      </div>
    </aside>
  )
}
