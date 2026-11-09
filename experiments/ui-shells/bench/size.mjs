import { readdirSync, readFileSync, statSync } from 'node:fs'
import { gzipSync, brotliCompressSync, constants } from 'node:zlib'
import path from 'node:path'

const targets = process.argv.slice(2)
if (targets.length === 0) {
  console.error('usage: node size.mjs <file-or-dir> [...]')
  process.exit(1)
}

function walk(entry) {
  const st = statSync(entry)
  if (st.isFile()) return [{ file: entry, size: st.size }]
  const out = []
  for (const child of readdirSync(entry, { withFileTypes: true })) {
    const full = path.join(entry, child.name)
    if (child.isDirectory()) out.push(...walk(full))
    else if (child.isFile()) out.push({ file: full, size: statSync(full).size })
  }
  return out
}

const rows = []
for (const target of targets) {
  const files = walk(target)
  let raw = 0
  let gz = 0
  let br = 0
  for (const { file, size } of files) {
    raw += size
    if (/\.(js|css|html|svg|json|webmanifest)$/i.test(file)) {
      const buf = readFileSync(file)
      gz += gzipSync(buf, { level: 9 }).length
      br += brotliCompressSync(buf, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length
    }
  }
  rows.push({
    target: path.relative(process.cwd(), path.resolve(target)) || '.',
    files: files.length,
    rawKB: +(raw / 1024).toFixed(1),
    gzipKB: +(gz / 1024).toFixed(1),
    brotliKB: +(br / 1024).toFixed(1),
  })
  for (const { file, size } of files.sort((a, b) => b.size - a.size)) {
    console.log(`    ${path.relative(process.cwd(), file)}  ${(size / 1024).toFixed(1)} KB`)
  }
}

console.log('')
for (const row of rows) {
  console.log(`${row.target}: ${row.files} files, raw ${row.rawKB} KB, gzip ${row.gzipKB} KB, brotli ${row.brotliKB} KB`)
}
console.log(JSON.stringify(rows))
