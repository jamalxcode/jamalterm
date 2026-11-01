import '@xterm/xterm/css/xterm.css'
import '@fontsource/jetbrains-mono/400.css'
import '@fontsource/jetbrains-mono/700.css'
import './style.css'

import { Settings, ShellName, HomeDir, Quit, Version, GPUOff } from '../wailsjs/go/main/App'
import { EventsOn, WindowSetTitle } from '../wailsjs/runtime/runtime'
import type { settings } from '../wailsjs/go/models'
import { Tab, setGpuOff } from './tab'
import { themeById, applyChrome } from './themes'
import { AIPanel, CommandBar } from './ai'
import { SettingsView } from './settingsView'
import { el, isMac, toast } from './util'

let cfg: settings.Settings
let shellName = 'shell'
let home = ''
const tabs: Tab[] = []
let active: Tab | undefined
let nextId = 1

const tabsEl = document.getElementById('tabs')!
const termsEl = document.getElementById('terminals')!
const hintEl = document.getElementById('hint')!
const findEl = document.getElementById('find')!
document.body.classList.toggle('mac', isMac)

const hooks = {
  onChange: (t: Tab) => {
    renderTabs()
    if (t === active) updateChrome()
  },
  onExplain: (t: Tab, b: Parameters<AIPanel['explain']>[1]) => {
    if (!aiReady()) return
    t.exitBrowse()
    void ai.explain(t, b)
  },
  settings: () => cfg,
  shellName: () => shellName,
  home: () => home,
}

const aiHost = {
  activeTab: () => active,
  shellName: () => shellName,
  openSettings: () => void settingsView.open(),
  model: () => cfg.aiModel,
}
const ai = new AIPanel(aiHost)
const commandBar = new CommandBar(aiHost)
const settingsView = new SettingsView({
  current: () => cfg,
  preview: (s) => applySettings(s),
  saved: (s) => applySettings(s),
  closed: () => active?.term.focus(),
})

function aiReady(): boolean {
  if (!cfg.aiEnabled) {
    toast('AI is turned off in Settings (⌘,)')
    return false
  }
  return true
}

// ---- Settings -------------------------------------------------------------------

function applySettings(s: settings.Settings) {
  cfg = s
  const theme = themeById(s.theme)
  applyChrome(theme)
  for (const t of tabs) t.applySettings(s, theme)
  document.body.classList.toggle('no-ai', !s.aiEnabled)
}

// ---- Tabs -----------------------------------------------------------------------

function newTab(cwd = active?.cwd ?? '') {
  const tab = new Tab(nextId++, termsEl, cwd, hooks, themeById(cfg.theme))
  tabs.push(tab)
  activate(tab)
  void tab.start()
  return tab
}

function activate(tab: Tab) {
  if (active && active !== tab) active.exitBrowse()
  active = tab
  for (const t of tabs) t.pane.classList.toggle('active', t === tab)
  requestAnimationFrame(() => {
    tab.refit()
    tab.term.focus()
  })
  closeFind()
  ai.syncTab()
  renderTabs()
  updateChrome()
}

function closeTab(tab = active) {
  if (!tab) return
  const i = tabs.indexOf(tab)
  tabs.splice(i, 1)
  ai.forgetTab(tab.id)
  tab.close()
  if (!tabs.length) {
    Quit()
    return
  }
  activate(tabs[Math.min(i, tabs.length - 1)])
}

function cycle(dir: number) {
  if (!active || tabs.length < 2) return
  const i = tabs.indexOf(active)
  activate(tabs[(i + dir + tabs.length) % tabs.length])
}

function renderTabs() {
  tabsEl.textContent = ''
  tabs.forEach((t, i) => {
    const label = el('span', { class: 'label', text: t.title })
    const close = el('button', { class: 'close', title: 'Close tab (⌘W)' }, '×')
    const state = t.runningBlock ? 'running' : t.exited ? 'exited' : ''
    const item = el('div', { class: `tab ${t === active ? 'active' : ''} ${state}`, title: t.prettyCwd || t.title }, el('i', { class: 'state' }), label, close)
    if (i < 9) item.append(el('span', { class: 'num', text: `⌘${i + 1}` }))
    item.addEventListener('mousedown', (e) => {
      if (e.button === 1) closeTab(t)
      else if (e.target !== close) activate(t)
    })
    close.addEventListener('click', (e) => {
      e.stopPropagation()
      closeTab(t)
    })
    tabsEl.append(item)
  })
  const plus = el('button', { class: 'new-tab', title: 'New tab (⌘T)' }, '+')
  plus.addEventListener('click', () => newTab())
  tabsEl.append(plus)
}

function updateChrome() {
  if (!active) return
  const title = active.prettyCwd ? `${active.title} — ${active.prettyCwd}` : active.title
  WindowSetTitle(title)
  const hint = active.browseHint()
  hintEl.innerHTML = hint
  hintEl.hidden = !hint
  document.body.classList.toggle('browsing', !!hint)
}

// ---- Find (⌘F) --------------------------------------------------------------------

const findInput = el('input', { type: 'text', placeholder: 'Find', spellcheck: 'false' }) as HTMLInputElement
const findCount = el('span', { class: 'count' })
findEl.append(findInput, findCount)
const findOpts = () => ({
  decorations: {
    matchBackground: '#ffc77755',
    activeMatchBackground: '#ff8fb1aa',
    matchOverviewRuler: '#ffc777',
    activeMatchColorOverviewRuler: '#ff8fb1',
  },
})

function openFind() {
  findEl.hidden = false
  findInput.select()
  findInput.focus()
}

function closeFind() {
  if (findEl.hidden) return
  findEl.hidden = true
  active?.search.clearDecorations()
  active?.term.focus()
}

findInput.addEventListener('input', () => active?.search.findNext(findInput.value, { ...findOpts(), incremental: true }))
findInput.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') closeFind()
  else if (e.key === 'Enter') {
    if (e.shiftKey) active?.search.findPrevious(findInput.value, findOpts())
    else active?.search.findNext(findInput.value, findOpts())
  }
})

// ---- About ---------------------------------------------------------------------

async function about() {
  const box = document.getElementById('about')!
  const version = await Version()
  box.innerHTML = `<div class="dialog about"><div class="logo">&gt;_</div><h2>jamalterm</h2>
    <p class="dim">Version ${version}</p>
    <p>A terminal with a free-floating cursor and Claude built in.</p>
    <table class="keys">
      <tr><td><kbd>⌘↑</kbd> or click</td><td>browse past commands</td></tr>
      <tr><td><kbd>⏎</kbd> in browse mode</td><td>run that command again</td></tr>
      <tr><td><kbd>⌘I</kbd></td><td>ask Claude</td></tr>
      <tr><td><kbd>⌘J</kbd></td><td>command from plain English</td></tr>
      <tr><td><kbd>⌘T</kbd> <kbd>⌘W</kbd></td><td>new / close tab</td></tr>
    </table>
    <button class="primary">OK</button></div>`
  box.hidden = false
  const close = () => {
    box.hidden = true
    active?.term.focus()
  }
  box.querySelector('button')!.addEventListener('click', close)
  box.onmousedown = (e) => e.target === box && close()
  ;(box.querySelector('button') as HTMLElement).focus()
}

// ---- Menu commands ------------------------------------------------------------------

const lastRun = new Map<string, number>()

function runMenu(cmd: string) {
  // The menu and the key handler can both see a shortcut; run it once.
  const now = Date.now()
  if (now - (lastRun.get(cmd) ?? 0) < 150) return
  lastRun.set(cmd, now)

  const t = active
  switch (cmd) {
    case 'about': return void about()
    case 'settings': return void settingsView.open()
    case 'newTab': return void newTab()
    case 'closeTab': return closeTab()
    case 'nextTab': return cycle(1)
    case 'prevTab': return cycle(-1)
    case 'clear':
      t?.exitBrowse()
      t?.term.clear()
      return
    case 'find': return openFind()
    case 'browse':
    case 'prevCommand':
      return t?.jumpBlock(-1)
    case 'nextCommand': return t?.jumpBlock(1)
    case 'zoomIn': return t && setFont((t.term.options.fontSize ?? cfg.fontSize) + 1)
    case 'zoomOut': return t && setFont((t.term.options.fontSize ?? cfg.fontSize) - 1)
    case 'zoomReset': return setFont(cfg.fontSize)
    case 'aiPanel': return aiReady() ? ai.toggle() : undefined
    case 'aiCommand': return aiReady() ? void commandBar.open() : undefined
    case 'aiExplainLast': {
      if (!aiReady() || !t) return
      const b = t.lastFailed ?? t.lastFinished
      if (b) void ai.explain(t, b)
      else toast('No command to explain yet')
      return
    }
  }
  const m = /^tab(\d)$/.exec(cmd)
  if (m) {
    const n = parseInt(m[1], 10)
    const target = n === 9 ? tabs[tabs.length - 1] : tabs[n - 1]
    if (target) activate(target)
  }
}

function setFont(size: number) {
  for (const t of tabs) t.setFontSize(size)
}

EventsOn('menu', runMenu)

// ⌘ shortcuts typed while the terminal has focus also arrive here, in case
// the menu bar didn't take them (e.g. in development).
const keyMap: Record<string, string> = {
  t: 'newTab', w: 'closeTab', k: 'clear', f: 'find', ',': 'settings',
  i: 'aiPanel', j: 'aiCommand', '=': 'zoomIn', '+': 'zoomIn', '-': 'zoomOut', '0': 'zoomReset',
  ArrowUp: 'browse', '}': 'nextTab', '{': 'prevTab', ']': 'nextTab', '[': 'prevTab',
}
window.addEventListener('keydown', (e) => {
  if (!e.metaKey || e.altKey || e.ctrlKey) return
  if (settingsView.visible && e.key !== ',') return
  let cmd = keyMap[e.key]
  if (/^[1-9]$/.test(e.key)) cmd = `tab${e.key}`
  if (e.key === 'ArrowUp' && e.shiftKey) cmd = 'prevCommand'
  if (e.key === 'ArrowDown' && e.shiftKey) cmd = 'nextCommand'
  if (e.key === 'e' && e.shiftKey) cmd = 'aiExplainLast'
  if (cmd) {
    e.preventDefault()
    e.stopPropagation() // xterm.js must not handle it a second time
    runMenu(cmd)
  }
}, true)

// ---- Events from Go --------------------------------------------------------------

EventsOn('pty:data', (e: { id: number; data: string }) => tabs.find((t) => t.id === e.id)?.write(e.data))
EventsOn('pty:exit', (e: { id: number; code: number }) => tabs.find((t) => t.id === e.id)?.onExit(e.code))
EventsOn('settings:changed', (s: settings.Settings) => applySettings(s))

window.addEventListener('resize', () => requestAnimationFrame(() => active?.refit()))
document.getElementById('ai-button')!.addEventListener('click', () => runMenu('aiPanel'))

// ---- Start --------------------------------------------------------------------

async function boot() {
  cfg = await Settings()
  applySettings(cfg)
  ;[shellName, home] = await Promise.all([ShellName(), HomeDir()])
  setGpuOff(await GPUOff())
  // Wait for the font so the character grid is measured correctly.
  await Promise.all([
    document.fonts.load(`${cfg.fontSize}px "JetBrains Mono"`),
    document.fonts.load(`bold ${cfg.fontSize}px "JetBrains Mono"`),
  ]).catch(() => undefined)
  await document.fonts.ready
  newTab('')
}

void boot()
