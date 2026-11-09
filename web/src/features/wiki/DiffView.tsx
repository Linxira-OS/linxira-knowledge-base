import { useMemo } from 'react'
import { Badge } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

export interface DiffLine {
  kind: 'same' | 'add' | 'remove'
  text: string
}

/** Above this many lines per side the LCS table is skipped (documented, not silent). */
const MAX_DIFF_LINES = 800

export interface DiffResult {
  lines: DiffLine[]
  added: number
  removed: number
  /** Set when the inputs were too large to diff; the view says so explicitly. */
  skipped: boolean
}

/**
 * Line diff (LCS) between two revision bodies.
 *
 * Both sides come from the history endpoint, which returns each revision's
 * `markdown`, so this is a real diff of stored content — no reconstruction.
 */
export function diffLines(before: string, after: string): DiffResult {
  const a = before.split('\n')
  const b = after.split('\n')
  if (a.length > MAX_DIFF_LINES || b.length > MAX_DIFF_LINES) {
    return { lines: [], added: 0, removed: 0, skipped: true }
  }
  const table: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1])
    }
  }
  const lines: DiffLine[] = []
  let added = 0
  let removed = 0
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      lines.push({ kind: 'same', text: a[i] })
      i += 1
      j += 1
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      lines.push({ kind: 'remove', text: a[i] })
      removed += 1
      i += 1
    } else {
      lines.push({ kind: 'add', text: b[j] })
      added += 1
      j += 1
    }
  }
  while (i < a.length) {
    lines.push({ kind: 'remove', text: a[i] })
    removed += 1
    i += 1
  }
  while (j < b.length) {
    lines.push({ kind: 'add', text: b[j] })
    added += 1
    j += 1
  }
  return { lines, added, removed, skipped: false }
}

export function DiffView({
  before,
  after,
  collapsible = true,
}: {
  before: string
  after: string
  collapsible?: boolean
}) {
  const { t, format } = useI18n()
  const diff = useMemo(() => diffLines(before, after), [before, after])

  if (diff.skipped) {
    return <p className="text-[11.5px] text-fg-3">{t('wiki.history.diffTooLarge', { limit: MAX_DIFF_LINES })}</p>
  }

  const changed = diff.added + diff.removed
  if (changed === 0) {
    return <p className="text-[11.5px] text-fg-3">{t('wiki.history.diffIdentical')}</p>
  }

  const visible = collapsible ? trimContext(diff.lines) : diff.lines

  return (
    <div>
      <div className="mb-1.5 flex items-center gap-2 text-[11px] text-fg-3">
        <Badge tone="accent">+{format.number(diff.added)}</Badge>
        <Badge tone="danger">−{format.number(diff.removed)}</Badge>
        <span>{t('wiki.history.diffLines')}</span>
      </div>
      <pre className="thin-scroll max-h-[420px] overflow-auto rounded-lg border border-line bg-code-bg px-0 py-2 font-mono text-[11.5px] leading-5 text-code-fg">
        {visible.map((line, index) => (
          <div
            key={index}
            className={
              line.kind === 'add'
                ? 'bg-accent-weak px-3 text-accent'
                : line.kind === 'remove'
                  ? 'bg-danger-weak px-3 text-danger'
                  : 'px-3 text-fg-3'
            }
          >
            {line.kind === 'add' ? '+ ' : line.kind === 'remove' ? '− ' : '  '}
            {line.text || ' '}
          </div>
        ))}
      </pre>
    </div>
  )
}

/** Keeps two lines of context around changes so long pages stay readable. */
function trimContext(lines: DiffLine[], context = 2): DiffLine[] {
  const keep = new Array<boolean>(lines.length).fill(false)
  lines.forEach((line, index) => {
    if (line.kind === 'same') return
    for (let i = Math.max(0, index - context); i <= Math.min(lines.length - 1, index + context); i += 1) {
      keep[i] = true
    }
  })
  const result: DiffLine[] = []
  let skipping = false
  lines.forEach((line, index) => {
    if (keep[index]) {
      if (skipping) {
        result.push({ kind: 'same', text: '…' })
        skipping = false
      }
      result.push(line)
    } else {
      skipping = true
    }
  })
  return result
}
