import '@xterm/xterm/css/xterm.css'
import '@fontsource/jetbrains-mono/400.css'
import '@fontsource/jetbrains-mono/700.css'
import './style.css'

import { Settings, SaveSettings, ShowProblemLog, ShellName, HomeDir, Quit, Version, GPUOff, SetWindowOpacity, SaveSession, LoadSession, IsPrimaryWindow, NewWindow, StartDir, Notify } from '../wailsjs/go/main/App'
import { EventsOn, WindowSetTitle } from '../wailsjs/runtime/runtime'
import type { settings } from '../wailsjs/go/models'
import { Tab, setGpuOff, type SavedTab } from './tab'
import { themeById, applyChrome } from './themes'
import { AIPanel, CommandBar } from './ai'
import { SettingsView } from './settingsView'
import { Updater } from './updater'
import { showDiagnostics, logWindowErrors } from './diagnostics'
import { Palette } from './palette'
import { Group, type Dir, type SavedNode, type PaneNode } from './panes'
import { loadProviders, providerInfo, modelOf } from './providers'
import { el, isMac, toast, duration } from './util'

let cfg: settings.Settings
let shellName = 'shell'
let home = ''
// tabs: every terminal; groups: the tabs in the tab bar, each holding one
// terminal or several split panes. active: the terminal with the focus.
const tabs: Tab[] = []
const groups: Group[] = []
let active: Tab | undefined
const groupOf = (t: Tab | undefined) => (t ? groups.find((g) => g.has(t)) : undefined)
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
  onCommandDone: (t: Tab, b: Parameters<AIPanel['explain']>[1]) => {
    // Tell the user when a long command finishes while they're elsewhere.
    if (!cfg.notifyLongCommands || !b.startedAt || !b.finishedAt || !b.command) return
    const took = b.finishedAt - b.startedAt
    if (took < cfg.notifyAfterSeconds * 1000 || document.hasFocus()) return
    const ok = b.exitCode === 0
    void Notify(`${ok ? '✓ Finished' : `✕ Failed (exit ${b.exitCode})`}: ${b.command.slice(0, 60)}`, `${duration(took)} · ${t.prettyCwd || t.title}`)
  },
  settings: () => cfg,
  shellName: () => shellName,
  home: () => home,
}

const aiHost = {
  activeTab: () => active,
  shellName: () => shellName,
  openSettings: () => void settingsView.open(),
  assistant: () => {
    const info = providerInfo(cfg.aiProvider) ?? providerInfo('anthropic')
    const id = info?.id ?? 'anthropic'
    const model = modelOf(cfg, id)
    return { id, name: info?.name ?? 'Claude', model: model ? model.replace(/^claude-/, '') : 'auto model' }
  },
  runInNewTab: (cmd: string) => void runInNewTab(cmd),
}
const ai = new AIPanel(aiHost)
const commandBar = new CommandBar(aiHost)
const settingsView = new SettingsView({
  current: () => cfg,
  preview: (s) => applySettings(s),
  saved: (s) => applySettings(s),
  closed: () => active?.term.focus(),
  checkUpdates: () => void updater.check(true),
  runInNewTab: (cmd) => void runInNewTab(cmd),
  diagnostics: () => void showDiagnostics(() => active?.term.focus()),
})

const palette = new Palette({
  run: (cmd) => runMenu(cmd),
  settings: () => cfg,
  save: async (s) => {
    try {
      applySettings(await SaveSettings(s))
    } catch (err) {
      toast(String(err), 'error')
    }
  },
  openSettings: (page, find) => void settingsView.open(page, find),
  tabs: () => groups.map((g) => ({ title: g.focused.title + (g.count > 1 ? ` (${g.count} panes)` : ''), detail: g.focused.prettyCwd, go: () => activate(g.focused) })),
  focus: () => active?.term.focus(),
})

/** Opens a tab and runs a command in it once the shell is ready. */
async function runInNewTab(command: string) {
  const tab = newTab()
  await tab.whenReady()
  tab.sendCommand(command, true)
}

const updater = new Updater(saveSessionNow)

function aiReady(): boolean {
  if (!cfg.aiEnabled) {
    toast('AI is turned off in Settings (⌘,)')
    return false
  }
  return true
}

// ---- Settings -------------------------------------------------------------------


function applySettings(s: settings.Settings) {
  const switched = cfg && cfg.aiProvider !== s.aiProvider
  cfg = s
  if (switched) {
    toast(`Now using ${providerInfo(s.aiProvider)?.name ?? s.aiProvider}`)
    void ai.refreshHeader()
  }
  const theme = themeById(s.theme)
  applyChrome(theme, s.opacity / 100)
  SetWindowOpacity(s.opacity) // also used for the live preview in Settings
  for (const t of tabs) t.applySettings(s, theme)
  document.body.classList.toggle('no-ai', !s.aiEnabled)
}

// ---- Tabs -----------------------------------------------------------------------

/** Creates a terminal (not yet placed in a group). */
function makeTerm(cwd: string, saved?: SavedTab) {
  const tab = new Tab(nextId++, termsEl, cwd, hooks, themeById(cfg.theme))
  if (saved) tab.restore(saved)
  tabs.push(tab)
  // Clicking a pane gives it the focus.
  tab.pane.addEventListener('mousedown', () => active !== tab && activate(tab), true)
  return tab
}

const refitGroup = (g: Group) => requestAnimationFrame(() => g.tabs().forEach((t) => t.refit()))

function newTab(cwd = active?.cwd ?? '', saved?: SavedTab) {
  const tab = makeTerm(cwd, saved)
  groups.push(new Group(termsEl, tab, refitGroup))
  activate(tab)
  void tab.start()
  sessionDirty = true
  return tab
}

/** Splits the focused pane: 'row' puts the new one on the right, 'column' below. */
function splitPane(dir: Dir) {
  const from = active
  const g = groupOf(from)
  if (!from || !g) return void newTab()
  const tab = makeTerm(from.cwd)
  g.split(from, tab, dir)
  activate(tab)
  void tab.start()
  sessionDirty = true
}

/** Rebuilds a saved tab with its split panes. */
function restoreGroup(node: SavedNode) {
  const made: Tab[] = []
  const build = (n: SavedNode): PaneNode => {
    if ('tab' in n) {
      const t = makeTerm(n.tab.cwd || '', n.tab)
      made.push(t)
      return { tab: t }
    }
    return { dir: n.dir === 'column' ? 'column' : 'row', ratio: Math.min(0.9, Math.max(0.1, Number(n.ratio) || 0.5)), a: build(n.a), b: build(n.b) }
  }
  const root = build(node)
  const g = new Group(termsEl, made[0], refitGroup)
  g.root = root
  g.render()
  groups.push(g)
  for (const t of made) void t.start()
  return g
}

// ---- Session: tabs come back after a restart or update ----------------------

let sessionDirty = false

async function saveSessionNow() {
  sessionDirty = false
  const g = groupOf(active)
  const data = { version: 2, active: g ? groups.indexOf(g) : 0, groups: groups.map((x) => x.snapshot()) }
  try {
    await SaveSession(JSON.stringify(data))
  } catch {
    /* best effort */
  }
}

setInterval(() => {
  if (sessionDirty) void saveSessionNow()
}, 3000)
window.addEventListener('beforeunload', () => void saveSessionNow())

async function restoreSession(): Promise<boolean> {
  if (!cfg.restoreSession || !(await IsPrimaryWindow())) return false
  let data: { active?: number; tabs?: SavedTab[]; groups?: SavedNode[] }
  try {
    data = JSON.parse((await LoadSession()) || '{}')
  } catch {
    return false
  }
  // Version 1 had one terminal per tab.
  const saved = data.groups ?? (data.tabs ?? []).map((t) => ({ tab: t }))
  if (!saved.length) return false
  for (const n of saved) restoreGroup(n)
  const want = groups[data.active ?? 0] ?? groups[0]
  activate(want.focused)
  return true
}

function activate(tab: Tab) {
  if (active && active !== tab) active.exitBrowse()
  active = tab
  const g = groupOf(tab)
  for (const x of groups) x.setActive(x === g)
  g?.setFocused(tab)
  requestAnimationFrame(() => {
    for (const t of g?.tabs() ?? [tab]) t.refit()
    tab.term.focus()
  })
  closeFind()
  ai.syncTab()
  renderTabs()
  updateChrome()
}

/** Closes a terminal: the focused pane, or the whole tab if it's alone. */
function closeTab(tab = active) {
  if (!tab) return
  const g = groupOf(tab)
  tabs.splice(tabs.indexOf(tab), 1)
  ai.forgetTab(tab.id)
  const next = g?.remove(tab)
  tab.close()
  sessionDirty = true
  if (g && next) return activate(tab === active ? next : active!)
  if (g) closeGroup(g)
}

/** Closes a whole tab with all its panes. */
function closeGroup(g = groupOf(active)) {
  if (!g) return
  for (const t of g.tabs()) {
    tabs.splice(tabs.indexOf(t), 1)
    ai.forgetTab(t.id)
    t.close()
  }
  const i = groups.indexOf(g)
  groups.splice(i, 1)
  g.el.remove()
  sessionDirty = true
  if (!groups.length) {
    void saveSessionNow().then(() => Quit()) // nothing to reopen next time
    return
  }
  activate(groups[Math.min(i, groups.length - 1)].focused)
}

function cycle(dir: number) {
  const g = groupOf(active)
  if (!g || groups.length < 2) return
  const i = groups.indexOf(g)
  activate(groups[(i + dir + groups.length) % groups.length].focused)
}

/** Moves the focus between panes: in order (dir ±1) or by direction. */
function cyclePane(dir: number) {
  const g = groupOf(active)
  if (!g || !active || g.count < 2) return
  const list = g.tabs()
  activate(list[(list.indexOf(active) + dir + list.length) % list.length])
}
function movePane(dx: number, dy: number) {
  const t = active && groupOf(active)?.neighbour(active, dx, dy)
  if (t) activate(t)
}

const STATUS_TEXT = { running: 'running', failed: 'last command failed', exited: 'shell exited', idle: '' }

function renderTabs() {
  tabsEl.textContent = ''
  groups.forEach((g, i) => {
    const t = g.focused
    const status = t.status
    // Folder first; while something runs, its name leads.
    const label = el('span', { class: 'label' })
    const run = t.runningName
    if (run) label.append(el('span', { class: 'cmd', text: run }), el('span', { class: 'sep', text: ' · ' }))
    label.append(el('span', { class: 'path', text: t.shortCwd || t.title }))
    if (t.branch) label.append(el('span', { class: 'branch' }, el('i', { class: 'nf', text: '\ue725' }), t.branch))
    if (status === 'exited') label.append(el('span', { class: 'sep', text: ' (exited)' }))
    if (g.count > 1) label.append(el('span', { class: 'panes', title: `${g.count} panes`, text: ` ⊞${g.count}` }))

    const last = t.lastFinished
    const tip = [
      t.prettyCwd || t.title,
      t.branch ? `Branch: ${t.branch}` : '',
      t.runningBlock ? `Running: ${t.runningBlock.command}` : '',
      !t.runningBlock && last ? `Last: ${last.command}  →  ${last.exitCode === 0 ? 'OK' : `failed (exit ${last.exitCode})`}` : '',
      g.count > 1 ? `${g.count} panes (⌘] next pane)` : '',
      i < 9 ? `⌘${i + 1} to switch` : '',
    ].filter(Boolean).join('\n')

    const close = el('button', { class: 'close', title: 'Close tab (⌘W)' }, '×')
    const icon = el('i', { class: `state ${status}`, 'aria-label': STATUS_TEXT[status] }, status === 'failed' ? '✕' : '')
    const num = el('span', { class: 'num', text: String(i + 1) })
    const item = el('div', { class: `tab ${g.has(active!) ? 'active' : ''} ${status}`, title: tip }, ...(i < 9 ? [num] : []), icon, label, close)
    item.addEventListener('mousedown', (e) => {
      if (e.button === 1) closeGroup(g)
      else if (e.target !== close) activate(g.focused)
    })
    close.addEventListener('click', (e) => {
      e.stopPropagation()
      closeGroup(g)
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
  if (cmd.startsWith('toast:')) return toast(cmd.slice(6), 'error')
  switch (cmd) {
    case 'about': return void about()
    case 'palette': return palette.open()
    case 'showLog': return void ShowProblemLog()
    case 'diagnostics': return void showDiagnostics(() => active?.term.focus())
    case 'checkUpdates': return void updater.check(true)
    case 'revertUpdate': return void updater.offerRevert()
    case 'settings': return void settingsView.open()
    case 'newTab': return void newTab()
    case 'newWindow': return void NewWindow(active?.cwd ?? '')
    case 'closeTab': return closeTab()
    case 'nextTab': return cycle(1)
    case 'prevTab': return cycle(-1)
    case 'splitRight': return splitPane('row')
    case 'splitDown': return splitPane('column')
    case 'nextPane': return cyclePane(1)
    case 'prevPane': return cyclePane(-1)
    case 'paneLeft': return movePane(-1, 0)
    case 'paneRight': return movePane(1, 0)
    case 'paneUp': return movePane(0, -1)
    case 'paneDown': return movePane(0, 1)
    case 'closeGroup': return closeGroup()
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
    const target = n === 9 ? groups[groups.length - 1] : groups[n - 1]
    if (target) activate(target.focused)
  }
}

function setFont(size: number) {
  for (const t of tabs) t.setFontSize(size)
}

EventsOn('menu', runMenu)

// ⌘ shortcuts typed while the terminal has focus also arrive here, in case
// the menu bar didn't take them (e.g. in development).
const keyMap: Record<string, string> = {
  t: 'newTab', n: 'newWindow', w: 'closeTab', k: 'clear', f: 'find', ',': 'settings',
  i: 'aiPanel', j: 'aiCommand', '=': 'zoomIn', '+': 'zoomIn', '-': 'zoomOut', '0': 'zoomReset',
  ArrowUp: 'browse', '}': 'nextTab', '{': 'prevTab', ']': 'nextPane', '[': 'prevPane', d: 'splitRight',
}
window.addEventListener('keydown', (e) => {
  if (!e.metaKey || e.altKey || e.ctrlKey) return
  if (settingsView.visible && e.key !== ',') return
  let cmd = keyMap[e.key]
  if (/^[1-9]$/.test(e.key)) cmd = `tab${e.key}`
  if (e.key === 'ArrowUp' && e.shiftKey) cmd = 'prevCommand'
  if (e.key === 'ArrowDown' && e.shiftKey) cmd = 'nextCommand'
  if (e.key === 'e' && e.shiftKey) cmd = 'aiExplainLast'
  if (e.key.toLowerCase() === 'p' && e.shiftKey) cmd = 'palette'
  if (e.key.toLowerCase() === 'd' && e.shiftKey) cmd = 'splitDown'
  if (e.key.toLowerCase() === 'w' && e.shiftKey) cmd = 'closeGroup'
  if (e.key === ']' && e.shiftKey) cmd = 'nextTab'
  if (e.key === '[' && e.shiftKey) cmd = 'prevTab'
  if (cmd) {
    e.preventDefault()
    e.stopPropagation() // xterm.js must not handle it a second time
    runMenu(cmd)
  }
}, true)

// ---- Events from Go --------------------------------------------------------------

EventsOn('pty:data', (e: { id: number; data: string }) => {
  tabs.find((t) => t.id === e.id)?.write(e.data)
  sessionDirty = true
})
EventsOn('pty:exit', (e: { id: number; code: number }) => tabs.find((t) => t.id === e.id)?.onExit(e.code))
EventsOn('settings:changed', (s: settings.Settings) => applySettings(s))

window.addEventListener('resize', () => requestAnimationFrame(() => active?.refit()))
document.getElementById('ai-button')!.addEventListener('click', () => runMenu('aiPanel'))

// ---- Start --------------------------------------------------------------------

async function boot() {
  await loadProviders()
  cfg = await Settings()
  applySettings(cfg)
  ;[shellName, home] = await Promise.all([ShellName(), HomeDir()])
  setGpuOff(await GPUOff())
  // Wait for the font so the character grid is measured correctly.
  await Promise.all([
    document.fonts.load(`${cfg.fontSize}px "JetBrains Mono"`),
    document.fonts.load(`bold ${cfg.fontSize}px "JetBrains Mono"`),
    // Load the icon font before the terminal caches any glyphs.
    document.fonts.load(`${cfg.fontSize}px "Symbols Nerd Font Mono"`, '\ue0b0\uf07b'),
  ]).catch(() => undefined)
  await document.fonts.ready
  if (!(await restoreSession())) newTab(await StartDir())
  void updater.startAuto(() => cfg.autoUpdate)
}

logWindowErrors()
void boot()
