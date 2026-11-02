// Help → Copy Diagnostics: shows the whole report first, and copies only
// when you press Copy. jamalterm never sends it anywhere.
import { Diagnostics, ShowProblemLog, LogProblem } from './backend'
import { el, copyText, toast } from './util'

export async function showDiagnostics(onClose: () => void) {
  const box = el('div', { class: 'modal', id: 'diagnostics' })
  const text = el('textarea', { class: 'diag-text', readonly: '', spellcheck: 'false', 'aria-label': 'Diagnostics report' })
  text.value = await Diagnostics()
  const copy = el('button', { class: 'primary', text: 'Copy' })
  const show = el('button', { class: 'ghost', text: 'Show Problem Log' })
  const cancel = el('button', { class: 'ghost', text: 'Close' })
  box.append(el('div', { class: 'dialog' },
    el('div', { class: 'dialog-head' },
      el('h2', { text: 'Diagnostics' }),
      el('p', { class: 'help', text: 'This is everything that will be copied. Secrets, your home folder, email addresses and this computer\'s name are already removed. No commands or terminal output are included. Nothing is sent anywhere: paste it wherever you choose.' })),
    el('div', { class: 'dialog-body' }, text),
    el('div', { class: 'dialog-foot' }, show, el('span', { class: 'spacer' }), cancel, copy)))
  document.body.append(box)

  const close = () => {
    box.remove()
    onClose()
  }
  copy.onclick = async () => {
    await copyText(text.value)
    toast('Diagnostics copied')
    close()
  }
  show.onclick = () => ShowProblemLog()
  cancel.onclick = close
  box.onmousedown = (e) => e.target === box && close()
  box.onkeydown = (e) => {
    if (e.key === 'Escape') { e.preventDefault(); close() }
  }
  copy.focus()
}

// Script errors in the window go to the private problem log (message and
// place only).
export function logWindowErrors() {
  let sent = 0
  const report = (area: string, msg: string) => {
    if (sent++ > 50) return // a loop of errors shouldn't fill the log
    try { LogProblem(area, msg) } catch { /* nothing more to do */ }
  }
  window.addEventListener('error', (e) => {
    const where = e.filename ? ` (${e.filename.split('/').pop()}:${e.lineno})` : ''
    report('error', String(e.message || e.error) + where)
  })
  window.addEventListener('unhandledrejection', (e) => {
    const r = e.reason
    report('promise', r instanceof Error ? r.message : String(r))
  })
}
