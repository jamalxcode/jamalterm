// The command palette (⇧⌘P): type a few letters of any action, setting,
// theme, assistant or tab and press ⏎.
import { themes } from './themes'
import { providers } from './providers'
import type { SettingsPage } from './settingsView'
import { el, hideToast } from './util'
import { commands, keyLabel } from './keys'
import type { Settings } from './types'

export interface PaletteHost {
  run(cmd: string): void // a menu command (main.ts runMenu)
  settings(): Settings
  save(s: Settings): Promise<void>
  openSettings(page: SettingsPage, find?: string): void
  tabs(): { title: string; detail: string; go(): void }[]
  focus(): void
}

export interface PaletteItem {
  title: string
  group: string
  keys?: string
  detail?: string
  run(): void
}


type Bool = { [K in keyof Settings]: Settings[K] extends boolean ? K : never }[keyof Settings]

/** Settings you can flip from the palette: [field, label, page]. */
export const toggles: [Bool, string, SettingsPage][] = [
  ['cursorBlink', 'Blinking cursor', 'appearance'],
  ['ligatures', 'Font ligatures (=> != …)', 'appearance'],
  ['blur', 'Blur behind see-through window', 'appearance'],
  ['shellIntegration', 'Shell integration', 'shell'],
  ['arrowBrowse', '↑ at an empty prompt browses commands', 'shell'],
  ['restoreSession', 'Reopen tabs after a restart', 'shell'],
  ['confirmRerun', 'Ask before re-running a command', 'shell'],
  ['optionIsMeta', 'Option as Meta key', 'shell'],
  ['notifyLongCommands', 'Notify when long commands finish', 'shell'],
  ['screenReader', 'Screen reader support', 'shell'],
  ['aiEnabled', 'AI features', 'ai'],
  ['aiShareOutput', 'Send command output with AI questions', 'privacy'],
  ['keepProblemLog', 'Private problem log', 'privacy'],
  ['autoUpdate', 'Check for updates automatically', 'updates'],
]

/** Other settings open Settings at the right place: [label, page, find]. */
const places: [string, SettingsPage, string][] = [
  ['Font', 'appearance', 'Font'],
  ['Font size', 'appearance', 'Size'],
  ['Line height', 'appearance', 'Line height'],
  ['Cursor style', 'appearance', 'Cursor'],
  ['Window opacity / transparency', 'appearance', 'Window opacity'],
  ['Host colors (ssh)', 'appearance', 'Host colors'],
  ['Shell program', 'shell', 'Shell'],
  ['Scrollback lines', 'shell', 'Scrollback'],
  ['Prompt editing keys (emacs / vi)', 'shell', 'Prompt editing keys'],
  ['AI assistant and API keys', 'ai', 'Assistant'],
  ['Forget saved tabs and output', 'privacy', 'Saved tabs'],
  ['Update channel (stable / beta)', 'updates', 'Update channel'],
]

/** Scores how well query matches text (letters in order); -1 = no match. */
export function score(query: string, text: string): number {
  const q = query.toLowerCase().replace(/\s+/g, '')
  if (!q) return 0
  const t = text.toLowerCase()
  let s = 0, ti = 0, run = 0
  for (const ch of q) {
    const at = t.indexOf(ch, ti)
    if (at < 0) return -1
    const wordStart = at === 0 || /[\s/:(—-]/.test(t[at - 1])
    run = at === ti ? run + 1 : 0
    s += 1 + (wordStart ? 6 : 0) + run * 3 - Math.min(at - ti, 8) * 0.3
    ti = at + 1
  }
  if (t.startsWith(query.toLowerCase())) s += 10
  return s - t.length * 0.02
}

const RECENT = 'palette.recent'
function recent(): string[] {
  try { return JSON.parse(localStorage.getItem(RECENT) ?? '[]') } catch { return [] }
}
function remember(title: string) {
  try { localStorage.setItem(RECENT, JSON.stringify([title, ...recent().filter((t) => t !== title)].slice(0, 8))) } catch { /* private window */ }
}

export class Palette {
  private root = el('div', { id: 'palette', class: 'palette-wrap', hidden: '' })
  private input = el('input', { type: 'text', placeholder: 'Type a command, setting, theme or tab…', spellcheck: 'false', 'aria-label': 'Command palette', role: 'combobox', 'aria-expanded': 'true', 'aria-controls': 'palette-list' }) as HTMLInputElement
  private list = el('ul', { id: 'palette-list', role: 'listbox' })
  private shown: PaletteItem[] = []
  private sel = 0

  constructor(private host: PaletteHost) {
    this.root.append(el('div', { class: 'palette' }, this.input, this.list))
    document.body.append(this.root)
    this.input.addEventListener('input', () => this.filter())
    this.input.addEventListener('keydown', (e) => this.key(e))
    this.root.addEventListener('mousedown', (e) => {
      if (e.target === this.root) this.close()
    })
  }

  get visible() { return !this.root.hidden }

  open() {
    hideToast()
    if (this.visible) return this.close()
    this.root.hidden = false
    this.input.value = ''
    this.filter()
    this.input.focus()
  }

  close(refocus = true) {
    this.root.hidden = true
    if (refocus) this.host.focus()
  }

  items(): PaletteItem[] {
    const h = this.host
    const cfg = h.settings()
    const out: PaletteItem[] = commands.filter((c) => c.palette).map((c) => ({
      title: c.title.replace(/…$/, ''), group: 'Action', keys: keyLabel(c.keys), run: () => h.run(c.id),
    }))
    for (const p of providers) {
      out.push({ title: `Use ${p.name}`, group: 'AI', detail: cfg.aiProvider === p.id ? 'in use' : p.company, run: () => void h.save({ ...cfg, aiProvider: p.id } as Settings) })
    }
    for (const th of themes) {
      out.push({ title: `Theme: ${th.name}`, group: 'Theme', detail: cfg.theme === th.id ? 'in use' : '', run: () => void h.save({ ...cfg, theme: th.id } as Settings) })
    }
    for (const [field, label, page] of toggles) {
      const on = Boolean(cfg[field])
      out.push({
        title: `${on ? 'Turn off' : 'Turn on'}: ${label}`, group: 'Setting', detail: `now ${on ? 'on' : 'off'}`,
        run: () => void h.save({ ...cfg, [field]: !on } as Settings),
      })
      out.push({ title: `Settings → ${label}`, group: 'Setting', run: () => h.openSettings(page, label) })
    }
    for (const [label, page, find] of places) out.push({ title: `Settings → ${label}`, group: 'Setting', run: () => h.openSettings(page, find) })
    h.tabs().forEach((t, i) => out.push({ title: `Tab ${i + 1}: ${t.title}`, group: 'Tab', detail: t.detail, keys: i < 9 ? keyLabel(commands.find((c) => c.id === `tab${i + 1}`)?.keys ?? '') : '', run: t.go }))
    // "Settings → x" duplicates the toggle's label: keep it only when searching.
    return out
  }

  private filter() {
    const q = this.input.value.trim()
    const all = this.items()
    if (!q) {
      const r = recent()
      const byTitle = new Map(all.map((i) => [i.title, i]))
      const first = r.map((t) => byTitle.get(t)).filter((i): i is PaletteItem => !!i)
      this.shown = [...first, ...all.filter((i) => i.group === 'Action' && !first.includes(i))]
    } else {
      this.shown = all
        .map((i) => ({ i, s: Math.max(score(q, i.title), score(q, `${i.group} ${i.title}`) - 4) }))
        .filter((x) => x.s >= 0)
        .sort((a, b) => b.s - a.s)
        .slice(0, 50)
        .map((x) => x.i)
    }
    this.sel = 0
    this.render()
  }

  private render() {
    this.list.textContent = ''
    if (!this.shown.length) {
      this.list.append(el('li', { class: 'empty', text: 'Nothing matches' }))
      return
    }
    this.shown.forEach((item, i) => {
      const li = el('li', { role: 'option', id: `palette-${i}`, class: i === this.sel ? 'on' : '' },
        el('span', { class: 'group', text: item.group }),
        el('span', { class: 'title', text: item.title }),
        el('span', { class: 'detail', text: item.detail ?? '' }),
        el('kbd', { text: item.keys ?? '' }))
      if (!item.keys) li.querySelector('kbd')!.remove()
      li.setAttribute('aria-selected', String(i === this.sel))
      li.addEventListener('mousemove', () => {
        if (this.sel !== i) { this.sel = i; this.mark() }
      })
      li.addEventListener('click', () => this.choose(i))
      this.list.append(li)
    })
    this.input.setAttribute('aria-activedescendant', `palette-${this.sel}`)
  }

  private mark() {
    this.list.querySelectorAll('li').forEach((li, i) => {
      li.classList.toggle('on', i === this.sel)
      li.setAttribute('aria-selected', String(i === this.sel))
    })
    this.list.children[this.sel]?.scrollIntoView({ block: 'nearest' })
    this.input.setAttribute('aria-activedescendant', `palette-${this.sel}`)
  }

  private key(e: KeyboardEvent) {
    const n = this.shown.length
    if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) { this.sel = (this.sel + 1) % Math.max(n, 1); this.mark() }
    else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) { this.sel = (this.sel - 1 + n) % Math.max(n, 1); this.mark() }
    else if (e.key === 'Enter') this.choose(this.sel)
    else if (e.key === 'Escape') this.close()
    else return
    e.preventDefault()
    e.stopPropagation()
  }

  private choose(i: number) {
    const item = this.shown[i]
    if (!item) return
    remember(item.title)
    // Actions that open something else take the focus themselves.
    this.close(item.group === 'Setting' && item.title.startsWith('Turn') || item.group === 'Theme' || item.group === 'AI')
    item.run()
  }
}
