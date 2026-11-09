// Exercises the browser sign-in path end to end against a session-mode
// deployment via the JSON auth API:
//   CSRF bootstrap → 401 → bad credentials → login → session cookie →
//   CSRF-guarded write → logout.
//
// usage: node tools/check-session-flow.mjs [base-url] [username] [password]
const base = process.argv[2] ?? 'http://127.0.0.1:4173'
const username = process.argv[3] ?? 'alice'
const password = process.argv[4] ?? 'lkb-web-demo'

const jar = new Map()
function rememberCookies(response) {
  const raw = response.headers.getSetCookie ? response.headers.getSetCookie() : []
  for (const cookie of raw) {
    const [pair] = cookie.split(';')
    const index = pair.indexOf('=')
    if (index > 0) jar.set(pair.slice(0, index).trim(), pair.slice(index + 1).trim())
  }
}
function cookieHeader() {
  return [...jar.entries()].map(([k, v]) => `${k}=${v}`).join('; ')
}

const request = async (path, init = {}) => {
  const response = await fetch(base + path, {
    ...init,
    redirect: 'manual',
    headers: { ...(init.headers ?? {}), cookie: cookieHeader() },
  })
  rememberCookies(response)
  return response
}

let failures = 0
const check = (label, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${label}${detail ? ` — ${detail}` : ''}`)
}

// 1. Unauthenticated access is rejected.
const anonymous = await request('/api/v1/features')
check('anonymous /api/v1/features → 401', anonymous.status === 401, `got ${anonymous.status}`)

// 2. CSRF bootstrap issues the cookie + token.
const csrfResponse = await request('/api/v1/auth/csrf')
const csrfBody = await csrfResponse.json().catch(() => ({}))
const csrf = typeof csrfBody.csrf_token === 'string' ? csrfBody.csrf_token : jar.get('lkb_csrf')
check('GET /api/v1/auth/csrf issues token', csrfResponse.status === 200 && Boolean(csrf))

const loginJson = (secret) => ({
  method: 'POST',
  headers: { 'content-type': 'application/json', 'x-csrf-token': csrf ?? '' },
  body: JSON.stringify({ username, password: secret }),
})

// 3. Wrong credentials are rejected.
const bad = await request('/api/v1/auth/login', loginJson('wrong-password'))
const badBody = await bad.text()
check('wrong password → 401', bad.status === 401, `got ${bad.status}`)
check('error envelope carries a stable code', /"error"\s*:\s*"unauthorized"/.test(badBody), badBody.slice(0, 80))

// 4. Correct credentials establish a session.
const login = await request('/api/v1/auth/login', loginJson(password))
const loginBody = await login.json().catch(() => ({}))
check('valid login → 200', login.status === 200, `got ${login.status}`)
check('login returns principal', typeof loginBody.principal?.user_id === 'string')
check('session cookie issued', Boolean(jar.get('lkb_session')))

// 5. The session authenticates reads.
const me = await request('/api/v1/auth/me')
check('session /api/v1/auth/me → 200', me.status === 200, `got ${me.status}`)
const features = await request('/api/v1/features')
check('session /api/v1/features → 200', features.status === 200, `got ${features.status}`)

// 6. Session writes require the CSRF header, and succeed with it.
const withoutHeader = await request('/api/v1/conversations', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ space: 'product', title: 'csrf probe' }),
})
check('session write without X-CSRF-Token → 403', withoutHeader.status === 403, `got ${withoutHeader.status}`)

const withHeader = await request('/api/v1/conversations', {
  method: 'POST',
  headers: { 'content-type': 'application/json', 'x-csrf-token': csrf ?? '' },
  body: JSON.stringify({ space: 'product', title: 'csrf probe' }),
})
const withHeaderBody = await withHeader.text()
check('session write with X-CSRF-Token → 201', withHeader.status === 201, `got ${withHeader.status} ${withHeaderBody.slice(0, 80)}`)

// 7. Logout drops the session.
const logout = await request('/api/v1/auth/logout', {
  method: 'POST',
  headers: { 'x-csrf-token': csrf ?? '' },
})
check('POST /api/v1/auth/logout → 200', logout.status === 200, `got ${logout.status}`)
const afterLogout = await request('/api/v1/features')
check('session invalid after logout → 401', afterLogout.status === 401, `got ${afterLogout.status}`)

console.log(failures === 0 ? '\nsession flow ok' : `\n${failures} session-flow check(s) failed`)
process.exitCode = failures === 0 ? 0 : 1
