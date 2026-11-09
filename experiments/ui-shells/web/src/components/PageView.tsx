import { useEffect, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { pageById, pagePlainText, pagesOf, spaceOf } from '../data/wiki'
import { useMockStream } from '../lib/useMockStream'
import { ArticleBody, sectionsOf } from './ArticleBody'

function SummaryCard({ summary, pageId }: { summary: string; pageId: string }) {
  const [generation, setGeneration] = useState(0)
  const { shown, done } = useMockStream(summary, { charsPerTick: 4, intervalMs: 20 })

  useEffect(() => {
    setGeneration((g) => g + 1)
  }, [pageId])

  return (
    <section className="rounded-xl border border-line bg-raise p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
          <h2 className="text-[12px] font-semibold tracking-[0.01em] text-fg">AI 摘要</h2>
          <span className="rounded-md border border-line px-1.5 py-0.5 text-[10px] text-fg-3">{done ? '已完成' : '生成中'}</span>
        </div>
        <button
          type="button"
          onClick={() => setGeneration((g) => g + 1)}
          className="rounded-md px-2 py-1 text-[11px] text-fg-2 transition-colors hover:bg-panel hover:text-fg"
        >
          重新生成
        </button>
      </div>
      <p key={generation} className={`mt-2 text-[13.5px] leading-[1.75] text-fg-2 ${done ? '' : 'streaming-caret'}`}>
        {shown}
      </p>
      <div className="mt-2.5 flex items-center gap-2 text-[10.5px] text-fg-3">
        <span>基于本页正文生成</span>
        <span className="h-1 w-1 rounded-full bg-line-strong" />
        <span>未引用外部来源</span>
      </div>
    </section>
  )
}

export function PageView() {
  const { pageId } = useParams<{ pageId: string }>()
  const navigate = useNavigate()
  const page = pageId ? pageById.get(pageId) : undefined

  useEffect(() => {
    if (!page) navigate('/', { replace: true })
  }, [page, navigate])

  if (!page) return null

  const space = spaceOf(page.id)
  const siblings = pagesOf(page.spaceId)
  const index = siblings.findIndex((p) => p.id === page.id)
  const prev = index > 0 ? siblings[index - 1] : undefined
  const next = index >= 0 && index < siblings.length - 1 ? siblings[index + 1] : undefined
  const sections = sectionsOf(page.body)
  const chars = pagePlainText(page).length

  return (
    <article className="mx-auto w-full max-w-5xl px-5 py-6 lg:px-8">
      <div className="mb-4 flex items-center gap-1.5 text-[11px] text-fg-3">
        {space && <span className="text-fg-2">{space.name}</span>}
        <span>/</span>
        <span className="truncate">{page.title}</span>
      </div>

      <header className="mb-5">
        <h1 className="text-[26px] font-semibold leading-tight tracking-[-0.02em] text-fg">{page.title}</h1>
        <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-[11px] text-fg-3">
          <span>更新于 {page.updated}</span>
          <span className="h-1 w-1 rounded-full bg-line-strong" />
          <span>{chars} 字</span>
          <span className="h-1 w-1 rounded-full bg-line-strong" />
          <span>约 {Math.max(1, Math.round(chars / 350))} 分钟读完</span>
          <div className="flex flex-wrap gap-1.5">
            {page.tags.map((tag) => (
              <span key={tag} className="rounded-md border border-line px-1.5 py-0.5 text-[10.5px] text-fg-2">
                #{tag}
              </span>
            ))}
          </div>
        </div>
      </header>

      <div className="grid gap-7 xl:grid-cols-[minmax(0,1fr)_190px]">
        <div className="min-w-0 space-y-5">
          <SummaryCard summary={page.summary} pageId={page.id} />
          <ArticleBody blocks={page.body} />

          <nav className="mt-8 grid gap-2.5 border-t border-line pt-5 sm:grid-cols-2">
            {prev ? (
              <Link
                to={`/p/${prev.id}`}
                className="group rounded-xl border border-line bg-surface p-3 transition-colors hover:border-line-strong hover:bg-panel"
              >
                <div className="text-[10px] uppercase tracking-[0.12em] text-fg-3">上一篇</div>
                <div className="mt-0.5 truncate text-[13px] font-medium text-fg-2 group-hover:text-fg">{prev.title}</div>
              </Link>
            ) : (
              <div className="rounded-xl border border-dashed border-line p-3 text-[11.5px] text-fg-3">已是本空间第一篇</div>
            )}
            {next ? (
              <Link
                to={`/p/${next.id}`}
                className="group rounded-xl border border-line bg-surface p-3 transition-colors hover:border-line-strong hover:bg-panel sm:text-right"
              >
                <div className="text-[10px] uppercase tracking-[0.12em] text-fg-3">下一篇</div>
                <div className="mt-0.5 truncate text-[13px] font-medium text-fg-2 group-hover:text-fg">{next.title}</div>
              </Link>
            ) : (
              <div className="rounded-xl border border-dashed border-line p-3 text-[11.5px] text-fg-3 sm:text-right">已是本空间最后一篇</div>
            )}
          </nav>
        </div>

        <aside className="hidden xl:block">
          <div className="sticky top-2">
            <div className="mb-1.5 text-[10px] font-medium uppercase tracking-[0.14em] text-fg-3">目录</div>
            <ul className="border-l border-line">
              {sections.map((section) => (
                <li key={section.id}>
                  <a
                    href={`#${section.id}`}
                    onClick={(event) => {
                      event.preventDefault()
                      document.getElementById(section.id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
                    }}
                    className="-ml-px block border-l-2 border-transparent py-1 pl-3 text-[12px] leading-5 text-fg-2 transition-colors hover:border-accent hover:text-fg"
                  >
                    {section.text}
                  </a>
                </li>
              ))}
            </ul>
            <div className="mt-4 rounded-lg border border-line bg-panel p-3 text-[11px] leading-[1.7] text-fg-3">
              右侧问答面板可以就本页内容追问，回答带引用。
            </div>
          </div>
        </aside>
      </div>
    </article>
  )
}
