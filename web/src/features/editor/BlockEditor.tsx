import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type DragEvent,
  type KeyboardEvent,
  type ReactNode,
} from 'react'
import { Card, cx } from '../../components/ui'
import { useI18n, type MessageKey } from '../../lib/i18n'
import { renderMarkdown } from '../../lib/markdown'
import { pluginRegistry } from '../../plugins/registry'
import {
  blockKind,
  createBlock,
  parseMarkdownBlocks,
  retypeBlock,
  serializeMarkdownBlocks,
  type BlockKind,
  type MarkdownBlock,
} from './blocks'

export type EditorView = 'markdown' | 'blocks'

export interface BlockEditorProps {
  /** Markdown is the single source of truth; blocks are only a view of it. */
  value: string
  onChange: (markdown: string) => void
  /** The Wiki keeps the existing Markdown split view by default. */
  defaultView?: EditorView
  className?: string
}

const BLOCK_KINDS: readonly BlockKind[] = [
  'paragraph',
  'heading1',
  'heading2',
  'heading3',
  'bulleted',
  'numbered',
  'quote',
  'code',
  'divider',
]

const KIND_LABELS: Record<BlockKind, MessageKey> = {
  paragraph: 'wiki.editor.blocks.kind.paragraph',
  heading1: 'wiki.editor.blocks.kind.heading1',
  heading2: 'wiki.editor.blocks.kind.heading2',
  heading3: 'wiki.editor.blocks.kind.heading3',
  bulleted: 'wiki.editor.blocks.kind.bulleted',
  numbered: 'wiki.editor.blocks.kind.numbered',
  quote: 'wiki.editor.blocks.kind.quote',
  code: 'wiki.editor.blocks.kind.code',
  divider: 'wiki.editor.blocks.kind.divider',
}

interface ToolButtonProps {
  label: string
  onPress: () => void
  disabled?: boolean
  children: ReactNode
}

/** Small toolbar control; `mousedown` is cancelled to keep textarea selection. */
function ToolButton({ label, onPress, disabled, children }: ToolButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onPress}
      className="inline-flex h-6 min-w-6 items-center justify-center rounded-md px-1 text-[11.5px] leading-none text-fg-3 transition-colors hover:bg-panel hover:text-fg disabled:cursor-not-allowed disabled:opacity-40"
    >
      {children}
    </button>
  )
}

interface BlockRowProps {
  index: number
  total: number
  block: MarkdownBlock
  focusToken: number | undefined
  pluginRevision: number
  dragActive: boolean
  dragging: boolean
  dropTarget: boolean
  onChangeText: (index: number, text: string) => void
  onChangeKind: (index: number, kind: BlockKind) => void
  onAddAfter: (index: number) => void
  onMove: (index: number, direction: -1 | 1) => void
  onDelete: (index: number) => void
  onDragStart: (index: number) => void
  onDragOver: (index: number) => void
  onDragEnd: () => void
  onDrop: (index: number, event: DragEvent) => void
}

const BlockRow = memo(function BlockRow({
  index,
  total,
  block,
  focusToken,
  pluginRevision,
  dragActive,
  dragging,
  dropTarget,
  onChangeText,
  onChangeKind,
  onAddAfter,
  onMove,
  onDelete,
  onDragStart,
  onDragOver,
  onDragEnd,
  onDrop,
}: BlockRowProps) {
  const { t } = useI18n()
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const kind = blockKind(block.text)
  const blockNumber = index + 1

  useLayoutEffect(() => {
    const textarea = textareaRef.current
    if (!textarea) return
    textarea.style.height = 'auto'
    textarea.style.height = `${Math.max(textarea.scrollHeight + 2, 64)}px`
  }, [block.text])

  useEffect(() => {
    if (focusToken === undefined) return
    const textarea = textareaRef.current
    if (!textarea) return
    textarea.focus()
    textarea.setSelectionRange(textarea.value.length, textarea.value.length)
  }, [focusToken])

  const customRender = useMemo(() => {
    void pluginRevision
    return pluginRegistry.resolveBlockRenderer({ source: block.text, kind, index })
  }, [block.text, kind, index, pluginRevision])

  const previewHtml = useMemo(
    () => customRender?.html ?? renderMarkdown(block.text),
    [customRender, block.text],
  )

  const wrapSelection = (before: string, after: string, placeholder: string) => {
    const textarea = textareaRef.current
    if (!textarea) return
    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const selected = textarea.value.slice(start, end) || placeholder
    const next = `${textarea.value.slice(0, start)}${before}${selected}${after}${textarea.value.slice(end)}`
    onChangeText(index, next)
    requestAnimationFrame(() => {
      textarea.focus()
      textarea.setSelectionRange(start + before.length, start + before.length + selected.length)
    })
  }

  const insertLink = () => {
    const textarea = textareaRef.current
    if (!textarea) return
    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const label = textarea.value.slice(start, end) || t('wiki.editor.blocks.textPlaceholder')
    const url = 'https://'
    const next = `${textarea.value.slice(0, start)}[${label}](${url})${textarea.value.slice(end)}`
    onChangeText(index, next)
    requestAnimationFrame(() => {
      textarea.focus()
      const urlStart = start + label.length + 3
      textarea.setSelectionRange(urlStart, urlStart + url.length)
    })
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault()
      onAddAfter(index)
    }
  }

  const dragLabel = t('wiki.editor.blocks.drag', { index: blockNumber })

  return (
    <section
      aria-label={t('wiki.editor.blocks.blockLabel', { index: blockNumber })}
      className={cx(
        'min-w-0 rounded-xl border bg-surface transition-colors',
        dropTarget ? 'border-accent' : 'border-line',
        dragging && 'opacity-55',
      )}
      onDragOver={(event) => {
        if (!dragActive) return
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
        onDragOver(index)
      }}
      onDrop={(event) => onDrop(index, event)}
    >
      <div className="flex flex-wrap items-center gap-1 border-b border-line px-2 py-1">
        <button
          type="button"
          draggable
          aria-label={dragLabel}
          title={dragLabel}
          onDragStart={(event) => {
            event.dataTransfer.effectAllowed = 'move'
            event.dataTransfer.setData('text/plain', String(index))
            onDragStart(index)
          }}
          onDragEnd={onDragEnd}
          className="inline-flex h-6 cursor-grab items-center justify-center rounded-md px-1 text-[12px] leading-none text-fg-3 hover:bg-panel hover:text-fg active:cursor-grabbing"
        >
          ⋮⋮
        </button>

        <label className="contents">
          <span className="sr-only">{t('wiki.editor.blocks.type')}</span>
          <select
            value={kind}
            aria-label={t('wiki.editor.blocks.type')}
            onChange={(event) => onChangeKind(index, event.target.value as BlockKind)}
            className="h-6 w-auto max-w-[150px] rounded-md px-1 py-0 text-[11.5px]"
          >
            {BLOCK_KINDS.map((option) => (
              <option key={option} value={option}>
                {t(KIND_LABELS[option])}
              </option>
            ))}
          </select>
        </label>

        <span aria-hidden="true" className="mx-0.5 h-4 w-px bg-line" />

        <ToolButton label={t('wiki.editor.blocks.bold')} onPress={() => wrapSelection('**', '**', t('wiki.editor.blocks.textPlaceholder'))}>
          <span className="font-semibold">B</span>
        </ToolButton>
        <ToolButton label={t('wiki.editor.blocks.italic')} onPress={() => wrapSelection('*', '*', t('wiki.editor.blocks.textPlaceholder'))}>
          <span className="italic">I</span>
        </ToolButton>
        <ToolButton label={t('wiki.editor.blocks.inlineCode')} onPress={() => wrapSelection('`', '`', 'code')}>
          <span className="font-mono text-[10.5px]">code</span>
        </ToolButton>
        <ToolButton label={t('wiki.editor.blocks.link')} onPress={insertLink}>
          <span className="font-mono text-[10.5px]">[]()</span>
        </ToolButton>

        <div className="ml-auto flex items-center gap-0.5">
          <ToolButton
            label={t('wiki.editor.blocks.add', { index: blockNumber })}
            onPress={() => onAddAfter(index)}
          >
            +
          </ToolButton>
          <ToolButton
            label={t('wiki.editor.blocks.moveUp', { index: blockNumber })}
            disabled={index === 0}
            onPress={() => onMove(index, -1)}
          >
            ↑
          </ToolButton>
          <ToolButton
            label={t('wiki.editor.blocks.moveDown', { index: blockNumber })}
            disabled={index === total - 1}
            onPress={() => onMove(index, 1)}
          >
            ↓
          </ToolButton>
          <ToolButton
            label={t('wiki.editor.blocks.delete', { index: blockNumber })}
            onPress={() => onDelete(index)}
          >
            ×
          </ToolButton>
        </div>
      </div>

      <div className="grid min-w-0 lg:grid-cols-2">
        <textarea
          ref={textareaRef}
          value={block.text}
          aria-label={t('wiki.editor.blocks.blockLabel', { index: blockNumber })}
          placeholder={t('wiki.editor.blocks.placeholder')}
          spellCheck={false}
          onChange={(event) => onChangeText(index, event.target.value)}
          onKeyDown={handleKeyDown}
          className="thin-scroll min-h-[64px] w-full resize-none overflow-hidden border-0 bg-transparent px-3 py-2 font-mono text-[12.5px] leading-6"
        />
        <div className="min-w-0 border-t border-line bg-panel px-3 py-2 lg:border-l lg:border-t-0">
          <div className="mb-1 flex min-h-4 items-center justify-between gap-2">
            <span className="text-[9.5px] uppercase tracking-wide text-fg-3">
              {t('wiki.editor.blocks.preview')}
            </span>
            {customRender && (
              <span className="truncate text-[9.5px] text-accent" title={customRender.rendererName}>
                {t('wiki.editor.blocks.pluginBadge', { name: customRender.pluginName })}
              </span>
            )}
          </div>
          {previewHtml.trim() === '' ? (
            <p className="py-1 text-[11px] text-fg-3">{t('wiki.editor.blocks.previewEmpty')}</p>
          ) : (
            <div
              className={cx('prose-lkb thin-scroll max-h-[240px] min-w-0 overflow-auto', customRender?.className)}
              dangerouslySetInnerHTML={{ __html: previewHtml }}
            />
          )}
        </div>
      </div>
    </section>
  )
})

export function BlockEditor({ value, onChange, defaultView = 'markdown', className }: BlockEditorProps) {
  const { t } = useI18n()
  const pluginRevision = useSyncExternalStore(
    (listener) => pluginRegistry.subscribe(listener),
    () => pluginRegistry.getRevision(),
    () => pluginRegistry.getRevision(),
  )
  const [view, setView] = useState<EditorView>(defaultView)
  const [blocks, setBlocks] = useState<MarkdownBlock[]>(() => parseMarkdownBlocks(value))
  const blocksRef = useRef(blocks)
  const lastEmitted = useRef(value)
  const onChangeRef = useRef(onChange)
  const [focusRequest, setFocusRequest] = useState<{ index: number; token: number } | null>(null)
  const focusToken = useRef(0)
  const [dragFrom, setDragFrom] = useState<number | null>(null)
  const [dropAt, setDropAt] = useState<number | null>(null)

  useEffect(() => {
    onChangeRef.current = onChange
  }, [onChange])

  // Parent-driven resets (reload, discard, conflict resolution) replace blocks.
  useEffect(() => {
    if (value === lastEmitted.current) return
    lastEmitted.current = value
    const parsed = parseMarkdownBlocks(value)
    blocksRef.current = parsed
    setBlocks(parsed)
  }, [value])

  const apply = useCallback((transform: (previous: MarkdownBlock[]) => MarkdownBlock[]) => {
    const next = transform(blocksRef.current)
    blocksRef.current = next
    lastEmitted.current = serializeMarkdownBlocks(next)
    setBlocks(next)
    onChangeRef.current(lastEmitted.current)
  }, [])

  const requestFocus = useCallback((index: number) => {
    focusToken.current += 1
    setFocusRequest({ index, token: focusToken.current })
  }, [])

  const updateText = useCallback(
    (index: number, text: string) => {
      apply((previous) => previous.map((block, position) => (position === index ? { ...block, text } : block)))
    },
    [apply],
  )

  const changeKind = useCallback(
    (index: number, kind: BlockKind) => {
      apply((previous) =>
        previous.map((block, position) =>
          position === index ? { ...block, text: retypeBlock(block.text, kind) } : block,
        ),
      )
    },
    [apply],
  )

  const addAfter = useCallback(
    (index: number) => {
      apply((previous) => {
        const next = [...previous]
        next.splice(index + 1, 0, createBlock())
        return next
      })
      requestFocus(index + 1)
    },
    [apply, requestFocus],
  )

  const removeBlock = useCallback(
    (index: number) => {
      const length = blocksRef.current.length
      apply((previous) => (previous.length <= 1 ? [createBlock()] : previous.filter((_, position) => position !== index)))
      requestFocus(Math.max(0, Math.min(index, length - 2)))
    },
    [apply, requestFocus],
  )

  const moveBlock = useCallback(
    (index: number, direction: -1 | 1) => {
      const target = index + direction
      if (target < 0 || target >= blocksRef.current.length) return
      apply((previous) => {
        const next = [...previous]
        const moved = next[index]
        next[index] = next[target]
        next[target] = moved
        return next
      })
      requestFocus(target)
    },
    [apply, requestFocus],
  )

  const dragStart = useCallback((index: number) => {
    setDragFrom(index)
    setDropAt(index)
  }, [])

  const dragEnd = useCallback(() => {
    setDragFrom(null)
    setDropAt(null)
  }, [])

  const drop = useCallback(
    (target: number, event: DragEvent) => {
      event.preventDefault()
      const transfer = event.dataTransfer.getData('text/plain')
      if (transfer === '' && dragFrom === null) return
      const raw = transfer === '' ? Number.NaN : Number(transfer)
      const source = Number.isInteger(raw) ? raw : dragFrom
      setDragFrom(null)
      setDropAt(null)
      if (source === null || source === target || source < 0 || source >= blocksRef.current.length) return
      apply((previous) => {
        const next = [...previous]
        const [moved] = next.splice(source, 1)
        next.splice(target, 0, moved)
        return next
      })
      requestFocus(target)
    },
    [apply, dragFrom, requestFocus],
  )

  const switchView = (next: EditorView) => {
    if (next === view) return
    if (view === 'blocks') {
      const markdown = serializeMarkdownBlocks(blocksRef.current)
      if (markdown !== lastEmitted.current) {
        lastEmitted.current = markdown
        onChangeRef.current(markdown)
      }
    } else {
      const parsed = parseMarkdownBlocks(value)
      blocksRef.current = parsed
      setBlocks(parsed)
    }
    setView(next)
  }

  const markdownPreview = useMemo(
    () => (view === 'markdown' ? renderMarkdown(value) : ''),
    [view, value],
  )
  const modeItems: { id: EditorView; label: string }[] = [
    { id: 'markdown', label: t('wiki.editor.mode.markdown') },
    { id: 'blocks', label: t('wiki.editor.mode.blocks') },
  ]

  return (
    <div className={cx('min-w-0', className)}>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div
          role="group"
          aria-label={t('wiki.editor.mode.label')}
          className="inline-flex items-center rounded-lg border border-line bg-panel p-0.5"
        >
          {modeItems.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-label={item.label}
              aria-pressed={view === item.id}
              onClick={() => switchView(item.id)}
              className={cx(
                'rounded-md px-2.5 py-1 text-[11.5px] font-medium transition-colors',
                view === item.id ? 'border border-line bg-surface text-fg' : 'border border-transparent text-fg-3 hover:text-fg',
              )}
            >
              {item.label}
            </button>
          ))}
        </div>
        {view === 'blocks' && (
          <span className="text-[11px] text-fg-3">{t('wiki.editor.blocks.count', { count: blocks.length })}</span>
        )}
      </div>

      {view === 'markdown' ? (
        <div className="grid min-w-0 gap-3 lg:grid-cols-2">
          <Card title={t('wiki.editor.markdown')} bodyClassName="p-2" className="min-w-0">
            <textarea
              value={value}
              onChange={(event) => {
                lastEmitted.current = event.target.value
                onChangeRef.current(event.target.value)
              }}
              aria-label={t('wiki.editor.markdown')}
              spellCheck={false}
              className="thin-scroll min-h-[380px] w-full resize-y border-0 bg-transparent p-2 font-mono text-[12.5px] leading-6"
            />
          </Card>
          <Card title={t('wiki.editor.preview')} bodyClassName="p-3" className="min-w-0">
            {value.trim() === '' ? (
              <p className="text-[11.5px] text-fg-3">{t('wiki.editor.previewEmpty')}</p>
            ) : (
              <div
                className="prose-lkb thin-scroll max-h-[420px] min-w-0 overflow-y-auto"
                dangerouslySetInnerHTML={{ __html: markdownPreview }}
              />
            )}
          </Card>
        </div>
      ) : (
        <div className="min-w-0">
          <p className="mb-2 text-[11px] text-fg-3">{t('wiki.editor.blocks.hint')}</p>
          <div className="space-y-2">
            {blocks.map((block, index) => (
              <BlockRow
                key={block.id}
                index={index}
                total={blocks.length}
                block={block}
                focusToken={focusRequest?.index === index ? focusRequest.token : undefined}
                pluginRevision={pluginRevision}
                dragActive={dragFrom !== null}
                dragging={dragFrom === index}
                dropTarget={dragFrom !== null && dropAt === index && dragFrom !== index}
                onChangeText={updateText}
                onChangeKind={changeKind}
                onAddAfter={addAfter}
                onMove={moveBlock}
                onDelete={removeBlock}
                onDragStart={dragStart}
                onDragOver={setDropAt}
                onDragEnd={dragEnd}
                onDrop={drop}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
