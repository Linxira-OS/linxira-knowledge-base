import { cp, rm, stat } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const source = path.resolve(here, '../../web/dist')
const target = path.resolve(here, '../renderer')

try {
  await stat(path.join(source, 'index.html'))
} catch {
  console.error(`missing ${source}/index.html — run "npm run build" in ../web first`)
  process.exit(1)
}

await rm(target, { recursive: true, force: true })
await cp(source, target, { recursive: true })
console.log(`copied ${source} -> ${target}`)
