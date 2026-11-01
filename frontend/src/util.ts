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
  toastTimer = window.setTimeout(() => (t.className = ''), 2600)
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

export const isMac = navigator.platform.toLowerCase().includes('mac') || navigator.userAgent.includes('Mac OS')

/** Minimal, safe Markdown for AI answers: code blocks, inline code, bold, lists, paragraphs. */
export function renderMarkdown(md: string, onCode: (code: string, pre: HTMLElement) => void): DocumentFragment {
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
      onCode(code, pre)
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
