// The window frame follows macOS, as Apple's guidelines ask: light or dark
// appearance, the system font and accent color, and the accessibility
// settings (Reduce Motion, Reduce Transparency, Increase Contrast).
// The terminal itself keeps its theme.

const root = document.documentElement
const listeners: (() => void)[] = []

function watch(query: string, cls: string) {
  const m = matchMedia(query)
  const set = () => {
    root.classList.toggle(cls, m.matches)
    for (const f of listeners) f()
  }
  root.classList.toggle(cls, m.matches)
  m.addEventListener('change', set)
}

let started = false
/** Watches the Mac's appearance settings; onChange runs when one changes. */
export function applySystemLook(onChange?: () => void) {
  if (onChange) listeners.push(onChange)
  if (started) return
  started = true
  root.classList.add('system-look')
  watch('(prefers-color-scheme: dark)', 'sys-dark')
  watch('(prefers-reduced-motion: reduce)', 'reduce-motion')
  watch('(prefers-reduced-transparency: reduce)', 'reduce-transparency')
  watch('(prefers-contrast: more)', 'more-contrast')
}

export { systemDark, systemPalette } from './themes'
