import { useMemo } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { answerQuestion, searchPages, spaceOf } from '../data/wiki'
import { useMockStream } from '../lib/useMockStream'

function AiAnswer({ question }: { question: string }) {
  const result = useMemo(() => answerQuestion(question), [question])
  const { shown, done } = useMockStream(result.answer, { charsPerTick: 4, intervalMs: 16 })

  return (
    <section className="mt-4 rounded-xl border border-line bg-raise p-4">
      <div className="flex items-center gap-2">
        <span className="h-1.5 w-1.5 rounded-full bg-accent" />
        <span className="text-[12px] font-medium text-fg">AI 回答</span>
        {result.degraded && (
          <span className="rounded-md border border-line bg-warn-weak px-1.5 py-0.5 text-[10px] text-warn">抽取式</span>
        )}
      </div>
      <p className={`mt-2 whitespace-pre-wrap text-[13.5px] leading-[1.8] text-fg ${done ? '' : 'streaming-caret'}`}>{shown}</p>
      {done && result.citations.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {result.citations.map((citation, i) => (
            <Link
              key={`${citation.pageId}-${i}`}
              to={`/p/${citation.pageId}`}
              className="rounded-lg border border-line px-2 py-1 text-[11px] text-fg-2 transition-colors hover:border-accent-line hover:text-accent"
            >
              [{i + 1}] {citation.title}
            </Link>
          ))}
        </div>
      )}
    </section>
  )
}

export function SearchView({ onAsk }: { onAsk: (question: string) => void }) {
  const [params] = useSearchParams()
  const query = (params.get('q') ?? '').trim()
  const hits = useMemo(() => searchPages(query, 8), [query])

  return (
    <div className="mx-auto w-full max-w-3xl px-5 py-6 lg:px-8">
      <div className="text-[11px] uppercase tracking-[0.12em] text-fg-3">检索</div>
      <h1 className="mt-1 text-[22px] font-semibold tracking-[-0.02em] text-fg">「{query || '空查询'}」</h1>
      <div className="mt-1 text-[11px] text-fg-3">命中 {hits.length} 个页面 · 索引为本地模拟数据</div>

      {query ? (
        <AiAnswer question={query} />
      ) : (
        <div className="mt-6 rounded-xl border border-dashed border-line p-6 text-[13px] text-fg-2">在顶部搜索框输入关键词后回车。</div>
      )}

      {query && (
        <section className="mt-6">
          <div className="mb-2 flex items-center justify-between">
            <h2 className="text-[10px] font-medium uppercase tracking-[0.14em] text-fg-3">页面与段落</h2>
            <button
              type="button"
              onClick={() => onAsk(query)}
              className="rounded-lg border border-line px-2 py-1 text-[11px] text-fg-2 transition-colors hover:border-accent-line hover:text-accent"
            >
              在问答面板追问 →
            </button>
          </div>
          <ul className="space-y-2">
            {hits.map((hit) => (
              <li key={hit.page.id}>
                <Link
                  to={`/p/${hit.page.id}`}
                  className="block rounded-xl border border-line bg-surface p-3 transition-colors hover:border-line-strong hover:bg-panel"
                >
                  <div className="flex items-baseline justify-between gap-3">
                    <span className="text-[13.5px] font-medium text-fg">{hit.page.title}</span>
                    <span className="shrink-0 text-[10.5px] text-fg-3">
                      {spaceOf(hit.page.id)?.name} · 相关度 {hit.score}
                    </span>
                  </div>
                  <p className="mt-1 line-clamp-2 text-[12px] leading-5 text-fg-2">{hit.snippet}</p>
                </Link>
              </li>
            ))}
            {hits.length === 0 && (
              <li className="rounded-xl border border-dashed border-line p-5 text-[12.5px] text-fg-2">
                没有页面命中。试用「ACL」「RRF」「doc_id」「Tauri」等词。
              </li>
            )}
          </ul>
        </section>
      )}
    </div>
  )
}
