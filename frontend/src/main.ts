import '@xterm/xterm/css/xterm.css'
import './style.css'

import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'

import { Start, Write, Resize, ShellName } from '../wailsjs/go/main/App'
import { EventsOn, BrowserOpenURL, WindowSetTitle } from '../wailsjs/runtime/runtime'
import { jamalNight, chrome } from './theme'

const DEFAULT_FONT_SIZE = 14
const MIN_FONT_SIZE = 9
const MAX_FONT_SIZE = 32

const host = document.getElementById('terminal')!
const titleEl = document.getElementById('title')!
document.documentElement.style.setProperty('--bg', chrome.background)
document.documentElement.style.setProperty('--title', chrome.title)

const term = new Terminal({
  allowProposedApi: true, // needed by the unicode11 addon
  fontFamily: '"SF Mono", Menlo, Monaco, "JetBrains Mono", monospace',
  fontSize: DEFAULT_FONT_SIZE,
  lineHeight: 1.15,
  theme: jamalNight,
  cursorBlink: true,
  cursorStyle: 'block',
  cursorInactiveStyle: 'outline',
  macOptionIsMeta: true, // Option+B / Option+F jump words in zsh and bash
  macOptionClickForcesSelection: true,
  scrollback: 10000,
  drawBoldTextInBrightColors: true,
  minimumContrastRatio: 1,
})

const fit = new FitAddon()
term.loadAddon(fit)
term.loadAddon(new WebLinksAddon((_e, url) => BrowserOpenURL(url)))
const unicode = new Unicode11Addon()
term.loadAddon(unicode)
term.unicode.activeVersion = '11' // correct widths for emoji and CJK

term.open(host)

// GPU rendering; fall back to the default renderer if WebGL is unavailable.
try {
  const webgl = new WebglAddon()
  webgl.onContextLoss(() => webgl.dispose())
  term.loadAddon(webgl)
} catch {
  /* DOM renderer is fine */
}

// ---- Shell output -> screen -------------------------------------------------

function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

let running = false
let starting = false

EventsOn('pty:data', (b64: string) => term.write(b64ToBytes(b64)))

EventsOn('pty:exit', (code: number) => {
  running = false
  term.write(`\r\n\x1b[2m[process exited with code ${code} — press any key to start a new shell]\x1b[0m\r\n`)
})

// Window title: "zsh — ~/projects" if the shell reports it (OSC 0/2).
let shellName = 'shell'
term.onTitleChange((t) => setTitle(t || shellName))
function setTitle(t: string) {
  titleEl.textContent = t
  WindowSetTitle(t)
}

// ---- Keyboard -> shell ------------------------------------------------------

term.onData((data) => {
  if (!running) {
    void startShell()
    return
  }
  Write(data)
})

term.attachCustomKeyEventHandler((e) => {
  if (e.type !== 'keydown' || !e.metaKey) return true
  switch (e.key) {
    case 'k': // Cmd+K: clear screen and scrollback, like Terminal.app
      term.clear()
      return false
    case '=':
    case '+':
      setFontSize((term.options.fontSize ?? DEFAULT_FONT_SIZE) + 1)
      return false
    case '-':
      setFontSize((term.options.fontSize ?? DEFAULT_FONT_SIZE) - 1)
      return false
    case '0':
      setFontSize(DEFAULT_FONT_SIZE)
      return false
  }
  return true // let Cmd+C / Cmd+V / Cmd+Q etc. reach the menu
})

function setFontSize(size: number) {
  term.options.fontSize = Math.max(MIN_FONT_SIZE, Math.min(MAX_FONT_SIZE, size))
  refit()
}

// ---- Size -------------------------------------------------------------------

let lastCols = 0
let lastRows = 0
function refit() {
  fit.fit()
  if (term.cols !== lastCols || term.rows !== lastRows) {
    lastCols = term.cols
    lastRows = term.rows
    if (running) Resize(term.cols, term.rows)
  }
}

let pending = 0
new ResizeObserver(() => {
  cancelAnimationFrame(pending)
  pending = requestAnimationFrame(refit)
}).observe(host)

// ---- Start ------------------------------------------------------------------

async function startShell() {
  if (starting) return
  starting = true
  try {
    refit()
    term.reset()
    await Start(term.cols, term.rows)
    running = true
    Resize(term.cols, term.rows)
  } catch (err) {
    term.write(`\x1b[31mCould not start the shell: ${err}\x1b[0m\r\n`)
  } finally {
    starting = false
  }
}

async function boot() {
  // Wait for the font so the character grid is measured correctly.
  await document.fonts.ready
  shellName = await ShellName()
  setTitle(shellName)
  await startShell()
  term.focus()
}

void boot()
