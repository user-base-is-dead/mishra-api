import { useEffect, useRef, useState } from 'react'

/** box select starts only after the pixel threshold is passed"drag box select"mode,avoid mistakenputa normal click mistaken as box select. */
const DRAG_THRESHOLD = 5

export interface RectSelectionState {
  /** whether in the box select process(mouse drag distance already exceeded the threshold) */
  active: boolean
  /** box select rectangle(fixed position coordinate) */
  rect: { left: number; top: number; width: number; height: number } | null
}

export interface UseRectSelectOptions {
  containerRef: React.RefObject<HTMLElement | null>
  /** hit elementof CSS selector;require the element to carryhas data-{idAttribute},store number id */
  itemSelector: string
  /** data attribute name(without `data-` prefix),fromread from the hit element id */
  idAttribute: string
  /** when box select completesofcallback,the parameter is the hit elementof id set + whether it is held down Ctrl/Meta */
  onSelectionChange: (ids: Set<number>, additive: boolean) => void
  enabled?: boolean
}

/**
 * inwithin the given containerEnableleft mouse drag box select.
 *
 * design point:
 * - only inthe press point is not a button / Inputbox / dropdownetc.start only on interactive elements,avoid accidental touch.
 * - hold down Ctrl/Meta append to the existing set on box selecthasselection,otherwiseReplace.
 * - downgrade when drag distance is below the thresholdasnormal click,byoriginalhas onClick take over.
 * - use fixed positioned dashed rectangleShowselection,avoid polluting the parent layout.
 */
export function useRectSelect(options: UseRectSelectOptions): RectSelectionState {
  const { containerRef, itemSelector, idAttribute, onSelectionChange, enabled = true } = options
  const [state, setState] = useState<RectSelectionState>({ active: false, rect: null })

  const startRef = useRef<{ x: number; y: number; additive: boolean } | null>(null)
  const activeRef = useRef(false)

  useEffect(() => {
    if (!enabled) return
    const el = containerRef.current
    if (!el) return

    const isInteractive = (target: EventTarget | null): boolean => {
      if (!(target instanceof HTMLElement)) return false
      return Boolean(
        target.closest(
          'button, a, input, select, textarea, label, [role="checkbox"], [role="menuitem"], [data-no-rect-select]'
        )
      )
    }

    const dataAttr = `data-${idAttribute}`

    const onMouseDown = (e: MouseEvent) => {
      if (e.button !== 0) return
      if (isInteractive(e.target)) return
      startRef.current = { x: e.clientX, y: e.clientY, additive: e.ctrlKey || e.metaKey }
      activeRef.current = false
    }

    const onMouseMove = (e: MouseEvent) => {
      const start = startRef.current
      if (!start) return
      const dx = e.clientX - start.x
      const dy = e.clientY - start.y

      if (!activeRef.current) {
        if (Math.abs(dx) < DRAG_THRESHOLD && Math.abs(dy) < DRAG_THRESHOLD) return
        activeRef.current = true
        document.body.style.userSelect = 'none'
      }

      const left = Math.min(e.clientX, start.x)
      const top = Math.min(e.clientY, start.y)
      const width = Math.abs(dx)
      const height = Math.abs(dy)
      setState({ active: true, rect: { left, top, width, height } })
    }

    const onMouseUp = (e: MouseEvent) => {
      const start = startRef.current
      startRef.current = null
      if (!start) return
      if (!activeRef.current) return // a normal click does not change the selection
      activeRef.current = false
      document.body.style.userSelect = ''

      const left = Math.min(e.clientX, start.x)
      const top = Math.min(e.clientY, start.y)
      const right = Math.max(e.clientX, start.x)
      const bottom = Math.max(e.clientY, start.y)

      const items = el.querySelectorAll<HTMLElement>(itemSelector)
      const hits = new Set<number>()
      items.forEach((item) => {
        const r = item.getBoundingClientRect()
        const overlap = r.left < right && r.right > left && r.top < bottom && r.bottom > top
        if (!overlap) return
        const raw = item.getAttribute(dataAttr)
        const id = raw ? Number(raw) : NaN
        if (Number.isFinite(id)) hits.add(id)
      })

      onSelectionChange(hits, start.additive)
      setState({ active: false, rect: null })
    }

    el.addEventListener('mousedown', onMouseDown)
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)

    return () => {
      el.removeEventListener('mousedown', onMouseDown)
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      document.body.style.userSelect = ''
    }
  }, [containerRef, itemSelector, idAttribute, onSelectionChange, enabled])

  return state
}
