import { useCallback, useEffect, useRef, type PointerEvent as ReactPointerEvent, type KeyboardEvent as ReactKeyboardEvent, type RefObject } from 'react'
import { MAIN_MIN, RESIZE_STEP, clamp } from '../lib/layout'

/** Class toggled on <html> while dragging: blocks selection and iframe hit-testing. */
const RESIZING_CLASS = 'lkb-resizing'

export interface ResizeHandleProps {
  side: 'left' | 'right'
  value: number
  min: number
  max: number
  /** Width of the opposite panel; keeps the main column ≥ MAIN_MIN while dragging. */
  oppositeWidth: number
  containerRef: RefObject<HTMLElement | null>
  onChange: (next: number) => void
  onReset: () => void
  /** Accessible name, e.g. "Resize sidebar". */
  label: string
}

/**
 * Vertical separator between a side panel and the main column.
 *
 * Pointer drags use pointer capture so the pointer may leave the 6px handle;
 * ArrowLeft/ArrowRight nudge by RESIZE_STEP (Home/End jump to the limits) and a
 * double click restores the default width.
 */
export function ResizeHandle({
  side,
  value,
  min,
  max,
  oppositeWidth,
  containerRef,
  onChange,
  onReset,
  label,
}: ResizeHandleProps) {
  const drag = useRef<{ pointerId: number; startX: number; startValue: number } | null>(null)

  const effectiveMax = useCallback(() => {
    const width = containerRef.current?.getBoundingClientRect().width ?? 0
    if (width <= 0) return max
    return Math.max(min, Math.min(max, width - oppositeWidth - MAIN_MIN))
  }, [containerRef, max, min, oppositeWidth])

  const stopDragging = useCallback(() => {
    if (drag.current === null) return
    drag.current = null
    document.documentElement.classList.remove(RESIZING_CLASS)
  }, [])

  // Never leave the document stuck in resize mode if the handle unmounts mid-drag.
  useEffect(() => () => document.documentElement.classList.remove(RESIZING_CLASS), [])

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    drag.current = { pointerId: event.pointerId, startX: event.clientX, startValue: value }
    event.currentTarget.setPointerCapture(event.pointerId)
    document.documentElement.classList.add(RESIZING_CLASS)
  }

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const state = drag.current
    if (state === null || state.pointerId !== event.pointerId) return
    const delta = event.clientX - state.startX
    const next = state.startValue + (side === 'left' ? delta : -delta)
    onChange(clamp(next, min, effectiveMax()))
  }

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const grow = side === 'left' ? 'ArrowRight' : 'ArrowLeft'
    const shrink = side === 'left' ? 'ArrowLeft' : 'ArrowRight'
    if (event.key === grow) {
      event.preventDefault()
      onChange(clamp(value + RESIZE_STEP, min, effectiveMax()))
    } else if (event.key === shrink) {
      event.preventDefault()
      onChange(clamp(value - RESIZE_STEP, min, effectiveMax()))
    } else if (event.key === 'Home') {
      event.preventDefault()
      onChange(min)
    } else if (event.key === 'End') {
      event.preventDefault()
      onChange(effectiveMax())
    }
  }

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={Math.round(max)}
      tabIndex={0}
      title={label}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={stopDragging}
      onPointerCancel={stopDragging}
      onLostPointerCapture={stopDragging}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
      className="group relative z-10 -mx-[3px] w-[6px] shrink-0 cursor-col-resize touch-none select-none focus:outline-none"
    >
      <span
        aria-hidden="true"
        className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-transparent transition-colors group-hover:bg-accent-line group-focus-visible:bg-accent"
      />
    </div>
  )
}
