// One-off: verify every t('...') key literal exists in both zh/en catalogs.
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

const srcRoot = new URL('../src/', import.meta.url).pathname.replace(/^\//, '')
const walk = (dir) => {
  const out = []
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) out.push(...walk(p))
    else if (/\.tsx?$/.test(p)) out.push(p)
  }
  return out
}

const files = walk(srcRoot)
const msgDir = join(srcRoot, 'lib', 'i18n', 'messages')
const catalog = { zh: new Set(), en: new Set() }
for (const f of files.filter((f) => f.startsWith(msgDir))) {
  const text = readFileSync(f, 'utf8')
  const zhMatch = text.match(/'zh-CN':\s*\{([\s\S]*?)\n  \},/)
  const enMatch = text.match(/\n  en:\s*\{([\s\S]*?)\n  \},/)
  for (const [locale, match] of [['zh', zhMatch], ['en', enMatch]]) {
    if (!match) continue
    for (const m of match[1].matchAll(/'([^']+)':/g)) catalog[locale].add(m[1])
  }
}

const used = new Map()
for (const f of files.filter((f) => !f.startsWith(msgDir))) {
  const text = readFileSync(f, 'utf8')
  for (const m of text.matchAll(/\bt\(\s*'([^']+)'/g)) {
    if (!used.has(m[1])) used.set(m[1], [])
    used.get(m[1]).push(f.replace(/\\/g, '/').split('/src/')[1])
  }
}

// Reverse direction: every catalog key must still appear as a literal in src.
// `error.*`/`http.*` are composed at runtime (`error.${code}`) and mirror the
// server catalog, so they are exempt from the unused report.
const literals = new Set()
for (const f of files.filter((f) => !f.startsWith(msgDir))) {
  const text = readFileSync(f, 'utf8')
  for (const m of text.matchAll(/'([^'\n]+)'/g)) literals.add(m[1])
  for (const m of text.matchAll(/"([^"\n]+)"/g)) literals.add(m[1])
}
const unused = [...catalog.zh].filter(
  (key) => !literals.has(key) && !key.startsWith('error.') && !key.startsWith('http.'),
)
if (unused.length > 0) {
  for (const key of unused.slice(0, 40)) console.log(`UNUSED ${key}`)
  if (unused.length > 40) console.log(`… ${unused.length - 40} more unused keys`)
}

let missing = 0
for (const [key, where] of used) {
  if (!catalog.zh.has(key) || !catalog.en.has(key)) {
    missing++
    console.log(`MISSING ${key} (zh:${catalog.zh.has(key)} en:${catalog.en.has(key)}) <- ${where[0]}`)
  }
}
console.log(`checked ${used.size} used keys, catalog zh=${catalog.zh.size} en=${catalog.en.size}, missing=${missing}`)
