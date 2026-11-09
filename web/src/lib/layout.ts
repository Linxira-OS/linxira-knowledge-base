/**
 * Three-column shell sizing.
 *
 * Widths live in localStorage; drag/keyboard changes are clamped so the main
 * column keeps at least `MAIN_MIN` pixels. The right column auto-collapses
 * below 1100px and the left column below 1024px (the previous CSS breakpoints),
 * but only the user's explicit collapse state is persisted.
 */

export const LEFT_DEFAULT = 240
export const LEFT_MIN = 200
export const LEFT_MAX = 420
export const RIGHT_DEFAULT = 336
export const RIGHT_MIN = 300
export const RIGHT_MAX = 560
/** Main content must never be squeezed below this. */
export const MAIN_MIN = 420
/** Arrow-key resize step. */
export const RESIZE_STEP = 16

export const LAYOUT_STORAGE_KEY = 'lkb-layout'
export const HIDE_LEFT_QUERY = '(max-width: 1023px)'
export const COLLAPSE_RIGHT_QUERY = '(max-width: 1099px)'

export interface LayoutState {
  left: number
  right: number
  leftCollapsed: boolean
  rightCollapsed: boolean
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

const DEFAULTS: LayoutState = {
  left: LEFT_DEFAULT,
  right: RIGHT_DEFAULT,
  leftCollapsed: false,
  rightCollapsed: false,
}

export function loadLayout(): LayoutState {
  try {
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY)
    if (!raw) return DEFAULTS
    const parsed = JSON.parse(raw) as Partial<LayoutState> | null
    if (!parsed || typeof parsed !== 'object') return DEFAULTS
    return {
      left: typeof parsed.left === 'number' ? clamp(parsed.left, LEFT_MIN, LEFT_MAX) : DEFAULTS.left,
      right: typeof parsed.right === 'number' ? clamp(parsed.right, RIGHT_MIN, RIGHT_MAX) : DEFAULTS.right,
      leftCollapsed: parsed.leftCollapsed === true,
      rightCollapsed: parsed.rightCollapsed === true,
    }
  } catch {
    return DEFAULTS
  }
}

export function saveLayout(state: LayoutState): void {
  try {
    localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(state))
  } catch {
    /* private mode: the in-memory layout still applies */
  }
}
