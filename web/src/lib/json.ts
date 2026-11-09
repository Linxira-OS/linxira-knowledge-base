/**
 * The single canonical JSON shape guard for this package.
 *
 * Parsed JSON (network bodies, SSE frames, `*_json` columns, DOM payloads)
 * arrives as `unknown`; this guard proves only "non-null, non-array object".
 * Field-level guarantees stay with the caller (`typeof`, `in`), so no shape is
 * ever assumed twice.
 */
export function asJsonObject(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}
