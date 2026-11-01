import { Terminal, type IMarker } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { SearchAddon } from '@xterm/addon-search'

import { Start, Write, Resize, CloseTab, OpenURL } from '../wailsjs/go/main/App'
import type { settings } from '../wailsjs/go/models'
import type { Theme } from './themes'
import { b64ToBytes, b64ToText, el, escapeHtml, toast, copyText, duration, relTime } from './util'

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
  settings(): settings.Settings
  shellName(): string
  home(): string
}

/** Adds the bundled Nerd Font icons as a fallback to any font. */
export function fontStack(family: string): string {
  const sym = '"Symbols Nerd Font Mono"'
  if (family.includes(sym)) return family
  const parts = family.split(',').map((p) => p.trim()).filter(Boolean)
  const generic = parts.length && /^(monospace|ui-monospace)$/.test(parts[parts.length - 1]) ? parts.pop()! : 'monospace'
  return [...parts, sym, generic].join(', ')
}

function termTheme(theme: Theme) {
  return {
    ...theme.colors,
    scrollbarSliderBackground: theme.light ? '#00000022' : '#ffffff22',
    scrollbarSliderHoverBackground: theme.light ? '#00000044' : '#ffffff44',
    scrollbarSliderActiveBackground: theme.light ? '#00000055' : '#ffffff55',
  }
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

export class Tab {
  readonly term: Terminal
  readonly fit = new FitAddon()
  readonly search = new SearchAddon()
  readonly pane: HTMLDivElement
  readonly host: HTMLDivElement
  readonly overlay: HTMLDivElement

  running = false // shell process alive
  private starting = false
  exited = false
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
  browse: BrowseState = { active: false, row: 0, col: 0, confirm: null }

  constructor(readonly id: number, parent: HTMLElement, cwd: string, private hooks: TabHooks, theme: Theme) {
    this.cwd = cwd
    this.host = el('div', { class: 'term-host' })
    this.overlay = el('div', { class: 'overlay' })
    this.pane = el('div', { class: 'pane' }, this.host, this.overlay)
    parent.append(this.pane)

    const s = hooks.settings()
    this.term = new Terminal({
      allowProposedApi: true,
      fontFamily: fontStack(s.fontFamily),
      fontSize: s.fontSize,
      lineHeight: s.lineHeight,
      theme: termTheme(theme),
      cursorBlink: s.cursorBlink,
      cursorStyle: s.cursorStyle as 'block' | 'bar' | 'underline',
      cursorInactiveStyle: 'outline',
      macOptionIsMeta: s.optionIsMeta,
      macOptionClickForcesSelection: true,
      scrollback: s.scrollback,
      drawBoldTextInBrightColors: true,
    })
    this.term.loadAddon(this.fit)
    this.term.loadAddon(this.search)
    this.term.loadAddon(new WebLinksAddon((e, url) => {
      if (e.metaKey || e.ctrlKey || !this.browse.active) OpenURL(url)
    }))
    const unicode = new Unicode11Addon()
    this.term.loadAddon(unicode)
    this.term.unicode.activeVersion = '11'
    this.term.open(this.host)
    if (!gpuOff) try {
      const webgl = new WebglAddon()
      webgl.onContextLoss(() => webgl.dispose())
      this.term.loadAddon(webgl)
    } catch {
      /* the DOM renderer is fine */
    }

    this.registerShellMarks()

    this.term.onData((data) => {
      if (this.exited) {
        void this.start()
        return
      }
      if (this.running) {
        this.typedSincePrompt = true
        Write(this.id, data)
      }
    })
    this.term.onTitleChange((t) => {
      this.oscTitle = t
      hooks.onChange(this)
    })
    this.term.onRender(() => this.scheduleOverlay())
    this.term.onScroll(() => this.scheduleOverlay())
    this.term.attachCustomKeyEventHandler((e) => this.onKey(e))
    this.setupMouse()

    new ResizeObserver(() => requestAnimationFrame(() => this.refit())).observe(this.host)
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

  write(b64: string) {
    this.term.write(b64ToBytes(b64))
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

  applySettings(s: settings.Settings, theme: Theme) {
    const o = this.term.options
    o.theme = termTheme(theme)
    o.fontFamily = fontStack(s.fontFamily)
    o.fontSize = s.fontSize
    o.lineHeight = s.lineHeight
    o.cursorBlink = s.cursorBlink
    o.cursorStyle = s.cursorStyle as 'block' | 'bar' | 'underline'
    o.scrollback = s.scrollback
    o.macOptionIsMeta = s.optionIsMeta
    this.refit()
    this.overlayKey = ''
    this.scheduleOverlay()
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
      if (this.cwd === home) return '~'
      return this.cwd.split('/').filter(Boolean).pop() ?? '/'
    }
    return this.oscTitle || this.hooks.shellName()
  }

  get prettyCwd(): string {
    const home = this.hooks.home()
    if (!this.cwd) return ''
    return home && this.cwd.startsWith(home) ? '~' + this.cwd.slice(home.length) : this.cwd
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
    const b = this.blocks[this.blocks.length - 1]
    if (!b || b.end || !b.start) return
    b.end = this.term.registerMarker(0)
    b.exitCode = code ?? 0
    b.finishedAt = Date.now()
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
        if (s.aiShareOutput) {
          const out = this.blockOutput(b)
          if (out) parts.push(clip(out, b === focus ? 12000 : 2500))
        }
      }
    } else if (s.aiShareOutput) {
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
    Write(this.id, '\x05\x15' + payload + (run ? '\r' : ''))
    this.term.scrollToBottom()
    this.term.focus()
    return true
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
      row = last ? this.dotLine(last) : Math.max(0, buf.baseY + buf.cursorY - 1)
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
      if (cur && this.browse.row > this.dotLine(cur)) target = cur
      else target = idx > 0 ? live[idx - 1] : idx === -1 ? [...live].reverse().find((b) => b.prompt.line < this.browse.row) : undefined
    } else {
      target = idx >= 0 ? live[idx + 1] : live.find((b) => b.prompt.line > this.browse.row)
    }
    if (target) this.moveTo(this.dotLine(target), 0)
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
        this.activate(e.shiftKey ? 'edit' : 'run')
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

  private activate(how: 'run' | 'edit') {
    const block = this.blockAt(this.browse.row)
    if (!block || !block.command) {
      if (how === 'edit') {
        const text = this.lineText(this.browse.row).trim()
        if (text && this.sendCommand(text, false)) this.exitBrowse()
      } else {
        toast(this.integration ? 'No command on this line — move to one with ↑ ↓' : 'Shell integration is off — use ⇧⏎ to copy this line to the prompt')
      }
      return
    }
    if (how === 'run' && this.hooks.settings().confirmRerun && this.browse.confirm !== block) {
      this.browse.confirm = block
      this.updateBrowse()
      return
    }
    if (this.sendCommand(block.command, how === 'run')) this.exitBrowse()
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
    const keys =
      '<kbd>↑</kbd><kbd>↓</kbd> move <kbd>⌥↑</kbd><kbd>⌥↓</kbd> commands <kbd>⏎</kbd> run <kbd>⇧⏎</kbd> edit ' +
      '<kbd>C</kbd> copy <kbd>O</kbd> output <kbd>A</kbd> ask AI <kbd>esc</kbd> back'
    if (this.browse.confirm) {
      return `<span class="hint-cmd">Run <code>${escapeHtml(this.browse.confirm.command)}</code> again?</span> <kbd>⏎</kbd> yes <kbd>esc</kbd> no`
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
    if (e.key === 'ArrowUp' && !e.altKey && !e.ctrlKey && !e.shiftKey && this.atEmptyPrompt()) {
      // The free-floating cursor starts where the text cursor is.
      const buf = this.term.buffer.active
      this.keyConsumed = true
      e.preventDefault()
      this.enterBrowse(buf.baseY + buf.cursorY - 1, buf.cursorX)
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

  private geometry() {
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

  // ---- Overlay: gutter dots and the browse cursor ------------------------------

  /** The first line of a block that has text (prompts often start with a blank line). */
  private dotLine(b: Block): number {
    const last = b.start && !b.start.isDisposed ? b.start.line - 1 : b.prompt.line + 3
    for (let i = b.prompt.line; i <= last; i++) if (this.lineText(i).trim()) return i
    return b.prompt.line
  }

  scheduleOverlay() {
    if (this.overlayFrame) return
    this.overlayFrame = requestAnimationFrame(() => {
      this.overlayFrame = 0
      this.renderOverlay()
    })
  }

  private renderOverlay() {
    const g = this.geometry()
    if (!g || !g.ch) return
    const vy = this.term.buffer.active.viewportY
    const rows = this.term.rows
    const alt = this.term.buffer.active.type === 'alternate'
    const visible = alt ? [] : this.liveBlocks().filter((b) => b.prompt.line >= vy - 200 && b.prompt.line < vy + rows)
    const br = this.browse
    const key = [
      vy, rows, g.left, g.top, g.ch, g.cw,
      visible.map((b) => `${b.id}:${b.prompt.line}:${b.end ? b.exitCode : 'r'}`).join(','),
      br.active ? `${br.row}:${br.col}:${br.confirm?.id ?? ''}` : '',
    ].join('|')
    if (key === this.overlayKey) return
    this.overlayKey = key
    this.overlay.textContent = ''

    const y = (line: number) => g.top + (line - vy) * g.ch

    if (br.active) {
      const block = this.blockAt(br.row)
      if (block) {
        const from = Math.max(block.prompt.line, vy)
        const to = Math.min(this.blockEnd(block), vy + rows - 1)
        if (to >= from) {
          const hl = el('div', { class: 'block-hl' + (block.end && block.exitCode !== 0 ? ' bad' : '') })
          hl.style.top = `${y(from)}px`
          hl.style.height = `${(to - from + 1) * g.ch}px`
          hl.style.left = `${g.left - 6}px`
          hl.style.width = `${g.cw * this.term.cols + 8}px`
          this.overlay.append(hl)
        }
      }
      if (br.row >= vy && br.row < vy + rows) {
        const line = el('div', { class: 'line-hl' })
        line.style.top = `${y(br.row)}px`
        line.style.height = `${g.ch}px`
        line.style.left = `${g.left - 6}px`
        line.style.width = `${g.cw * this.term.cols + 8}px`
        const cur = el('div', { class: 'float-cursor' })
        cur.style.top = `${y(br.row)}px`
        cur.style.left = `${g.left + br.col * g.cw}px`
        cur.style.width = `${g.cw}px`
        cur.style.height = `${g.ch}px`
        this.overlay.append(line, cur)
      }
    }

    for (const b of visible) {
      const line = this.dotLine(b)
      if (line < vy || line >= vy + rows) continue
      const state = !b.end ? 'run' : b.exitCode === 0 ? 'ok' : 'bad'
      const dot = el('button', { class: `dot ${state}` })
      dot.style.top = `${y(line) + g.ch / 2 - 4}px`
      dot.style.left = `${Math.max(2, g.left - 12)}px`
      const took = b.finishedAt && b.startedAt ? ` · took ${duration(b.finishedAt - b.startedAt)}` : ''
      dot.title =
        state === 'bad'
          ? `${b.command}\nFailed with exit ${b.exitCode}${took}\nClick to ask Claude why`
          : `${b.command}\n${state === 'run' ? 'Running' : 'OK'}${took}\nClick to select`
      dot.addEventListener('click', (e) => {
        e.stopPropagation()
        if (state === 'bad') this.hooks.onExplain(this, b)
        else this.enterBrowse(line, 0)
      })
      this.overlay.append(dot)
    }
  }
}
