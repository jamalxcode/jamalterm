import '@xterm/xterm/css/xterm.css'
import '@fontsource/jetbrains-mono/400.css'
import '@fontsource/jetbrains-mono/700.css'
import './style.css'

import { Settings, SaveSettings, ShowProblemLog, ShellName, HomeDir, CloseWindow, WindowID, GPUOff, SetWindowOpacity, SaveSession, LoadSession, IsPrimaryWindow, NewWindow, StartDir, Notify } from './backend'
import { EventsOn, WindowSetTitle } from './backend'
import { Tab, setGpuOff, hostMatches, type SavedTab } from './tab'
import { themeById, applyChrome } from './themes'
import { applySystemLook } from './system'
import { AIPanel, CommandBar } from './ai'
import { Updater } from './updater'
import { showWelcome, showLastCrash, showWhatsNew } from './cards'
import { showDiagnostics, logWindowErrors } from './diagnostics'
import { Palette } from './palette'
import { FindBar } from './find'
import { showAbout } from './about'
import { loadCommands, commandForKey, runCommand, commands } from './keys'
import { runSmoke } from './smoke'
import { SmokeMode } from './backend'
import { Group, type Dir, type SavedNode, type PaneNode } from './panes'
import { loadProviders, providerInfo, modelOf } from './providers'
import { el, isMac, toast, duration, quotePath, stored, store } from './util'
import { OpenURL, Confirm, OpenSettingsAt, SetMenuState, BusyTabs, QuitNow, CloseWindowNow, SetActiveTab, LiveTabs, RemoteSetupLine, Write } from './backend'
import type { Settings as Prefs } from './types'

let cfg: Prefs
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
termsEl.setAttribute('data-file-drop-target', '') // files dropped here are typed as paths
const hintEl = document.getElementById('hint')!
const findEl = document.getElementById('find')!
document.body.classList.toggle('mac', isMac)

const sshTipShown = new WeakSet<Tab>()

const hooks = {
  onChange: (t: Tab) => {
    // The first ssh in a tab: say how browse mode can work over there too.
    if (t.runningName === 'ssh' && !sshTipShown.has(t)) {
      sshTipShown.add(t)
      toast('Browse mode on that server: Shell → Enable Browse Mode on This Server (once you\'re logged in)')
    }
    rememberFolder(t.cwd)
    renderTabs()
    if (t === active) updateChrome()
    else syncMenu() // the Window menu names every tab
  },
  onExplain: (t: Tab, b: Parameters<AIPanel['explain']>[1]) => {
    if (!aiReady()) return
    t.exitBrowse()
    void ai.explain(t, b)
  },
  onFix: (t: Tab, b: Parameters<AIPanel['explain']>[1]) => {
    if (!aiReady()) return
    t.exitBrowse()
    void ai.fix(t, b)
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
  // Broadcast: what's typed in one pane goes to the tab's other panes too.
  onInput: (t: Tab, data: string) => {
    const g = groupOf(t)
    if (!g?.broadcast) return
    for (const o of g.tabs()) if (o !== t && o.running) o.input(data, false)
  },
}

const aiHost = {
  activeTab: () => active,
  shellName: () => shellName,
  openSettings: () => void OpenSettingsAt('ai', ''),
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
const palette = new Palette({
  run: (id) => {
    const c = commands.find((x) => x.id === id)
    if (c) runCommand(c, runMenu)
    else runMenu(id)
  },
  settings: () => cfg,
  save: async (s) => {
    try {
      applySettings(await SaveSettings(s))
    } catch (err) {
      toast(String(err), 'error')
    }
  },
  openSettings: (page, find) => void OpenSettingsAt(page, find ?? ''),
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


function applySettings(s: Prefs) {
  const switched = cfg && cfg.aiProvider !== s.aiProvider
  cfg = s
  if (switched) {
    toast(`Now using ${providerInfo(s.aiProvider)?.name ?? s.aiProvider}`)
    void ai.refreshHeader()
  }
  const theme = themeById(s.theme)
  applyChrome(theme, s.opacity / 100, s.windowFrame === 'theme' ? 'theme' : 'system')
  SetWindowOpacity(s.opacity) // also used for the live preview in Settings
  for (const t of tabs) t.applySettings(s, theme)
  document.body.classList.toggle('no-ai', !s.aiEnabled)
}

// ---- Tabs -----------------------------------------------------------------------

/** Creates a terminal (not yet placed in a group). */
function makeTerm(cwd: string, saved?: SavedTab, id = nextId++) {
  const tab = new Tab(id, termsEl, cwd, hooks, themeById(cfg.theme))
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
function restoreGroup(node: SavedNode, live?: Set<number>) {
  const made: Tab[] = []
  const attach = new Set<Tab>()
  const build = (n: SavedNode): PaneNode => {
    if ('tab' in n) {
      // After a reload the shell is still running: reconnect to it.
      const again = live && n.tab.id !== undefined && live.has(n.tab.id)
      const t = again ? makeTerm(n.tab.cwd || '', undefined, n.tab.id) : makeTerm(n.tab.cwd || '', n.tab)
      if (again) attach.add(t)
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
  for (const t of made) void (attach.has(t) ? t.attach() : t.start())
  return g
}

/** Keeps only the panes whose shells are still running. */
function pruneLive(n: SavedNode, live: Set<number>): SavedNode | null {
  if ('tab' in n) return n.tab.id !== undefined && live.has(n.tab.id) ? n : null
  const a = pruneLive(n.a, live), b = pruneLive(n.b, live)
  if (a && b) return { ...n, a, b }
  return a ?? b
}

/** A reloaded window: put back the tabs whose shells kept running. */
async function reattach(): Promise<boolean> {
  const live = new Set(await LiveTabs())
  if (!live.size) return false
  let data: { active?: number; groups?: SavedNode[] } = {}
  try {
    data = JSON.parse((await LoadSession()) || '{}')
  } catch { /* use the plain list */ }
  const seen = new Set<number>()
  const walk = (n: SavedNode): void => { if ('tab' in n) { if (n.tab.id !== undefined) seen.add(n.tab.id) } else { walk(n.a); walk(n.b) } }
  for (const n of data.groups ?? []) {
    const kept = pruneLive(n, live)
    if (kept) { walk(kept); restoreGroup(kept, live) }
  }
  // Tabs opened just before the reload (not saved yet): one tab each.
  for (const id of live) if (!seen.has(id)) restoreGroup({ tab: { id, cwd: '' } }, live)
  nextId = Math.max(nextId, ...live) + 1
  const want = groups[data.active ?? 0] ?? groups[0]
  if (want) activate(want.focused)
  return groups.length > 0
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
  for (const x of groups) {
    x.setActive(x === g)
    // GPU drawing only for the panes you can see (browsers allow few
    // WebGL contexts, and hidden ones only use memory).
    for (const p of x.tabs()) p.setVisible(x === g)
  }
  g?.setFocused(tab)
  void SetActiveTab(tab.id)
  requestAnimationFrame(() => {
    for (const t of g?.tabs() ?? [tab]) t.refit()
    tab.term.focus()
  })
  closeFind()
  ai.syncTab()
  renderTabs()
  updateChrome()
}

/** "vim (~/proj), npm (~/web)" for the programs running in these tabs. */
function runningList(busy: { id: number; name: string }[]): string {
  return busy.map((b) => {
    const where = tabs.find((t) => t.id === b.id)?.prettyCwd
    return where ? `${b.name} (${where})` : b.name
  }).join(', ')
}

/** Asks (a native alert, as in Terminal) before stopping programs still
 * running in these tabs. */
async function okToStop(list: Tab[], what: string): Promise<boolean> {
  const busy = await BusyTabs(list.map((t) => t.id))
  if (!busy?.length) return true
  const scope = what === 'Window' ? 'this window' : what === 'Pane' ? 'this pane' : 'this tab'
  return Confirm(
    `Do you want to terminate running processes in ${scope}?`,
    `Closing ${scope} will terminate: ${runningList(busy)}.`,
    'Terminate',
  )
}

/** Closes a terminal: the focused pane, or the whole tab if it's alone. */
async function closeTab(tab = active) {
  if (!tab) return
  if (tab.running && !(await okToStop([tab], groupOf(tab)!.count > 1 ? 'Pane' : 'Tab'))) return
  const g = groupOf(tab)
  tabs.splice(tabs.indexOf(tab), 1)
  ai.forgetTab(tab.id)
  const next = g?.remove(tab)
  tab.close()
  sessionDirty = true
  if (g && next) return activate(tab === active ? next : active!)
  if (g) void closeGroup(g, true)
}

/** Closes a whole tab with all its panes. */
async function closeGroup(g = groupOf(active), asked = false) {
  if (!g) return
  if (!asked && !(await okToStop(g.tabs().filter((t) => t.running), 'Tab'))) return
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
    void saveSessionNow().then(() => CloseWindow()) // the last window closing quits
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

/** The host-color rule for a tab's machine, if any. */
function hostColor(t: Tab): { host: string; color: string } | null {
  const host = t.remoteHost
  if (!host) return null
  const rule = (cfg.hostColors ?? []).find((h) => hostMatches(host, h.pattern))
  return rule ? { host, color: rule.color } : { host, color: '' }
}

function paintHosts() {
  for (const t of tabs) {
    const hc = hostColor(t)
    t.pane.classList.toggle('host-tinted', !!hc?.color)
    t.pane.dataset.host = hc?.color ? hc.host : ''
    if (hc?.color) t.pane.style.setProperty('--host', `var(--${hc.color})`)
    else t.pane.style.removeProperty('--host')
  }
}

const STATUS_TEXT = { running: 'running', failed: 'last command failed', exited: 'shell exited', idle: '' }

function renderTabs() {
  tabsEl.textContent = ''
  groups.forEach((g, i) => {
    const t = g.focused
    const status = t.status
    let item_proj = ''
    // Folder first; while something runs, its name leads.
    const label = el('span', { class: 'label' })
    const run = t.runningName
    if (run) label.append(el('span', { class: 'cmd', text: run }), el('span', { class: 'sep', text: ' · ' }))
    label.append(el('span', { class: 'path', text: t.shortCwd || t.title }))
    if (t.branch) label.append(el('span', { class: 'branch' }, el('i', { class: 'nf', text: '\ue725' }), t.branch))
    if (status === 'exited') label.append(el('span', { class: 'sep', text: ' (exited)' }))
    if (g.tabs().some((x) => x.passwordMode)) label.append(el('span', { class: 'lock', title: 'Password prompt: typing is protected', text: '🔒' }))
    if (g.count > 1) label.append(el('span', { class: 'panes', title: `Split into ${g.count} panes (⌘] next pane)`, text: ` · ${g.count} panes` }))
    if (g.broadcast) label.append(el('span', { class: 'broadcast', title: 'Typing goes to every pane in this tab (⌥⌘I)', text: ' 📣' }))
    const hc = hostColor(t)
    if (!hc && cfg.projectColors && t.repo) {
      item_proj = projectColor(t.repo)
    }
    if (hc) {
      // The machine's name, always as text; the color is extra.
      const chip = el('span', { class: 'host', title: `Connected to ${hc.host}`, text: `@${hc.host}` })
      if (hc.color) chip.style.setProperty('--host', `var(--${hc.color})`)
      label.append(chip)
    }

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
    // Failure is a filled "!" badge, never a ✕ (that's the close button).
    // Idle shows nothing.
    const icon = status === 'idle' ? null
      : el('i', { class: `state ${status}`, title: STATUS_TEXT[status], 'aria-label': STATUS_TEXT[status] }, status === 'failed' ? '!' : '')
    const num = el('span', { class: 'num', text: String(i + 1) })
    const item = el('div', { class: `tab ${g.has(active!) ? 'active' : ''} ${status}${item_proj ? ' proj' : ''}`, title: tip }, ...(i < 9 ? [num] : []), ...(icon ? [icon] : []), label, close)
    if (item_proj) item.style.setProperty('--proj', `var(--${item_proj})`)
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

/** Close Window (⇧⌘W): asks first if something is running in it. */
async function closeWindow() {
  if (!(await okToStop(tabs.filter((t) => t.running), 'Window'))) return
  await saveSessionNow()
  void CloseWindowNow()
}

/** Tells the menu bar what its items would do here. */
// ---- Recent folders (⇧⌘O) and project colors ---------------------------------

/** Folders you've worked in, newest first (shared by all windows). */
function recentFolders(): string[] {
  try { return JSON.parse(stored('jt-folders') ?? '[]') } catch { return [] }
}
function rememberFolder(dir: string) {
  if (!dir || dir === '/') return
  const list = recentFolders()
  if (list[0] === dir) return
  store('jt-folders', JSON.stringify([dir, ...list.filter((d) => d !== dir)].slice(0, 30)))
}
function openRecentFolder() {
  const pretty = (d: string) => (d === home ? '~' : home && d.startsWith(home + '/') ? '~' + d.slice(home.length) : d)
  const items = recentFolders().map((d) => ({
    title: d === home ? 'Home' : d.split('/').filter(Boolean).pop() ?? d,
    group: 'Folder', detail: pretty(d), run: () => newTab(d),
  }))
  if (!items.length) return toast('No folders yet: cd somewhere first')
  palette.openList(items, 'Open a folder in a new tab…')
}

/** Each git repository gets its own tab color (the folder name is always
 * shown too). Red is left for failures. */
const PROJECT_COLORS = ['blue', 'green', 'yellow', 'magenta', 'cyan']
export function projectColor(repo: string): string {
  let h = 0
  for (const ch of repo) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return PROJECT_COLORS[h % PROJECT_COLORS.length]
}

/** A name people can read, for the window title and the Window menu:
 * "Home — zsh", "jamalterm — vim". Never just symbols like "~ — ~". */
function tabName(t: Tab | undefined): string {
  if (!t) return shellName
  if (t.exited) return 'Shell exited'
  const what = t.runningName || shellName
  const folder = !t.cwd ? '' : t.cwd === home ? 'Home' : t.cwd === '/' ? 'Computer root' : t.cwd.split('/').filter(Boolean).pop() ?? ''
  return folder ? `${folder} — ${what}` : what
}

function syncMenu() {
  const g = groupOf(active)
  // Each tab is named after the pane with the focus in it.
  const tabsNow = groups.map((x) => tabName(x === g ? active : x.tabs()[0]))
  void SetMenuState({ split: (g?.count ?? 0) > 1, tabBar: tabBarShown(), broadcast: !!g?.broadcast, tabs: tabsNow })
}

/** As in Terminal, the tab bar appears with two or more tabs (or always,
 * with View → Show Tab Bar). */
function tabBarShown() {
  return groups.length > 1 || !!cfg.showTabBar
}

function updateChrome() {
  paintHosts()
  document.body.classList.toggle('no-tab-bar', !tabBarShown())
  syncMenu()
  if (!active) return
  // "~/proj — zsh", or "~/proj — vim" while something runs (like Terminal).
  windowTitle.textContent = tabName(active)
  const g = groupOf(active)
  broadcastBar.hidden = !g?.broadcast
  if (g?.broadcast) broadcastBar.textContent = `📣 Typing goes to all ${g.count} panes in this tab — ⌥⌘I to stop`
  WindowSetTitle(tabName(active)) // also the window's name in the Window menu
  secureBadge.hidden = !active.passwordMode
  const hint = active.browseHint()
  hintEl.innerHTML = hint
  hintEl.hidden = !hint
  document.body.classList.toggle('browsing', !!hint)
}

const broadcastBar = el('div', { id: 'broadcast-bar', role: 'status', hidden: '' })
// With one tab, the title bar shows the window's title instead, like Terminal.
const windowTitle = el('div', { id: 'window-title', 'aria-hidden': 'true' })
document.getElementById('titlebar')!.insertBefore(windowTitle, document.getElementById('ai-button'))
document.body.append(broadcastBar)

function toggleBroadcast() {
  const g = groupOf(active)
  if (!g) return
  if (g.count < 2 && !g.broadcast) return toast('Split the tab first (⌘D): broadcast types into every pane of a tab')
  g.broadcast = !g.broadcast
  renderTabs()
  updateChrome()
}

// ---- Find (⌘F) and About: find.ts, about.ts ------------------------------------

const find = new FindBar(findEl, () => active)
const openFind = () => find.open()
const closeFind = () => find.close()
const about = () => showAbout(() => active?.term.focus())

// ---- Menu commands ------------------------------------------------------------------

const lastRun = new Map<string, number>()

function runMenu(cmd: string) {
  // The menu and the key handler can both see a shortcut; run it once.
  const now = Date.now()
  if (now - (lastRun.get(cmd) ?? 0) < 150) return
  lastRun.set(cmd, now)

  const t = active
  if (cmd.startsWith('toast:')) return toast(cmd.slice(6), 'error')
  if (cmd.startsWith('run:')) return void runInNewTab(cmd.slice(4)) // from Settings (sign in…)
  switch (cmd) {
    case 'about':
    case 'shortcuts': return void about()
    case 'selectAll': {
      // In a text field, select its text; in the terminal, all its output.
      const f = document.activeElement as HTMLElement | null
      if (f && /^(INPUT|TEXTAREA)$/.test(f.tagName) && !f.classList.contains('xterm-helper-textarea')) (f as HTMLInputElement).select()
      else t?.term.selectAll()
      return
    }
    case 'aiAskSelection': {
      const sel = t?.term.getSelection().trim()
      if (!sel || !aiReady()) return sel ? undefined : toast('Select some text first')
      void ai.show().then(() => ai.send(`Explain this:\n\n${sel.slice(0, 4000)}`))
      return
    }
    case 'lookUp':
    case 'searchWeb': {
      const sel = t?.term.getSelection().trim().slice(0, 300)
      if (!sel) return toast('Select some text first')
      return void OpenURL(cmd === 'lookUp' ? `dict://${encodeURIComponent(sel)}` : `https://www.google.com/search?q=${encodeURIComponent(sel)}`)
    }
    case 'findNext': return find.next()
    case 'findPrevious': return find.next(true)
    case 'findSelection': return find.useSelection()
    case 'toggleTabBar': return void SaveSettings({ ...cfg, showTabBar: !cfg.showTabBar }).then(applySettings)
    case 'closeWindow': return void closeWindow()
    case 'palette': return palette.open()
    case 'remoteIntegration':
      if (!t) return
      void RemoteSetupLine().then((line) => {
        Write(t.id, line + '\r')
        t.term.focus()
      })
      return
    case 'showLog': return void ShowProblemLog()
    case 'diagnostics': return void showDiagnostics(() => active?.term.focus())
    case 'checkUpdates': return void updater.check(true)
    case 'revertUpdate': return void updater.offerRevert()
    case 'settings': return void OpenSettingsAt('', '')
    case 'newTab': return void newTab()
    case 'recentFolders': return openRecentFolder()
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
    case 'broadcast': return toggleBroadcast()
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

// ⌘ shortcuts typed while the terminal has the focus also arrive here, in
// case the menu bar didn't take them (the same table as the menu: keys.ts).
window.addEventListener('keydown', (e) => {
  const c = commandForKey(e)
  if (!c) return
  e.preventDefault()
  e.stopPropagation() // xterm.js must not handle it a second time
  runCommand(c, runMenu)
}, true)

// ---- Events from Go --------------------------------------------------------------

EventsOn('pty:data', (e: { id: number; data: string }) => {
  tabs.find((t) => t.id === e.id)?.write(e.data)
  sessionDirty = true
})
EventsOn('pty:exit', (e: { id: number; code: number }) => tabs.find((t) => t.id === e.id)?.onExit(e.code))
// Quitting or closing the window would stop running programs: ask first.
EventsOn('confirm', async (e: { kind: string; busy: { id: number; name: string }[] }) => {
  const quit = e.kind === 'quit'
  const ok = await Confirm(
    quit ? 'Do you want to terminate running processes and quit?' : 'Do you want to terminate running processes in this window?',
    `${quit ? 'Quitting jamalterm' : 'Closing this window'} will terminate: ${runningList(e.busy)}.`,
    quit ? 'Terminate and Quit' : 'Terminate',
  )
  if (!ok) return active?.term.focus()
  if (quit) {
    await saveSessionNow()
    void QuitNow()
  } else void CloseWindowNow()
})

// A password prompt: typing is protected (Secure Keyboard Entry); show a lock.
const secureBadge = el('div', { id: 'secure-badge', hidden: '', role: 'status' }, '🔒 Typing is protected')
document.body.append(secureBadge)
EventsOn('secure:password', (e: { id: number; on: boolean }) => {
  const t = tabs.find((x) => x.id === e.id)
  if (!t) return
  t.passwordMode = e.on
  renderTabs()
  secureBadge.hidden = !(active?.passwordMode)
})
EventsOn('secure:input', (on: boolean) => {
  secureBadge.textContent = on ? '🔒 Typing is protected' : '🔒 Password prompt'
})

EventsOn('settings:changed', (s: Prefs) => applySettings(s))
// Files dropped on the window: their paths go on the prompt, quoted.
EventsOn('files:dropped', (files: string[]) => {
  if (!active || !files?.length) return
  active.input(files.map(quotePath).join(' ') + ' ')
  active.term.focus()
})
// Hovering a theme in Settings shows it here for a moment; "" = back to yours.
EventsOn('theme:preview', (id: string) => {
  if (!cfg) return
  const theme = themeById(id || cfg.theme)
  applyChrome(theme, cfg.opacity / 100, cfg.windowFrame === 'theme' ? 'theme' : 'system')
  for (const t of tabs) t.applySettings({ ...cfg, theme: theme.id }, theme)
})

window.addEventListener('resize', () => requestAnimationFrame(() => active?.refit()))
document.getElementById('ai-button')!.addEventListener('click', () => runMenu('aiPanel'))

// ---- Start --------------------------------------------------------------------

/** When each part of starting up finished (ms since the page began loading). */
export const bootTimes: Record<string, number> = {}

async function boot() {
  // Re-colour the window frame when macOS switches light/dark.
  applySystemLook(() => cfg && applySettings(cfg))
  // Tab numbers are unique across windows.
  nextId = (await WindowID()) * 100000 + 1
  const mark = (name: string) => (bootTimes[name] = Math.round(performance.now()))
  ;[cfg, shellName, home] = await Promise.all([Settings(), ShellName(), HomeDir(), loadProviders(), loadCommands()]) as [Prefs, string, string, void, void]
  applySettings(cfg)
  setGpuOff(await GPUOff())
  mark('settings')
  // Wait for the text font so the character grid is measured correctly.
  await Promise.all([
    document.fonts.load(`${cfg.fontSize}px "JetBrains Mono"`),
    document.fonts.load(`bold ${cfg.fontSize}px "JetBrains Mono"`),
  ]).catch(() => undefined)
  mark('fonts')
  // The icon font (Nerd Font symbols) is big: don't wait for it. When it
  // arrives, redraw so icons drawn meanwhile get the right glyphs.
  void document.fonts.load(`${cfg.fontSize}px "Symbols Nerd Font Mono"`, '\ue0b0\uf07b').then(() => {
    mark('iconFont')
    for (const t of tabs) t.redrawGlyphs()
  }).catch(() => undefined)
  const resumed = (await reattach()) || (await restoreSession())
  if (!resumed) newTab(await StartDir())
  mark('tabs')
  const focus = () => active?.term.focus()
  void showLastCrash(() => void showDiagnostics(focus))
  // A first start gets the welcome card; an update gets What's new.
  if (resumed || !(await showWelcome(focus))) void showWhatsNew()
  if (await SmokeMode()) {
    void runSmoke({
      bootTimes,
      active: () => active,
      tabCount: () => groups.length,
      paneCount: () => groupOf(active)?.count ?? 0,
      run: (cmd) => runMenu(cmd),
    })
    return
  }
  void updater.startAuto(() => cfg.autoUpdate)
}

logWindowErrors()
void boot()
