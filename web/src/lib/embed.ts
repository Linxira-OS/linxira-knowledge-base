/**
 * Embed mode (`?embed=1`).
 *
 * The SPA is designed to be framed: it never touches `window.top`, never
 * requires host APIs, and in embed mode it drops the shell chrome and keeps a
 * single content column. The flag is read from the URL so it works for any
 * route (hash-free, standard query string).
 */
export function isEmbedMode(location: Location = window.location): boolean {
  const value = new URLSearchParams(location.search).get('embed')
  return value === '1' || value === 'true'
}

/** True when the document is inside a frame (host integration or embed). */
export function isFramed(): boolean {
  try {
    return window.self !== window.top
  } catch {
    return true
  }
}
