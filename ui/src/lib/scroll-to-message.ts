/**
 * Listens for `uclaw:scroll-to-message` events dispatched by the
 * SearchPalette and scrolls + flashes the matching DOM element.
 *
 * Element discovery: rows must have `data-message-id={id}` on a
 * stable wrapper. Returns an unsubscribe.
 */
export function installScrollToMessage(): () => void {
  const handler = (e: Event) => {
    const detail = (e as CustomEvent).detail as
      | { sessionId: string; messageId: string }
      | undefined
    if (!detail?.messageId) return
    // Defer one frame so any tab-switch or list-mount has time to settle.
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLElement>(
        `[data-message-id="${CSS.escape(detail.messageId)}"]`,
      )
      if (!el) return
      el.scrollIntoView({ behavior: 'smooth', block: 'center' })
      el.classList.remove('flash-hit') // restart animation
      void el.offsetWidth // force reflow
      el.classList.add('flash-hit')
      window.setTimeout(() => el.classList.remove('flash-hit'), 1500)
    })
  }
  window.addEventListener('uclaw:scroll-to-message', handler)
  return () => window.removeEventListener('uclaw:scroll-to-message', handler)
}
