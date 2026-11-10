// Cards shown once when a window starts: the welcome card on a first start,
// What's new after an update, and a notice after a crash. Each is a small
// card in a corner, never a modal: the terminal is usable straight away.
import { LastCrash, DismissCrash, OpenURL, Version, WhatsNew } from './backend'
import { commands, keyLabel } from './keys'
import { el, stored, store } from './util'

const keysOf = (id: string) => keyLabel(commands.find((c) => c.id === id)?.keys ?? '')

/** First start only (main.ts skips it when tabs were reopened): the three
 * things that aren't obvious. Returns true if it was shown. */
export async function showWelcome(focus: () => void): Promise<boolean> {
  if (stored('jt-welcomed') || stored('jt-last-version')) return false
  store('jt-welcomed', '1')
  store('jt-last-version', await Version()) // no What's new on top of this
  const tip = (keys: string, what: string, how: string) =>
    el('li', {}, el('kbd', { text: keys }), el('span', {}, el('strong', { text: what }), ' ', how))
  const ok = el('button', { class: 'primary small' }, 'Got It')
  const card = el('div', { id: 'welcome', role: 'dialog', 'aria-label': 'Welcome to jamalterm' },
    el('strong', { text: 'Welcome to jamalterm' }),
    el('ul', {},
      tip(keysOf('browse') || '⌘↑', 'Browse past commands.', 'Move with ↑ ↓, press Return to run one again, Esc to stop.'),
      tip(keysOf('aiPanel') || '⌘I', 'Ask AI.', 'About an error, or for a command. Set up an assistant in Settings → AI.'),
      tip(keysOf('palette') || '⇧⌘P', 'Every command and setting.', 'Type a few letters to find it.')),
    el('div', { class: 'keyrow' }, ok))
  document.body.append(card)
  ok.onclick = () => {
    card.remove()
    focus()
  }
  return true
}

/** jamalterm crashed last time: say so, and offer the (private) report. */
export async function showLastCrash(diagnostics: () => void) {
  if (!(await LastCrash())) return
  const copy = el('button', { class: 'primary small' }, 'Copy Diagnostics…')
  const dismiss = el('button', { class: 'ghost small' }, 'Dismiss')
  const card = el('div', { id: 'crash-card', role: 'alert' },
    el('strong', { text: 'jamalterm quit unexpectedly last time.' }),
    el('span', { class: 'help', text: 'A short report is saved on this Mac (nothing was sent). You can copy it to share with whoever helps you.' }),
    el('div', { class: 'keyrow' }, copy, dismiss))
  document.body.append(card)
  const close = () => { card.remove(); void DismissCrash() }
  copy.onclick = () => { close(); diagnostics() }
  dismiss.onclick = close
}

/** Once after an update: what changed since the version you had. */
export async function showWhatsNew() {
  const version = await Version()
  if (/dev|test/.test(version)) return
  const last = stored('jt-last-version')
  store('jt-last-version', version)
  if (!last || last === version) return
  const notes = await WhatsNew(last)
  if (!notes) return
  const items = notes.split('\n').filter((l) => /^\s*[-*] /.test(l)).slice(0, 6)
    .map((l) => el('li', { text: l.replace(/^\s*[-*]\s+/, '').replace(/\*\*/g, '') }))
  const all = el('button', { class: 'ghost small' }, 'Full Release Notes')
  const ok = el('button', { class: 'primary small' }, 'OK')
  const card = el('div', { id: 'whats-new', role: 'dialog', 'aria-label': 'What’s new' },
    el('strong', { text: `What’s new in jamalterm ${version}` }), el('ul', {}, ...items), el('div', { class: 'keyrow' }, all, ok))
  document.body.append(card)
  all.onclick = () => OpenURL(`https://github.com/jamalxcode/jamalterm/releases/tag/v${version}`)
  ok.onclick = () => card.remove()
}
