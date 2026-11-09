/** Compile-time exhaustiveness helper for discriminated unions. */
export function assertNever(value: never): never {
  throw new Error(`unexpected variant: ${String(value)}`)
}
