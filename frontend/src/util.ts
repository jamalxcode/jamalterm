export function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

const utf8 = new TextDecoder()
export function b64ToText(b64: string): string {
  try {
    return utf8.decode(b64ToBytes(b64))
  } catch {
    return ''
  }
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!)
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K, attrs: Record<string, string> = {}, ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag)
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') e.className = v
    else if (k === 'text') e.textContent = v
    else e.setAttribute(k, v)
  }
  for (const c of children) e.append(c)
  return e
}

let toastTimer = 0
export function toast(message: string, kind: 'info' | 'error' = 'info') {
  const t = document.getElementById('toast')!
  t.textContent = message
  t.className = 'show ' + kind
  clearTimeout(toastTimer)
  // Long enough to read: about 2.6 s, plus time for longer messages.
  toastTimer = window.setTimeout(() => (t.className = ''), Math.min(6000, 2600 + message.length * 25))
}

/** Hides the tip at once (a panel or the palette opened over it). */
export function hideToast() {
  clearTimeout(toastTimer)
  const t = document.getElementById('toast')
  if (t) t.className = ''
}

export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = el('textarea')
    ta.value = text
    document.body.append(ta)
    ta.select()
    document.execCommand('copy')
    ta.remove()
  }
}

export function relTime(ms: number): string {
  const s = Math.round((Date.now() - ms) / 1000)
  if (s < 5) return 'just now'
  if (s < 60) return `${s}s ago`
  if (s < 3600) return `${Math.round(s / 60)}m ago`
  return `${Math.round(s / 3600)}h ago`
}

export function duration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
  return `${Math.floor(ms / 60000)}m ${Math.round((ms % 60000) / 1000)}s`
}

/** Typed into a new tab: installs Anthropic's `ant` CLI if needed, then signs in in the browser. */
export const SIGN_IN_COMMAND =
  `sh -c 'command -v ant >/dev/null 2>&1 || { command -v brew >/dev/null 2>&1 && echo "Installing the Anthropic ant CLI with Homebrew..." && brew install anthropics/tap/ant; }; ` +
  `command -v ant >/dev/null 2>&1 || { echo "Install the ant CLI first: brew install anthropics/tap/ant  (or see github.com/anthropics/anthropic-cli)"; exit 1; }; ` +
  `ant auth login && echo && echo "Signed in. AI features in jamalterm now use your Anthropic account."'`

export const SIGN_OUT_COMMAND = `ant auth logout`

export const isMac = navigator.platform.toLowerCase().includes('mac') || navigator.userAgent.includes('Mac OS')

/** Minimal, safe Markdown for AI answers: code blocks, inline code, bold, lists, paragraphs. */
export function renderMarkdown(md: string, onCode: (code: string, pre: HTMLElement, lang: string) => void): DocumentFragment {
  const frag = document.createDocumentFragment()
  const parts = md.split(/```([^\n`]*)\n?([\s\S]*?)(?:```|$)/g)
  for (let i = 0; i < parts.length; i++) {
    if (i % 3 === 0) {
      const text = parts[i].trim()
      if (!text) continue
      for (const para of text.split(/\n{2,}/)) {
        const lines = para.split('\n')
        if (lines.every((l) => /^\s*([-*]|\d+\.)\s+/.test(l))) {
          const list = el(/^\s*\d/.test(lines[0]) ? 'ol' : 'ul')
          for (const l of lines) {
            const li = el('li')
            li.innerHTML = inline(l.replace(/^\s*([-*]|\d+\.)\s+/, ''))
            list.append(li)
          }
          frag.append(list)
        } else {
          const p = el('p')
          p.innerHTML = lines.map(inline).join('<br>')
          frag.append(p)
        }
      }
    } else if (i % 3 === 2) {
      const code = parts[i].replace(/\n$/, '')
      const pre = el('pre')
      pre.append(el('code', { text: code }))
      frag.append(pre)
      onCode(code, pre, parts[i - 1].trim().toLowerCase())
    }
  }
  return frag
}

function inline(s: string): string {
  return escapeHtml(s)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/^#{1,4}\s+(.*)$/, '<strong>$1</strong>')
}

/** A small modal question. Resolves true for the main button. */
export function confirmDialog(o: { title: string; body: string; items?: string[]; ok: string; cancel?: string }): Promise<boolean> {
  return new Promise((resolve) => {
    const box = el('div', { class: 'modal', role: 'alertdialog', 'aria-modal': 'true' })
    const okBtn = el('button', { class: 'primary danger', text: o.ok })
    const cancelBtn = el('button', { class: 'ghost', text: o.cancel ?? 'Cancel' })
    const list = o.items?.length ? [el('ul', { class: 'confirm-list' }, ...o.items.map((t) => el('li', { text: t })))] : []
    box.append(el('div', { class: 'dialog confirm' },
      el('div', { class: 'dialog-head' }, el('h2', { text: o.title })),
      el('div', { class: 'dialog-body' }, el('p', { text: o.body }), ...list),
      el('div', { class: 'dialog-foot' }, cancelBtn, okBtn)))
    document.body.append(box)
    const done = (v: boolean) => {
      box.remove()
      resolve(v)
    }
    okBtn.onclick = () => done(true)
    cancelBtn.onclick = () => done(false)
    box.onmousedown = (e) => e.target === box && done(false)
    box.onkeydown = (e) => {
      if (e.key === 'Escape') { e.preventDefault(); done(false) }
    }
    cancelBtn.focus() // the safe choice has the focus
  })
}

/** localStorage that never throws (it can be unavailable). */
export function stored(k: string): string | null {
  try {
    return localStorage.getItem(k)
  } catch {
    return null
  }
}
export function store(k: string, v: string) {
  try {
    localStorage.setItem(k, v)
  } catch {
    /* unavailable */
  }
}

/** A path the shell reads as one word: as is when safe, else in single
 * quotes (Terminal does the same when you drop a file). */
export function quotePath(p: string): string {
  return /^[\w@%+=:,./~-]+$/.test(p) ? p : `'${p.replace(/'/g, `'\\''`)}'`
}
