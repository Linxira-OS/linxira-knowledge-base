// Runs the Vite preview server with the API proxy pointed at an explicit
// backend origin. `vite preview` only accepts host/port on the command line,
// so this wrapper supplies LKB_API_TARGET (the variable vite.config.ts reads)
// without depending on shell-specific environment syntax.
//
// usage: node tools/preview-proxy.mjs [backend-origin] [port]
import { spawn } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const backend = process.argv[2] ?? 'http://127.0.0.1:7331'
const port = process.argv[3] ?? '4173'
const viteBin = path.resolve(here, '..', 'node_modules', 'vite', 'bin', 'vite.js')

const child = spawn(process.execPath, [viteBin, 'preview', '--port', port], {
  cwd: path.resolve(here, '..'),
  env: { ...process.env, LKB_API_TARGET: backend },
  stdio: 'inherit',
})
child.on('exit', (code, signal) => process.exit(signal ? 0 : (code ?? 0)))
