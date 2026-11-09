import DOMPurify from 'dompurify'
import { marked } from 'marked'

/**
 * Renders Markdown for the editor preview.
 *
 * The reader renders the backend's own HTML (pulldown-cmark + ammonia); the
 * preview must look the same, so both go through the `.prose-lkb` styles and
 * the preview output is sanitised here before it reaches the DOM.
 */
export function renderMarkdown(markdown: string): string {
  const raw = marked.parse(markdown, { async: false, gfm: true, breaks: false })
  return DOMPurify.sanitize(raw, { USE_PROFILES: { html: true } })
}

export interface TocEntry {
  id: string
  level: number
  text: string
}

export interface AnnotatedHtml {
  html: string
  toc: TocEntry[]
}

/**
 * Adds stable `id`s to `h1`–`h4` in already-rendered article HTML and returns
 * the heading list for the table of contents. Works on backend HTML (which has
 * no ids of its own) and on locally rendered Markdown alike.
 */
export function annotateHeadings(html: string): AnnotatedHtml {
  const container = document.createElement('div')
  container.innerHTML = html
  const headings = container.querySelectorAll('h1, h2, h3, h4')
  const toc: TocEntry[] = []
  let index = 0
  for (const heading of headings) {
    index += 1
    const level = Number(heading.tagName.slice(1))
    const text = heading.textContent?.trim() ?? ''
    if (!text) continue
    const id = `lkb-h-${index}`
    heading.id = id
    toc.push({ id, level, text })
  }
  return { html: container.innerHTML, toc }
}

/** Strips raw HTML from a heading label for the outline list. */
export function plainText(html: string): string {
  const container = document.createElement('div')
  container.innerHTML = html
  return container.textContent ?? ''
}

/**
 * Finds `passage` inside `root` and marks the first match, returning whether it
 * was found. Matches are performed on whitespace-normalised text, so a quote
 * taken from search results (trimmed lines) still locates its paragraph.
 */
export function highlightPassage(root: HTMLElement, passage: string): boolean {
  const needle = normalise(passage).slice(0, 120)
  if (needle.length < 8) return false
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  const nodes: Text[] = []
  let textNode = walker.nextNode()
  while (textNode) {
    if (textNode.nodeValue && textNode.nodeValue.trim()) nodes.push(textNode as Text)
    textNode = walker.nextNode()
  }
  let combined = ''
  const ranges: { node: Text; start: number; end: number }[] = []
  for (const node of nodes) {
    const value = normalise(node.nodeValue ?? '')
    const start = combined.length
    combined += value
    ranges.push({ node, start, end: combined.length })
  }
  const hit = combined.indexOf(needle)
  if (hit === -1) return false
  const end = hit + needle.length
  const touched = ranges.filter((range) => range.end > hit && range.start < end)
  if (touched.length === 0) return false
  for (const range of touched) {
    const mark = document.createElement('mark')
    mark.className = 'lkb-hit'
    const node = range.node
    const from = Math.max(0, hit - range.start)
    const to = Math.min(node.nodeValue?.length ?? 0, end - range.start)
    if (to <= from) continue
    const target = node.splitText(from)
    target.splitText(to - from)
    target.parentNode?.insertBefore(mark, target)
    mark.appendChild(target)
  }
  return true
}

function normalise(value: string): string {
  return value.replace(/\s+/g, ' ').trim()
}
