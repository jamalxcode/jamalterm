// The command table from Go (commands.go): one list for the menu bar, the
// command palette and ⌘ shortcuts typed while the terminal has the focus.
import { Commands, RunCommand } from './backend'
import type { Command } from '../bindings/jamalterm/models'
import { isMac } from './util'

export let commands: Command[] = []

export async function loadCommands() {
  commands = (await Commands()) ?? []
}

const codes: Record<string, string> = {
  '[': 'BracketLeft', ']': 'BracketRight', '=': 'Equal', '-': 'Minus', ',': 'Comma', '.': 'Period',
  '/': 'Slash', ';': 'Semicolon', "'": 'Quote', '`': 'Backquote', '\\': 'Backslash',
  Up: 'ArrowUp', Down: 'ArrowDown', Left: 'ArrowLeft', Right: 'ArrowRight',
}

interface Combo { cmd: boolean; shift: boolean; alt: boolean; ctrl: boolean; code: string }

function parse(keys: string): Combo | null {
  if (!keys) return null
  const parts = keys.split('+')
  const key = parts.pop()!
  const has = (m: string) => parts.includes(m)
  const code = /^[A-Za-z]$/.test(key) ? 'Key' + key.toUpperCase() : /^\d$/.test(key) ? 'Digit' + key : codes[key] ?? key
  return { cmd: has('Cmd'), shift: has('Shift'), alt: has('Option') || has('Alt'), ctrl: has('Ctrl'), code }
}

/** The command for a ⌘ key press, if any (Mac builds). */
export function commandForKey(e: KeyboardEvent): Command | undefined {
  if (!e.metaKey) return undefined
  return commands.find((c) => {
    const k = parse(c.keys)
    return !!k && k.cmd && k.code === e.code && k.shift === e.shiftKey && k.alt === e.altKey && k.ctrl === e.ctrlKey
  })
}

/** Runs a command: the app's own (windows, quit…) or the window's. */
export function runCommand(c: Command, runMenu: (id: string) => void) {
  if (c.native) void RunCommand(c.id)
  else runMenu(c.id)
}

/** Shortcut text for people: "⇧⌘D" on a Mac. */
export function keyLabel(keys: string): string {
  if (!keys || !isMac) return keys
  const parts = keys.split('+')
  const key = parts.pop()!
  const sym: Record<string, string> = { Ctrl: '⌃', Option: '⌥', Shift: '⇧', Cmd: '⌘' }
  const order = ['Ctrl', 'Option', 'Shift', 'Cmd']
  const arrows: Record<string, string> = { Up: '↑', Down: '↓', Left: '←', Right: '→' }
  return order.filter((m) => parts.includes(m)).map((m) => sym[m]).join('') + (arrows[key] ?? key.toUpperCase())
}
