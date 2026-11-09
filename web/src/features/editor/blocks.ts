/**
 * Markdown block model for the block editor.
 *
 * A "block" is one top-level Markdown unit. The default separator is a blank
 * line, but fenced code blocks are kept intact even when they contain blank
 * lines, and a fence always starts a new block. Tables stay in one block
 * because their rows are contiguous non-blank lines. Parsing and serialising
 * preserve every non-blank source line; only block separators are normalised
 * (a run of blank lines collapses to `\n\n`, and adjacent blocks gain one
 * blank line when joined).
 */

export type BlockKind =
  | 'paragraph'
  | 'heading1'
  | 'heading2'
  | 'heading3'
  | 'bulleted'
  | 'numbered'
  | 'quote'
  | 'code'
  | 'divider'

export interface MarkdownBlock {
  id: string
  text: string
}

let blockSequence = 0

function nextBlockId(): string {
  blockSequence += 1
  return `lkb-block-${blockSequence}`
}

export function createBlock(text = ''): MarkdownBlock {
  return { id: nextBlockId(), text }
}

const FENCE_OPEN = /^ {0,3}(`{3,}|~{3,})(.*)$/
const FENCE_CLOSE = /^ {0,3}(`{3,}|~{3,})[ \t]*$/
const HEADING = /^ {0,3}(#{1,6})[ \t]+/
const DIVIDER = /^ {0,3}(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$/
const QUOTE = /^ {0,3}>/
const BULLET = /^ {0,3}[-*+][ \t]+/
const NUMBERED = /^ {0,3}\d{1,9}[.)][ \t]+/

interface FenceInfo {
  info: string
  body: string[]
}

/** Detects a fenced code block at the start of `text` (closed or not). */
function fenceInfo(text: string): FenceInfo | null {
  const lines = text.split('\n')
  const opening = lines[0]?.match(FENCE_OPEN)
  if (!opening) return null
  const char = opening[1][0]
  let end = lines.length
  if (lines.length > 1) {
    const closing = lines[lines.length - 1].match(FENCE_CLOSE)
    if (closing && closing[1][0] === char && closing[1].length >= opening[1].length) {
      end = lines.length - 1
    }
  }
  return { info: opening[2].trim(), body: lines.slice(1, end) }
}

/** True when the block is a standalone thematic break (`---`, `***`, `___`). */
export function isDividerBlock(text: string): boolean {
  return DIVIDER.test(text.split('\n', 1)[0] ?? '')
}

/**
 * Splits Markdown into editable blocks.
 *
 * Fence bookkeeping happens before blank-line splitting, so a code sample with
 * empty lines inside stays one block. A fence that follows a paragraph without
 * a blank line also starts a new block, matching CommonMark's ability for
 * fenced code to interrupt a paragraph.
 */
export function parseMarkdownBlocks(markdown: string): MarkdownBlock[] {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n')
  const texts: string[] = []
  let current: string[] = []
  let fence: { char: string; length: number } | null = null

  const flush = () => {
    while (current.length > 0 && current[0].trim() === '') current.shift()
    while (current.length > 0 && current[current.length - 1].trim() === '') current.pop()
    if (current.length > 0) texts.push(current.join('\n'))
    current = []
  }

  for (const line of lines) {
    if (fence !== null) {
      current.push(line)
      const closing = line.match(FENCE_CLOSE)
      if (closing && closing[1][0] === fence.char && closing[1].length >= fence.length) {
        fence = null
        // A closed fence ends its block even when the next paragraph follows
        // without a blank line; block editing must not swallow that paragraph.
        flush()
      }
      continue
    }

    const opening = line.match(FENCE_OPEN)
    if (opening) {
      flush()
      current.push(line)
      fence = { char: opening[1][0], length: opening[1].length }
      continue
    }

    if (line.trim() === '') {
      flush()
      continue
    }

    current.push(line)
  }
  flush()

  if (texts.length === 0) return [createBlock()]
  return texts.map((text) => createBlock(text))
}

/**
 * Joins blocks with exactly one blank line. Whitespace-only blocks are UI
 * placeholders and are not persisted, so an empty block never leaks a stray
 * `\n\n` into the saved Markdown.
 */
export function serializeMarkdownBlocks(blocks: readonly MarkdownBlock[]): string {
  return blocks
    .map((block) => block.text.replace(/^\n+|\n+$/g, ''))
    .filter((text) => text.trim() !== '')
    .join('\n\n')
}

/** Best-effort type detection used by the per-block type selector. */
export function blockKind(text: string): BlockKind {
  const firstLine = text.split('\n', 1)[0] ?? ''
  if (FENCE_OPEN.test(firstLine)) return 'code'
  if (DIVIDER.test(firstLine)) return 'divider'
  const heading = firstLine.match(HEADING)
  if (heading) {
    const level = heading[1].length
    if (level === 1) return 'heading1'
    if (level === 2) return 'heading2'
    if (level === 3) return 'heading3'
    return 'paragraph'
  }
  if (QUOTE.test(firstLine)) return 'quote'
  if (BULLET.test(firstLine)) return 'bulleted'
  if (NUMBERED.test(firstLine)) return 'numbered'
  return 'paragraph'
}

/** Removes the block-level markers without touching inline Markdown. */
function plainLines(text: string): string[] {
  const fence = fenceInfo(text)
  if (fence) return [...fence.body]

  if (isDividerBlock(text)) return []

  let lines = text.replace(/\r\n?/g, '\n').split('\n')
  lines = lines.map((line) => line.replace(/^ {0,3}>[ \t]?/, ''))
  lines[0] = lines[0].replace(HEADING, '')
  lines = lines.map((line) => line.replace(BULLET, '').replace(NUMBERED, ''))
  while (lines.length > 0 && lines[0].trim() === '') lines.shift()
  while (lines.length > 0 && lines[lines.length - 1].trim() === '') lines.pop()
  return lines
}

/**
 * Re-applies a block type while keeping the inline Markdown source. Heading
 * conversion joins continuation lines with spaces because a Markdown heading
 * is a single line.
 */
export function retypeBlock(text: string, kind: BlockKind): string {
  const fence = fenceInfo(text)
  const lines = plainLines(text)

  switch (kind) {
    case 'paragraph':
      return lines.join('\n')
    case 'heading1':
    case 'heading2':
    case 'heading3': {
      const level = kind === 'heading1' ? 1 : kind === 'heading2' ? 2 : 3
      const content = lines
        .map((line) => line.trim())
        .filter(Boolean)
        .join(' ')
      return content === '' ? '#'.repeat(level) : `${'#'.repeat(level)} ${content}`
    }
    case 'bulleted':
      return lines.map((line) => (line.trim() === '' ? '' : `- ${line}`)).join('\n')
    case 'numbered': {
      let number = 1
      return lines
        .map((line) => {
          if (line.trim() === '') return ''
          const item = `${number}. ${line}`
          number += 1
          return item
        })
        .join('\n')
    }
    case 'quote':
      return lines.map((line) => (line.trim() === '' ? '>' : `> ${line}`)).join('\n')
    case 'code': {
      const info = fence?.info ?? ''
      const body = fence ? fence.body.join('\n') : lines.join('\n')
      return `\`\`\`${info}\n${body}\n\`\`\``
    }
    case 'divider':
      return '---'
    default:
      return text
  }
}
