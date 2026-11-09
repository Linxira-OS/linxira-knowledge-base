/**
 * Brand mark from `brand/logo*.svg` (same geometry), rendered with
 * `currentColor` so it is transparent on any surface: no white/dark plate.
 * The retrieval dot uses the theme accent token (#1a7f37 light / #32d74b dark).
 */
export function BrandMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} fill="currentColor" aria-hidden="true">
      <rect x="4" y="4" width="8" height="7.25" rx="2" />
      <path d="M6 12.75H10A2 2 0 0 1 12 14.75V18.8A1.2 1.2 0 0 0 13.2 20H26A2 2 0 0 1 28 22V26A2 2 0 0 1 26 28H6A2 2 0 0 1 4 26V14.75A2 2 0 0 1 6 12.75Z" />
      <circle cx="20" cy="12" r="3.5" fill="var(--color-accent)" />
    </svg>
  )
}
