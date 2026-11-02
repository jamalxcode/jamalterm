// The kitty keyboard protocol, level 1 ("disambiguate escape codes"):
// programs that ask for it (fish 4, neovim, helix, …) can tell Esc from
// Alt+key, Ctrl+I from Tab, Shift+Enter from Enter, and so on.
// https://sw.kovidgoyal.net/kitty/keyboard-protocol/
import type { Terminal } from '@xterm/xterm'

const DISAMBIGUATE = 1

// e.code → the key's base character (the US layout's, as kitty does for
// keys without a text of their own).
const codeChar: Record<string, string> = {
  Space: ' ', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\',
  Semicolon: ';', Quote: "'", Backquote: '`', Comma: ',', Period: '.', Slash: '/',
}

export class KittyKeyboard {
  private main: number[] = []
  private alt: number[] = []

  constructor(private term: Terminal, reply: (s: string) => void) {
    const p = term.parser
    const num = (v: number | number[] | undefined, d: number) => (typeof v === 'number' && v > 0 ? v : Array.isArray(v) ? v[0] ?? d : d)
    // CSI > flags u: push.
    p.registerCsiHandler({ prefix: '>', final: 'u' }, (params) => {
      const s = this.stack()
      s.push(num(params[0], 0) & 0x1f)
      if (s.length > 32) s.shift()
      return true
    })
    // CSI < n u: pop n.
    p.registerCsiHandler({ prefix: '<', final: 'u' }, (params) => {
      const s = this.stack()
      s.splice(Math.max(0, s.length - num(params[0], 1)))
      return true
    })
    // CSI = flags ; mode u: set (1), add (2) or remove (3).
    p.registerCsiHandler({ prefix: '=', final: 'u' }, (params) => {
      const f = num(params[0], 0) & 0x1f, mode = num(params[1], 1)
      const s = this.stack()
      const cur = s.length ? s[s.length - 1] : 0
      const next = mode === 2 ? cur | f : mode === 3 ? cur & ~f : f
      if (s.length) s[s.length - 1] = next
      else s.push(next)
      return true
    })
    // CSI ? u: which flags are on.
    p.registerCsiHandler({ prefix: '?', final: 'u' }, () => {
      reply(`\x1b[?${this.flags}u`)
      return true
    })
    // A full reset (ESC c) turns it all off; let xterm.js reset too.
    p.registerEscHandler({ final: 'c' }, () => {
      this.reset()
      return false
    })
  }

  private stack() {
    return this.term.buffer.active.type === 'alternate' ? this.alt : this.main
  }

  get flags(): number {
    const s = this.stack()
    return s.length ? s[s.length - 1] : 0
  }

  reset() {
    this.main = []
    this.alt = []
  }

  /** The bytes to send for a key, or null to let the normal handling run. */
  encode(e: KeyboardEvent, optionIsMeta: boolean): string | null {
    if (!(this.flags & DISAMBIGUATE) || e.metaKey) return null
    const alt = e.altKey && (optionIsMeta || e.ctrlKey)
    if (e.altKey && !alt) return null // Option types a special character
    const mods = 1 + (e.shiftKey ? 1 : 0) + (alt ? 2 : 0) + (e.ctrlKey ? 4 : 0)
    const csi = (code: number) => (mods === 1 ? `\x1b[${code}u` : `\x1b[${code};${mods}u`)

    switch (e.key) {
      case 'Escape': return csi(27)
      // Enter, Tab and Backspace stay as they were unless modified, so a
      // shell still works if a program left the mode on.
      case 'Enter': return mods === 1 ? null : csi(13)
      case 'Tab': return mods === 1 ? null : csi(9)
      case 'Backspace': return mods === 1 ? null : csi(127)
    }
    if (!e.ctrlKey && !alt) return null // text, also with Shift
    let ch = ''
    if (/^Key[A-Z]$/.test(e.code)) ch = e.code.slice(3).toLowerCase()
    else if (/^Digit\d$/.test(e.code)) ch = e.code.slice(5)
    else ch = codeChar[e.code] ?? ''
    if (!ch) return null
    return csi(ch.codePointAt(0)!)
  }
}
