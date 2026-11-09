// Smoke-checks the routing topology a deployment must provide: the SPA owns
// every non-API path, and the backend is reachable under `/api/` only.
//
// usage: node tools/check-deployment.mjs [base-url] [space] [slug]
const base = process.argv[2] ?? 'http://127.0.0.1:4173'
const space = process.argv[3] ?? 'product'
const slug = process.argv[4] ?? '01-overview'

const checks = [
  ['SPA /w/ reader', `/w/${space}/${slug}?lang=zh`, /<div id="root">/],
  ['SPA /login', '/login', /<div id="root">/],
  ['SPA /admin', '/admin/models', /<div id="root">/],
  ['api page read', `/api/v1/pages/${space}/${slug}?lang=zh`, /"current_revision_id"/],
  ['api page list', `/api/v1/pages?space=${space}&limit=5`, /"pages"/],
  ['api page history', `/api/v1/pages/${space}/${slug}/history?lang=zh&include_markdown=true`, /"revisions"/],
  ['api page summary', `/api/v1/pages/${space}/${slug}/summary?lang=zh`, /"source_revision"/],
  ['api csrf bootstrap', '/api/v1/auth/csrf', /"csrf_token"/],
  ['api spaces', '/api/v1/spaces', /"spaces"/],
  ['api features', '/api/v1/features', /"features"/],
]

let failures = 0
for (const [label, path, pattern] of checks) {
  try {
    const response = await fetch(base + path)
    const text = await response.text()
    const ok = response.ok && pattern.test(text)
    if (!ok) failures += 1
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${label.padEnd(24)} ${response.status} ${path}`)
    if (!ok) console.log(`     body: ${text.slice(0, 120).replace(/\s+/g, ' ')}`)
  } catch (error) {
    failures += 1
    console.log(`FAIL ${label.padEnd(24)} request error: ${error.message}`)
  }
}
console.log(failures === 0 ? '\nall routing checks passed' : `\n${failures} routing check(s) failed`)
process.exitCode = failures === 0 ? 0 : 1
