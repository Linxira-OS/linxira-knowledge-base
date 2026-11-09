import type { ReactNode } from 'react'
import type { Block } from '../data/wiki'

export function Inline({ text }: { text: string }): ReactNode {
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean)
  return (
    <>
      {parts.map((part, i) => {
        if (part.startsWith('**') && part.endsWith('**')) {
          return (
            <strong key={i} className="font-semibold text-fg">
              {part.slice(2, -2)}
            </strong>
          )
        }
        if (part.startsWith('`') && part.endsWith('`')) {
          return (
            <code key={i} className="rounded-md border border-line bg-panel px-1.5 py-0.5 font-mono text-[0.86em] text-fg">
              {part.slice(1, -1)}
            </code>
          )
        }
        return <span key={i}>{part}</span>
      })}
    </>
  )
}

export interface Section {
  id: string
  text: string
}

export function sectionsOf(blocks: Block[]): Section[] {
  const out: Section[] = []
  for (const block of blocks) {
    if (block.t === 'h2') out.push({ id: `sec-${out.length}`, text: block.text })
  }
  return out
}

export function ArticleBody({ blocks }: { blocks: Block[] }) {
  let sectionIndex = 0
  return (
    <div className="space-y-5">
      {blocks.map((block, i) => {
        switch (block.t) {
          case 'h2': {
            const id = `sec-${sectionIndex++}`
            return (
              <h2 key={i} id={id} className="scroll-mt-6 pt-4 text-[17px] font-semibold tracking-[-0.01em] text-fg">
                {block.text}
              </h2>
            )
          }
          case 'p':
            return (
              <p key={i} className="text-[14.5px] leading-[1.85] text-fg">
                <Inline text={block.text} />
              </p>
            )
          case 'ul':
            return (
              <ul key={i} className="space-y-2">
                {block.items.map((item, j) => (
                  <li key={j} className="flex gap-2.5 text-[14.5px] leading-[1.85] text-fg">
                    <span className="mt-[0.72em] h-1 w-1 shrink-0 rounded-full bg-fg-3" />
                    <span>
                      <Inline text={item} />
                    </span>
                  </li>
                ))}
              </ul>
            )
          case 'code':
            return (
              <figure key={i} className="overflow-hidden rounded-xl border border-line bg-code-bg">
                <figcaption className="flex items-center justify-between border-b border-line px-4 py-1.5 text-[10.5px] uppercase tracking-[0.12em] text-fg-3">
                  <span>{block.lang}</span>
                  <span>只读</span>
                </figcaption>
                <pre className="thin-scroll overflow-x-auto px-4 py-3.5 text-[12.5px] leading-6 text-code-fg">
                  <code className="font-mono">{block.text}</code>
                </pre>
              </figure>
            )
          case 'quote':
            return (
              <blockquote key={i} className="border-l-2 border-accent-line bg-panel py-3 pl-4 pr-3 text-[14px] leading-[1.8] text-fg-2">
                <Inline text={block.text} />
              </blockquote>
            )
          case 'table':
            return (
              <div key={i} className="thin-scroll overflow-x-auto rounded-xl border border-line">
                <table className="w-full border-collapse text-left text-[13.5px]">
                  <thead className="bg-panel">
                    <tr>
                      {block.head.map((h) => (
                        <th key={h} className="border-b border-line px-3.5 py-2.5 text-[11.5px] font-medium uppercase tracking-[0.08em] text-fg-3">
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {block.rows.map((row, r) => (
                      <tr key={r}>
                        {row.map((cell, c) => (
                          <td key={c} className={`border-b border-line px-3.5 py-2.5 align-top last:border-b-0 ${c === 0 ? 'font-medium text-fg' : 'text-fg-2'}`}>
                            <Inline text={cell} />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
        }
      })}
    </div>
  )
}
