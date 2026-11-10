import { Terminal, type IMarker } from '@xterm/xterm'
import { KittyKeyboard } from './kitty'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { SearchAddon } from '@xterm/addon-search'
import { SerializeAddon } from '@xterm/addon-serialize'
import { ClipboardAddon, type ClipboardSelectionType } from '@xterm/addon-clipboard'
import { ImageAddon } from '@xterm/addon-image'

import { Start, Attach, Write, Resize, CloseTab, OpenURL, GitBranch, Ack, ResolvePath, OpenPath, QuickLook, GitRepo } from './backend'
import { type Theme } from './themes'
import { shellQuote, once, fontStack, termTheme, guessCommand, sshTarget } from './termtext'
import { renderOverlay, dotLine } from './overlay'
export { fontStack, guessCommand, sshTarget, hostMatches } from './termtext'
import { b64ToBytes, b64ToText, el, escapeHtml, toast, copyText, duration, relTime } from './util'
import type { Settings } from './types'

export interface SavedTab {
  /** The tab's number, so a reloaded window can reattach to its shell. */
  id?: number
  cwd: string
  shareOutput?: boolean
  output?: string
}

/** One command the user ran, found through the shell integration marks. */
export interface Block {
  id: number
  prompt: IMarker // first line of the prompt (OSC 133;A)
  start?: IMarker // first line of output (OSC 133;C)
  end?: IMarker // line after the output, where the next prompt starts (OSC 133;D)
  command: string
  exitCode?: number
  cwd: string
  startedAt?: number
  finishedAt?: number
}

export interface TabHooks {
  onChange(tab: Tab): void // title or state changed
  onExplain(tab: Tab, block: Block): void // "ask AI about this block"
  onFix?(tab: Tab, block: Block): void // "Fix it": a corrected command for a failed one
  onCommandDone(tab: Tab, block: Block): void // for long-command notifications
  settings(): Settings
  shellName(): string
  home(): string
  /** Something was typed in this tab (for broadcasting to other panes). */
  onInput?(tab: Tab, data: string): void
}

interface BrowseState {
  active: boolean
  row: number // absolute buffer line
  col: number
  confirm: Block | null // waiting for a second Enter
}

// JAMALTERM_NO_GPU=1 turns off WebGL drawing (for odd graphics drivers).
let gpuOff = false
export function setGpuOff(off: boolean) {
  gpuOff = off
}

/** Browse mode's key bar: the short list, or every key (? toggles). */
let allKeys = false

export class Tab {
  readonly term: Terminal
  readonly fit = new FitAddon()
  readonly search = new SearchAddon()
  private readonly serializer = new SerializeAddon()
  readonly pane: HTMLDivElement
  readonly host: HTMLDivElement
  readonly overlay: HTMLDivElement
  /** Our own cursor for Settings → Cursor effect ("smooth" glides, "glow"
   * glows); xterm's is made invisible while it's on. */
  private fxCursor: HTMLDivElement
  private cursorEffect = 'none'

  running = false // shell process alive
  private starting = false
  exited = false
  /** The git repository this tab is in ("" outside one), for its color. */
  repo = ''
  cwd: string
  oscTitle = ''
  blocks: Block[] = []
  private pending: Block | null = null // prompt shown, nothing run yet
  integration = false
  private nextBlockId = 1
  private lastCols = 0
  private lastRows = 0
  private overlayKey = ''
  private overlayFrame = 0
  private keyConsumed = false
  private typedSincePrompt = false
  /** Off: this tab's output is never sent to the AI (only the commands). */
  shareOutput = true
  browse: BrowseState = { active: false, row: 0, col: 0, confirm: null }

  constructor(readonly id: number, parent: HTMLElement, cwd: string, private hooks: TabHooks, theme: Theme) {
    this.cwd = cwd
    this.host = el('div', { class: 'term-host' })
    this.overlay = el('div', { class: 'overlay' })
    this.fxCursor = el('div', { class: 'fx-cursor', hidden: '' })
    this.pane = el('div', { class: 'pane' }, this.host, this.overlay, this.fxCursor)
    parent.append(this.pane)

    const s = hooks.settings()
    this.term = new Terminal({
      allowProposedApi: true,
      allowTransparency: true,
      screenReaderMode: s.screenReader, // see-through window (Settings → Window opacity)
      fontFamily: fontStack(s.fontFamily),
      fontSize: s.fontSize,
      lineHeight: s.lineHeight,
      theme: this.cursorTheme(s, theme),
      cursorBlink: s.cursorBlink,
      cursorStyle: s.cursorStyle as 'block' | 'bar' | 'underline',
      cursorInactiveStyle: 'outline',
      macOptionIsMeta: s.optionIsMeta,
      macOptionClickForcesSelection: true,
      scrollback: s.scrollback,
      drawBoldTextInBrightColors: true,
      // Links programs print with OSC 8 (ls --hyperlink, gcc, git…): ⌘-click.
      linkHandler: {
        allowNonHttpProtocols: true,
        activate: (e, uri) => this.openLink(e, uri),
        hover: () => undefined,
      },
    })
    this.kitty = new KittyKeyboard(this.term, (s) => Write(this.id, s))
    this.term.loadAddon(this.fit)
    this.term.loadAddon(this.search)
    this.term.loadAddon(this.serializer)
    // OSC 52: programs (also over SSH / tmux) may copy to the clipboard,
    // but never read it.
    this.term.loadAddon(new ClipboardAddon(undefined, {
      readText: (_sel: ClipboardSelectionType) => '',
      writeText: (_sel: ClipboardSelectionType, text: string) => copyText(text),
    }))
    // ⌘-click a file path in the output to open it.
    this.term.registerLinkProvider({ provideLinks: (line, cb) => void this.pathLinks(line).then(cb) })
    this.term.loadAddon(new WebLinksAddon((e, url) => {
      if (e.metaKey || e.ctrlKey || !this.browse.active) OpenURL(url)
    }))
    const unicode = new Unicode11Addon()
    this.term.loadAddon(unicode)
    this.term.unicode.activeVersion = '11'
    this.term.open(this.host)
    try {
      // Pictures in the terminal (Sixel and iTerm2's image protocol).
      this.term.loadAddon(new ImageAddon({ sixelSizeLimit: 25_000_000 }))
    } catch {
      /* not essential */
    }
    this.setRenderer(s.ligatures)

    this.registerShellMarks()

    this.term.onData((data) => {
      if (this.exited) {
        void this.start()
        return
      }
      if (this.running) this.input(data)
    })
    this.term.onTitleChange((t) => {
      this.oscTitle = t
      hooks.onChange(this)
    })
    this.term.onRender(() => { this.scheduleOverlay(); this.placeFxCursor() })
    this.term.onCursorMove(() => this.placeFxCursor())
    this.term.onScroll(() => this.scheduleOverlay())
    this.term.attachCustomKeyEventHandler((e) => this.onKey(e))
    this.setupMouse()

    new ResizeObserver(() => requestAnimationFrame(() => this.refit())).observe(this.host)
  }

  // ---- ⌘-click file paths --------------------------------------------------------

  private static pathRe = /(?:~|\.{1,2})?\/?(?:[\w.@+-]+\/)+[\w.@+-]*|[\w@+-]+\.[A-Za-z][A-Za-z0-9]{0,7}\b/g

  private async pathLinks(lineNumber: number) {
    const line = this.term.buffer.active.getLine(lineNumber - 1)?.translateToString(true) ?? ''
    if (!line.trim()) return undefined
    const found: { start: number; text: string }[] = []
    for (const m of line.matchAll(Tab.pathRe)) {
      const before = line[m.index! - 1] ?? ''
      if (before === ':' || before === '/' || m[0].length < 3) continue // URLs are handled elsewhere
      found.push({ start: m.index!, text: m[0].replace(/[.,:;)]+$/, '') })
      if (found.length >= 20) break
    }
    const links = []
    for (const f of found) {
      const resolved = await ResolvePath(this.cwd, f.text)
      if (!resolved) continue
      links.push({
        range: { start: { x: f.start + 1, y: lineNumber }, end: { x: f.start + f.text.length, y: lineNumber } },
        text: f.text,
        decorations: { underline: true, pointerCursor: true },
        activate: (e: MouseEvent) => {
          if (e.metaKey || e.ctrlKey) void OpenPath(resolved)
          else toast('⌘-click to open ' + f.text)
        },
      })
    }
    return links.length ? links : undefined
  }

  // ---- Session: saved across restarts ---------------------------------------

  /** What's saved for this tab: folder, sharing switch and recent output. */
  // Serializing the scrollback is the expensive part of saving, so it's
  // redone only after new output.
  private snapDirty = true
  private snapOutput = ''

  snapshot(): SavedTab {
    if (this.snapDirty) {
      let output = ''
      try {
        output = this.serializer.serialize({ scrollback: 1000, excludeAltBuffer: true, excludeModes: true })
        if (output.length > 400_000) output = this.serializer.serialize({ scrollback: 200, excludeAltBuffer: true, excludeModes: true })
      } catch {
        /* nothing to save */
      }
      this.snapOutput = output
      this.snapDirty = false
    }
    return { id: this.id, cwd: this.cwd, shareOutput: this.shareOutput, output: this.snapOutput }
  }

  /** The program in front is asking for a password (typing is protected). */
  passwordMode = false

  /** Shows a saved tab's output (dimmed divider after it) before the shell starts. */
  restore(saved: SavedTab) {
    this.shareOutput = saved.shareOutput ?? true
    if (saved.output) {
      this.term.write(saved.output)
      this.term.write('\r\n\x1b[0m\x1b[2m──── restored from your last session ────\x1b[0m\r\n')
    }
  }

  // ---- Process ---------------------------------------------------------------

  async start() {
    if (this.starting) return
    this.starting = true
    try {
      this.refit()
      if (this.exited) {
        this.term.reset()
        this.disposeBlocks()
      }
      this.exited = false
      this.kitty.reset()
      await Start(this.id, this.term.cols, this.term.rows, this.cwd)
      this.running = true
      Resize(this.id, this.term.cols, this.term.rows)
    } catch (err) {
      this.term.write(`\x1b[31mCould not start the shell: ${err}\x1b[0m\r\n`)
      this.exited = true
    } finally {
      this.starting = false
      this.hooks.onChange(this)
    }
  }

  /** Reconnects to a shell that kept running while the window reloaded. */
  async attach() {
    this.starting = true
    try {
      const b64 = await Attach(this.id)
      this.term.reset()
      const bytes = b64ToBytes(b64)
      this.term.write(bytes)
      this.running = true
      this.exited = false
      this.refit()
      Resize(this.id, this.term.cols, this.term.rows)
    } catch {
      this.term.write('\x1b[2m[this tab\'s shell ended while the window reloaded]\x1b[0m\r\n')
      void this.start()
    } finally {
      this.starting = false
      this.hooks.onChange(this)
    }
  }

  write(b64: string) {
    const bytes = b64ToBytes(b64)
    this.snapDirty = true
    // Tell Go once this batch is drawn, so it can send more (backpressure).
    this.term.write(bytes, () => Ack(this.id, bytes.length))
  }

  onExit(code: number) {
    this.running = false
    this.exited = true
    this.exitBrowse()
    this.term.write(`\r\n\x1b[2m[process exited with code ${code} — press any key for a new shell, ⌘W to close]\x1b[0m\r\n`)
    this.hooks.onChange(this)
  }

  close() {
    CloseTab(this.id)
    this.term.dispose()
    this.pane.remove()
  }

  refit() {
    if (!this.pane.isConnected || this.pane.offsetParent === null) return
    try {
      this.fit.fit()
    } catch {
      return
    }
    if (this.term.cols !== this.lastCols || this.term.rows !== this.lastRows) {
      this.lastCols = this.term.cols
      this.lastRows = this.term.rows
      if (this.running) Resize(this.id, this.term.cols, this.term.rows)
      // Repaint everything: the GPU renderer can come back blank after a
      // resize while the terminal doesn't have focus.
      requestAnimationFrame(() => this.term.refresh(0, this.term.rows - 1))
    }
    this.scheduleOverlay()
  }

  // ---- Input -------------------------------------------------------------------

  readonly kitty: KittyKeyboard

  /** Sends typing to the shell (and, when broadcasting, to the other panes). */
  input(data: string, broadcast = true) {
    this.typedSincePrompt = true
    Write(this.id, data)
    if (broadcast) this.hooks.onInput?.(this, data)
  }

  /** An OSC 8 link: web pages in the browser, files in their app. */
  private openLink(e: MouseEvent, uri: string) {
    if (!(e.metaKey || e.ctrlKey)) {
      toast('⌘-click to open ' + (uri.length > 60 ? uri.slice(0, 57) + '…' : uri))
      return
    }
    let u: URL
    try {
      u = new URL(uri)
    } catch {
      return
    }
    if (u.protocol === 'http:' || u.protocol === 'https:' || u.protocol === 'mailto:') OpenURL(uri)
    else if (u.protocol === 'file:') {
      // Only files on this computer.
      if (u.hostname && u.hostname !== 'localhost' && !this.isLocalHost(u.hostname)) return void toast('That file is on ' + u.hostname)
      void OpenPath(decodeURIComponent(u.pathname))
    } else toast(`jamalterm doesn't open ${u.protocol} links`)
  }

  // ---- Drawing: GPU, or the DOM renderer for ligatures --------------------------

  private webgl: WebglAddon | null = null

  private visible = true

  /** Shown or hidden with its tab: only visible panes use the GPU. */
  setVisible(on: boolean) {
    if (this.visible === on) return
    this.visible = on
    this.setRenderer(this.hooks.settings().ligatures)
    if (on) this.redrawGlyphs()
  }

  private setRenderer(ligatures: boolean) {
    this.pane.classList.toggle('ligatures', ligatures)
    const wantGpu = !gpuOff && !ligatures && this.visible
    if (wantGpu && !this.webgl) {
      try {
        const webgl = new WebglAddon()
        webgl.onContextLoss(() => {
          webgl.dispose()
          this.webgl = null
        })
        this.term.loadAddon(webgl)
        this.webgl = webgl
      } catch {
        /* the DOM renderer is fine */
      }
    } else if (!wantGpu && this.webgl) {
      this.webgl.dispose()
      this.webgl = null
    }
  }

  /** A font finished loading: draw the glyphs again. */
  redrawGlyphs() {
    try {
      this.term.clearTextureAtlas()
    } catch {
      /* DOM renderer: nothing cached */
    }
    this.term.refresh(0, this.term.rows - 1)
  }

  // ---- Which machine this tab is on ---------------------------------------------

  /** The host from the first OSC 7 (this Mac), and the latest one. */
  private localHost = ''
  private oscHost = ''

  private isLocalHost(h: string) {
    return !!this.localHost && h.toLowerCase() === this.localHost.toLowerCase()
  }

  /** The other machine this tab is connected to ("" when it's this Mac). */
  get remoteHost(): string {
    if (this.oscHost && !this.isLocalHost(this.oscHost)) return this.oscHost
    for (let i = this.blocks.length - 1; i >= 0; i--) {
      const b = this.blocks[i]
      if (b.start && !b.end) {
        const h = sshTarget(b.command)
        if (h) return h
      }
    }
    return ''
  }

  applySettings(s: Settings, theme: Theme) {
    this.setRenderer(s.ligatures)
    const o = this.term.options
    o.theme = this.cursorTheme(s, theme)
    o.fontFamily = fontStack(s.fontFamily)
    o.fontSize = s.fontSize
    o.lineHeight = s.lineHeight
    o.cursorBlink = s.cursorBlink
    o.cursorStyle = s.cursorStyle as 'block' | 'bar' | 'underline'
    o.scrollback = s.scrollback
    o.macOptionIsMeta = s.optionIsMeta
    o.screenReaderMode = s.screenReader
    this.refit()
    this.overlayKey = ''
    this.scheduleOverlay()
  }

  /** The terminal colors, with xterm's cursor made invisible when a cursor
   * effect is on (ours is drawn instead). */
  private cursorTheme(s: Settings, theme: Theme) {
    this.cursorEffect = s.cursorEffect || 'none'
    const fx = this.cursorEffect !== 'none'
    this.fxCursor.hidden = !fx
    this.fxCursor.className = `fx-cursor ${this.cursorEffect} ${s.cursorStyle}${s.cursorBlink ? ' blink' : ''}`
    this.fxCursor.style.setProperty('--fx', theme.colors.cursor ?? '#fff')
    requestAnimationFrame(() => this.placeFxCursor())
    return fx ? { ...termTheme(theme, s.opacity), cursor: 'rgba(0,0,0,0)', cursorAccent: theme.colors.foreground } : termTheme(theme, s.opacity)
  }

  /** Moves our cursor to xterm's (only when a cursor effect is on). */
  private placeFxCursor() {
    if (this.cursorEffect === 'none') return
    const g = this.geometry()
    const buf = this.term.buffer.active
    const row = buf.baseY + buf.cursorY - buf.viewportY
    if (!g || row < 0 || row >= this.term.rows) {
      this.fxCursor.style.opacity = '0'
      return
    }
    this.fxCursor.style.opacity = ''
    this.fxCursor.style.width = `${g.cw}px`
    this.fxCursor.style.height = `${g.ch}px`
    this.fxCursor.style.transform = `translate(${g.left + buf.cursorX * g.cw}px, ${g.top + row * g.ch}px)`
    this.fxCursor.classList.toggle('unfocused', !this.pane.contains(document.activeElement))
  }

  setFontSize(size: number) {
    this.term.options.fontSize = Math.max(8, Math.min(48, size))
    this.refit()
  }

  // ---- Title -----------------------------------------------------------------

  get title(): string {
    if (this.exited) return 'exited'
    const run = this.runningBlock
    if (run && run.command) {
      const word = run.command.trim().split(/\s+/)[0] ?? ''
      return word.split('/').pop() || this.hooks.shellName()
    }
    if (this.cwd) {
      const home = this.hooks.home()
      if (this.cwd === home) return 'Home' // words, not "~"
      return this.cwd.split('/').filter(Boolean).pop() ?? '/'
    }
    return this.oscTitle || this.hooks.shellName()
  }

  shortPath(p: string): string {
    const home = this.hooks.home()
    if (home && p === home) return '~'
    return home && p.startsWith(home + '/') ? '~' + p.slice(home.length) : p
  }

  get prettyCwd(): string {
    const home = this.hooks.home()
    if (!this.cwd) return ''
    if (home && this.cwd === home) return '~'
    return home && this.cwd.startsWith(home + '/') ? '~' + this.cwd.slice(home.length) : this.cwd
  }

  /** The folder for a tab label: "~/code/app", or "~/…/src/app" when long. */
  get shortCwd(): string {
    const p = this.prettyCwd
    if (p === '~') return 'Home'
    if (p.length <= 28) return p
    const parts = p.split('/')
    const head = parts[0] === '~' ? '~' : ''
    return `${head}/…/${parts.slice(-2).join('/')}`
  }

  /** Git branch of the current folder ("" if not a repo). */
  branch = ''
  private branchFor = ''

  refreshBranch(force = false) {
    const dir = this.cwd
    if (!dir || (!force && dir === this.branchFor)) return
    this.branchFor = dir
    GitBranch(dir).then((b) => {
      if (dir === this.cwd && b !== this.branch) {
        this.branch = b
        this.hooks.onChange(this)
      }
    })
    GitRepo(dir).then((r) => {
      if (dir === this.cwd && r !== this.repo) {
        this.repo = r
        this.hooks.onChange(this)
      }
    }).catch(() => undefined)
  }

  /** What the tab shows: running, failed (last command), exited, or idle. */
  get status(): 'running' | 'failed' | 'exited' | 'idle' {
    if (this.exited) return 'exited'
    if (this.runningBlock) return 'running'
    const last = this.lastFinished
    return last && last.exitCode !== 0 ? 'failed' : 'idle'
  }

  /** Name of the running program, e.g. "npm" for "npm run dev". */
  get runningName(): string {
    const cmd = this.runningBlock?.command.trim() ?? ''
    if (!cmd) return ''
    const words = cmd.split(/\s+/)
    const first = words[0] === 'sudo' && words[1] ? words[1] : words[0]
    return first.split('/').pop() ?? first
  }

  // ---- Shell integration ----------------------------------------------------

  private registerShellMarks() {
    const parser = this.term.parser
    parser.registerOscHandler(133, (data) => {
      this.integration = true
      const [kind, arg] = data.split(';')
      if (kind === 'A') this.markPrompt()
      else if (kind === 'C') this.markStart()
      else if (kind === 'D') this.markDone(arg === undefined ? undefined : parseInt(arg, 10))
      return true
    })
    parser.registerOscHandler(6973, (data) => {
      if (data.startsWith('cmd;')) {
        const cmd = b64ToText(data.slice(4)).replace(/\n+$/, '')
        if (!this.pending) this.markPrompt()
        this.pending!.command = cmd
      }
      return true
    })
    parser.registerOscHandler(7, (data) => {
      const host = /^file:\/\/([^/]*)\//.exec(data)?.[1] ?? ''
      if (host) {
        if (!this.localHost) this.localHost = host
        if (host !== this.oscHost) {
          this.oscHost = host
          this.hooks.onChange(this)
        }
      }
      const m = /^file:\/\/[^/]*(\/.*)$/.exec(data)
      if (m) {
        let path = m[1]
        try {
          path = decodeURIComponent(path)
        } catch {
          /* already plain */
        }
        if (path !== this.cwd) {
          this.cwd = path
          this.hooks.onChange(this)
        }
        this.refreshBranch()
      }
      return true
    })
  }

  private promptWaiters: (() => void)[] = []

  /** Resolves once the shell shows its first prompt (or after 4 seconds). */
  whenReady(): Promise<void> {
    if (this.pending || this.blocks.length) return Promise.resolve()
    return new Promise((resolve) => {
      this.promptWaiters.push(resolve)
      setTimeout(resolve, 4000)
    })
  }

  private markPrompt() {
    for (const w of this.promptWaiters.splice(0)) setTimeout(w, 50)
    if (this.pending) this.pending.prompt.dispose() // empty Enter or Ctrl+C at the prompt
    const marker = this.term.registerMarker(0)
    this.typedSincePrompt = false
    if (!marker) return
    this.pending = { id: this.nextBlockId++, prompt: marker, command: '', cwd: this.cwd }
  }

  private markStart() {
    if (!this.pending && this.runningBlock) return // already started (shells like fish 4 mark it too)
    if (!this.pending) this.markPrompt()
    const b = this.pending
    if (!b) return
    this.pending = null
    b.start = this.term.registerMarker(0)
    b.startedAt = Date.now()
    b.cwd = this.cwd
    this.blocks.push(b)
    this.hooks.onChange(this)
    this.scheduleOverlay()
  }

  private markDone(code?: number) {
    // The most recent command still running. Over ssh with browse mode on
    // the server, the server's commands run inside the local "ssh" one;
    // each "done" closes the innermost.
    let b: Block | undefined
    for (let i = this.blocks.length - 1; i >= 0; i--) {
      if (this.blocks[i].start && !this.blocks[i].end) { b = this.blocks[i]; break }
    }
    if (!b) return
    b.end = this.term.registerMarker(0)
    b.exitCode = code ?? 0
    b.finishedAt = Date.now()
    this.hooks.onCommandDone(this, b)
    if (/\bgit\b/.test(b.command)) this.refreshBranch(true) // checkout, switch…
    this.hooks.onChange(this)
    this.scheduleOverlay()
  }

  private disposeBlocks() {
    for (const b of this.blocks) {
      b.prompt.dispose()
      b.start?.dispose()
      b.end?.dispose()
    }
    this.blocks = []
    this.pending?.prompt.dispose()
    this.pending = null
  }

  /** Blocks still in the scrollback (Cmd+K and scrolling drop old ones). */
  liveBlocks(): Block[] {
    this.blocks = this.blocks.filter((b) => !b.prompt.isDisposed && b.prompt.line >= 0)
    return this.blocks
  }

  get runningBlock(): Block | undefined {
    const b = this.blocks[this.blocks.length - 1]
    return b && b.start && !b.end ? b : undefined
  }

  get lastFinished(): Block | undefined {
    const live = this.liveBlocks()
    for (let i = live.length - 1; i >= 0; i--) if (live[i].end) return live[i]
    return undefined
  }

  get lastFailed(): Block | undefined {
    const live = this.liveBlocks()
    for (let i = live.length - 1; i >= 0; i--) if (live[i].end && live[i].exitCode !== 0) return live[i]
    return undefined
  }

  /** Last line of a block (inclusive). */
  blockEnd(b: Block): number {
    if (b.end && !b.end.isDisposed) return Math.max(b.prompt.line, b.end.line - 1)
    const buf = this.term.buffer.active
    return buf.baseY + buf.cursorY
  }

  blockAt(line: number): Block | undefined {
    for (const b of this.liveBlocks()) {
      if (line >= b.prompt.line && line <= this.blockEnd(b)) return b
    }
    return undefined
  }

  /** The word (path-like, no spaces) at a position. */
  wordAt(line: number, col: number): string {
    const text = this.lineText(line)
    if (!text[col] || text[col] === ' ') return ''
    let a = col, b = col
    while (a > 0 && text[a - 1] !== ' ') a--
    while (b < text.length - 1 && text[b + 1] !== ' ') b++
    return text.slice(a, b + 1)
  }

  lineText(line: number): string {
    return this.term.buffer.active.getLine(line)?.translateToString(true) ?? ''
  }

  /** Text between two lines, joining lines that were wrapped by the terminal. */
  textBetween(from: number, to: number): string {
    const buf = this.term.buffer.active
    const out: string[] = []
    for (let i = from; i <= to; i++) {
      const line = buf.getLine(i)
      if (!line) continue
      const text = line.translateToString(true)
      if (line.isWrapped && out.length) out[out.length - 1] += text
      else out.push(text)
    }
    while (out.length && out[out.length - 1].trim() === '') out.pop()
    return out.join('\n')
  }

  blockOutput(b: Block): string {
    if (!b.start || b.start.isDisposed) return ''
    return this.textBetween(b.start.line, this.blockEnd(b))
  }

  /** Recent commands and output, sent to Claude with a question. */
  aiContext(focus?: Block): string {
    const s = this.hooks.settings()
    const parts: string[] = [`Working directory: ${this.prettyCwd || 'unknown'}`]
    const clip = (text: string, max: number) => {
      if (text.length <= max) return text
      return '[…earlier output cut…]\n' + text.slice(text.length - max)
    }
    const blocks = this.liveBlocks().filter((b) => b.command && b !== focus).slice(-6)
    if (focus) blocks.push(focus)
    if (blocks.length) {
      parts.push('Recent commands, oldest first:')
      for (const b of blocks) {
        const status = b.end ? `exit ${b.exitCode}` : 'still running'
        parts.push(`$ ${b.command}    [${status}]`)
        if (s.aiShareOutput && this.shareOutput) {
          const out = this.blockOutput(b)
          if (out) parts.push(clip(out, b === focus ? 12000 : 2500))
        }
      }
    } else if (s.aiShareOutput && this.shareOutput) {
      const buf = this.term.buffer.active
      const end = buf.baseY + buf.cursorY
      parts.push('Screen:', clip(this.textBetween(Math.max(0, end - 120), end), 8000))
    }
    return parts.join('\n')
  }

  // ---- Sending commands -------------------------------------------------------

  /** Puts a command on the prompt line, replacing what was typed, and optionally runs it. */
  sendCommand(command: string, run: boolean): boolean {
    if (this.exited || !this.running) {
      toast('The shell has exited — press any key to start a new one', 'error')
      return false
    }
    if (this.term.buffer.active.type === 'alternate') {
      toast('A full-screen program is open — quit it first', 'error')
      return false
    }
    if (this.runningBlock) {
      toast('A command is still running — wait for it or press Ctrl+C', 'error')
      return false
    }
    const text = command.replace(/\r?\n+$/, '')
    let payload = text
    if (text.includes('\n')) {
      payload = this.term.modes.bracketedPasteMode ? `\x1b[200~${text}\x1b[201~` : text.replace(/\n/g, ' ')
    }
    // Ctrl+E Ctrl+U: go to the end of the line and clear it (zsh, bash, tcsh).
    // Clear what's on the prompt first: Ctrl+E Ctrl+U (emacs keys, the
    // default in zsh, bash, tcsh) or Esc + cc (vi keys).
    const clear = this.hooks.settings().promptKeys === 'vi' ? '\x1bcc' : '\x05\x15'
    Write(this.id, clear + payload + (run ? '\r' : ''))
    this.term.scrollToBottom()
    this.term.focus()
    return true
  }

  /** Runs a command and waits up to 30 s for it to finish (for Claude's tool). */
  async runAndWait(command: string): Promise<{ started: boolean; exitCode?: number; summary: string }> {
    const before = this.blocks.length
    if (!this.sendCommand(command, true)) return { started: false, summary: '' }
    if (!this.integration) return { started: true, exitCode: 0, summary: 'Ran it. (No result is available: shell integration is off.)' }
    const deadline = Date.now() + 30000
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 200))
      const b = this.blocks.slice(before).find((x) => x.end)
      if (b) {
        let out = this.blockOutput(b)
        if (out.length > 3000) out = '[…]\n' + out.slice(-3000)
        return { started: true, exitCode: b.exitCode, summary: `Ran it. Exit code ${b.exitCode}.\nOutput:\n${out || '(none)'}` }
      }
    }
    return { started: true, summary: 'Started it; it is still running after 30 seconds (output not shown yet).' }
  }

  // ---- Browse mode: the free-floating cursor ----------------------------------

  enterBrowse(row?: number, col = 0) {
    if (this.term.buffer.active.type === 'alternate') {
      toast('Browse mode is off while a full-screen program is open')
      return
    }
    const buf = this.term.buffer.active
    if (row === undefined) {
      const last = this.lastFinished ?? this.liveBlocks()[this.liveBlocks().length - 1]
      row = last ? dotLine(this, last) : Math.max(0, buf.baseY + buf.cursorY - 1)
    }
    this.browse = { active: true, row: this.clampRow(row), col: Math.max(0, col), confirm: null }
    this.term.clearSelection()
    this.revealRow()
    this.updateBrowse()
  }

  exitBrowse() {
    if (!this.browse.active) return
    this.browse = { active: false, row: 0, col: 0, confirm: null }
    this.term.scrollToBottom()
    this.updateBrowse()
    this.term.focus()
  }

  private clampRow(row: number) {
    const buf = this.term.buffer.active
    return Math.max(0, Math.min(row, buf.baseY + buf.cursorY))
  }

  private revealRow() {
    const vy = this.term.buffer.active.viewportY
    const rows = this.term.rows
    const r = this.browse.row
    if (r < vy) this.term.scrollToLine(r)
    else if (r >= vy + rows) this.term.scrollToLine(r - rows + 1)
  }

  private moveTo(row: number, col?: number) {
    this.browse.row = this.clampRow(row)
    if (col !== undefined) this.browse.col = Math.max(0, Math.min(col, this.term.cols - 1))
    this.browse.confirm = null
    this.revealRow()
    this.updateBrowse()
  }

  jumpBlock(dir: -1 | 1) {
    const live = this.liveBlocks()
    if (!this.browse.active) {
      this.enterBrowse()
      return
    }
    const cur = this.blockAt(this.browse.row)
    const idx = cur ? live.indexOf(cur) : -1
    let target: Block | undefined
    if (dir < 0) {
      if (cur && this.browse.row > dotLine(this, cur)) target = cur
      else target = idx > 0 ? live[idx - 1] : idx === -1 ? [...live].reverse().find((b) => b.prompt.line < this.browse.row) : undefined
    } else {
      target = idx >= 0 ? live[idx + 1] : live.find((b) => b.prompt.line > this.browse.row)
    }
    if (target) this.moveTo(dotLine(this, target), 0)
    else if (dir > 0) this.exitBrowse()
  }

  private browseKey(e: KeyboardEvent): boolean {
    const b = this.browse
    const page = Math.max(1, this.term.rows - 2)
    const jump = e.altKey || e.shiftKey
    switch (e.key) {
      case 'ArrowUp':
        if (e.metaKey) this.jumpBlock(-1)
        else if (jump) this.jumpBlock(-1)
        else this.moveTo(b.row - 1)
        return true
      case 'ArrowDown':
        if (e.metaKey || jump) this.jumpBlock(1)
        else if (b.row >= this.clampRow(Infinity)) this.exitBrowse()
        else this.moveTo(b.row + 1)
        return true
      case 'ArrowLeft':
        this.moveTo(b.row, b.col - (jump ? 8 : 1))
        return true
      case 'ArrowRight':
        this.moveTo(b.row, b.col + (jump ? 8 : 1))
        return true
      case 'PageUp':
        this.moveTo(b.row - page)
        return true
      case 'PageDown':
        this.moveTo(b.row + page)
        return true
      case 'Home':
        this.moveTo(b.row, 0)
        return true
      case 'End':
        this.moveTo(b.row, Math.max(0, this.lineText(b.row).trimEnd().length - 1))
        return true
      case 'Enter':
        this.activate(e.shiftKey ? 'edit' : e.altKey ? 'there' : 'run')
        return true
      case 'Escape':
        this.exitBrowse()
        return true
    }
    if (e.metaKey || e.ctrlKey) {
      if (e.ctrlKey && !e.metaKey) this.exitBrowse() // Ctrl+C etc. go to the shell
      return false
    }
    const key = e.key.toLowerCase()
    const block = this.blockAt(b.row)
    switch (key) {
      case 'q':
        this.exitBrowse()
        return true
      case 'c':
        if (block?.command) {
          copyText(block.command)
          toast('Copied the command')
        } else {
          copyText(this.lineText(b.row).trim())
          toast('Copied the line')
        }
        return true
      case 'o':
        if (block) {
          copyText(this.blockOutput(block))
          toast('Copied the output')
        }
        return true
      case 'e':
        this.activate('edit')
        return true
      case 'a':
        if (block) this.hooks.onExplain(this, block)
        else toast('Move to a command first')
        return true
      case ' ': {
        // Space: Quick Look the file or folder under the cursor, as in Finder.
        const word = this.wordAt(b.row, b.col)
        if (word) QuickLook(this.cwd, word).catch(() => toast(`No file called “${word}” here`))
        return true
      }
      case 'f':
        if (block?.end && block.exitCode !== 0) (this.hooks.onFix ?? this.hooks.onExplain)(this, block)
        else toast(block ? 'That command didn’t fail' : 'Move to a command first')
        return true
      case '?':
        // The short key list, or all of them.
        allKeys = !allKeys
        this.updateBrowse()
        return true
      case 'k':
        this.moveTo(b.row - 1)
        return true
      case 'j':
        this.moveTo(b.row + 1)
        return true
    }
    if (e.key.length === 1) {
      // Typing goes straight back to the shell.
      this.exitBrowse()
      return false
    }
    return true
  }

  private activate(how: 'run' | 'edit' | 'there') {
    const block = this.blockAt(this.browse.row)
    if (!block || !block.command) {
      // No marks here (a server, or a shell without integration): if the
      // line looks like "… $ command", take the command after the prompt.
      // It goes on the prompt for you to check, never straight to running.
      const line = this.lineText(this.browse.row)
      const guess = guessCommand(line)
      if (how === 'edit' || guess) {
        const text = guess || line.trim()
        if (text && this.sendCommand(text, false)) {
          this.exitBrowse()
          if (guess && how !== 'edit') toast('Put on the prompt: press ⏎ to run it (no shell integration here, so jamalterm doesn\'t run it for you)')
        }
      } else {
        toast(this.integration ? 'No command on this line — move to one with ↑ ↓' : 'No command found on this line — ⇧⏎ copies the whole line to the prompt')
      }
      return
    }
    // A command run again from another folder can do something different
    // (rm, git reset…): say so and let the user choose where.
    const elsewhere = !!block.cwd && !!this.cwd && block.cwd !== this.cwd
    if (how === 'there' && elsewhere) {
      if (this.sendCommand(`cd ${shellQuote(block.cwd)} && ${block.command}`, true)) this.exitBrowse()
      return
    }
    const mustConfirm = elsewhere || this.hooks.settings().confirmRerun
    if (how !== 'edit' && mustConfirm && this.browse.confirm !== block) {
      this.browse.confirm = block
      this.updateBrowse()
      return
    }
    if (this.sendCommand(block.command, how !== 'edit')) this.exitBrowse()
  }

  private updateBrowse() {
    this.overlayKey = ''
    this.scheduleOverlay()
    this.hooks.onChange(this)
  }

  /** Text for the hint bar under the terminal in browse mode. */
  browseHint(): string {
    if (!this.browse.active) return ''
    const block = this.blockAt(this.browse.row)
    // The everyday keys, grouped; ? shows the rest.
    const keys = allKeys
      ? '<span class="keys">Move <kbd>↑</kbd><kbd>↓</kbd></span><span class="keys">Commands <kbd>⌥↑</kbd><kbd>⌥↓</kbd></span>' +
        '<span class="keys">Run <kbd>⏎</kbd></span><span class="keys">Edit <kbd>⇧⏎</kbd></span><span class="keys">Copy <kbd>C</kbd></span>' +
        '<span class="keys">Copy output <kbd>O</kbd></span><span class="keys">Ask AI <kbd>A</kbd></span><span class="keys">Fix it <kbd>F</kbd></span><span class="keys">Quick Look <kbd>Space</kbd></span><span class="keys">Back <kbd>esc</kbd></span>' +
        '<span class="keys">Fewer keys <kbd>?</kbd></span>'
      : '<span class="keys">Run <kbd>⏎</kbd></span><span class="keys">Edit <kbd>⇧⏎</kbd></span><span class="keys">Copy <kbd>C</kbd></span>' +
        '<span class="keys">Ask AI <kbd>A</kbd></span><span class="keys">Back <kbd>esc</kbd></span><span class="keys more">More keys <kbd>?</kbd></span>'
    if (this.browse.confirm) {
      const b = this.browse.confirm
      if (b.cwd && this.cwd && b.cwd !== this.cwd) {
        const there = this.shortPath(b.cwd), here = this.prettyCwd
        return `<span class="hint-cmd warn">⚠ <code>${escapeHtml(b.command)}</code> ran in <b>${escapeHtml(there)}</b>, you're in <b>${escapeHtml(here)}</b></span>` +
          `<kbd>⏎</kbd> run here <kbd>⌥⏎</kbd> run in ${escapeHtml(there)} <kbd>esc</kbd> cancel`
      }
      return `<span class="hint-cmd">Run <code>${escapeHtml(b.command)}</code> again?</span> <kbd>⏎</kbd> yes <kbd>esc</kbd> no`
    }
    if (block?.command) {
      const status = !block.end
        ? '<span class="st run">running</span>'
        : block.exitCode === 0
          ? '<span class="st ok">✓</span>'
          : `<span class="st bad">exit ${block.exitCode}</span>`
      const when = block.finishedAt ? ` · ${relTime(block.finishedAt)}` : ''
      const took = block.finishedAt && block.startedAt ? ` · took ${duration(block.finishedAt - block.startedAt)}` : ''
      return `<span class="hint-cmd">${status} <code>${escapeHtml(block.command)}</code><span class="dim">${when}${took}</span></span>${keys}`
    }
    return `<span class="hint-cmd dim">Line ${this.browse.row + 1}</span>${keys}`
  }

  // ---- Keys & mouse ----------------------------------------------------------

  /** Called by xterm.js for every key event. false = xterm ignores it. */
  private onKey(e: KeyboardEvent): boolean {
    if (e.type !== 'keydown') return !this.keyConsumed
    this.keyConsumed = false
    if (this.browse.active) {
      this.keyConsumed = this.browseKey(e)
      if (this.keyConsumed) {
        e.preventDefault()
        return false
      }
      return !e.metaKey
    }
    // ⌘ shortcuts belong to the menu bar, never to the shell.
    if (e.metaKey) return false
    // Programs that asked for the kitty keyboard protocol get unambiguous keys.
    const kitty = this.running ? this.kitty.encode(e, this.hooks.settings().optionIsMeta) : null
    if (kitty) {
      e.preventDefault()
      this.keyConsumed = true
      this.input(kitty)
      return false
    }
    if (e.key === 'ArrowUp' && !e.altKey && !e.ctrlKey && !e.shiftKey && this.atEmptyPrompt()) {
      // The free-floating cursor starts where the text cursor is.
      const buf = this.term.buffer.active
      this.keyConsumed = true
      e.preventDefault()
      this.enterBrowse(buf.baseY + buf.cursorY - 1, buf.cursorX)
      if (once('jt-up-tip')) toast('↑ browses earlier commands (Esc to go back). Ctrl+R still searches history; you can turn this off in Settings.')
      return false
    }
    return true
  }

  /** True when ↑ should start browse mode instead of going to the shell. */
  private atEmptyPrompt(): boolean {
    return (
      this.hooks.settings().arrowBrowse &&
      this.running &&
      !!this.pending &&
      !this.runningBlock &&
      !this.typedSincePrompt &&
      this.term.buffer.active.type === 'normal' &&
      this.term.modes.mouseTrackingMode === 'none'
    )
  }

  private setupMouse() {
    let down: { x: number; y: number } | null = null
    let timer = 0
    this.host.addEventListener('mousedown', (e) => {
      down = e.button === 0 ? { x: e.clientX, y: e.clientY } : null
    })
    this.host.addEventListener('dblclick', () => clearTimeout(timer))
    this.host.addEventListener('mouseup', (e) => {
      if (!down || e.button !== 0) return
      const moved = Math.abs(e.clientX - down.x) + Math.abs(e.clientY - down.y)
      down = null
      if (moved > 3 || e.shiftKey || e.altKey || e.metaKey) return
      if (this.term.modes.mouseTrackingMode !== 'none' || this.term.buffer.active.type === 'alternate') return
      const pos = this.cellAt(e.clientX, e.clientY)
      if (!pos) return
      clearTimeout(timer)
      timer = window.setTimeout(() => {
        if (this.term.hasSelection()) return
        // Clicking on the live prompt or running output means "type here".
        const buf = this.term.buffer.active
        const live = this.pending && !this.pending.prompt.isDisposed ? this.pending.prompt.line : this.runningBlock?.start?.line
        const cursorLine = buf.baseY + buf.cursorY
        if ((live !== undefined && pos.row >= live) || pos.row >= cursorLine) {
          this.exitBrowse()
          return
        }
        if (this.browse.active) this.moveTo(pos.row, pos.col)
        else this.enterBrowse(pos.row, pos.col)
      }, 230)
    })
  }

  geometry() {
    const screen = this.host.querySelector('.xterm-screen') as HTMLElement | null
    if (!screen) return null
    const sr = screen.getBoundingClientRect()
    const pr = this.pane.getBoundingClientRect()
    return { left: sr.left - pr.left, top: sr.top - pr.top, cw: sr.width / this.term.cols, ch: sr.height / this.term.rows, sr }
  }

  private cellAt(x: number, y: number) {
    const g = this.geometry()
    if (!g) return null
    const col = Math.floor((x - g.sr.left) / g.cw)
    const r = Math.floor((y - g.sr.top) / g.ch)
    if (r < 0 || r >= this.term.rows || col < 0) return null
    return { row: this.term.buffer.active.viewportY + r, col: Math.min(col, this.term.cols - 1) }
  }

  // ---- Overlay: gutter dots and the browse cursor (overlay.ts) ----------------

  scheduleOverlay() {
    if (this.overlayFrame) return
    this.overlayFrame = requestAnimationFrame(() => {
      this.overlayFrame = 0
      this.overlayKey = renderOverlay(this, this.overlayKey, {
        explain: (b) => this.hooks.onExplain(this, b),
        fix: (b) => (this.hooks.onFix ?? this.hooks.onExplain)(this, b),
      })
    })
  }
}
