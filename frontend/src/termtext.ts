// Small helpers for terminal text: quoting, prompts, ssh targets, fonts and
// colors. No state; used by tab.ts and tested through it.
import { type Theme, bgAt } from './themes'

export function shellQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`
}

/** True the first time it's called for a key (remembered per Mac). */
export function once(key: string): boolean {
  try {
    if (localStorage.getItem(key)) return false
    localStorage.setItem(key, '1')
  } catch {
    /* private mode: show it */
  }
  return true
}

/** A tab as saved in session.json. */

/** Adds the bundled Nerd Font icons as a fallback to any font. */
export function fontStack(family: string): string {
  const sym = '"Symbols Nerd Font Mono"'
  if (family.includes(sym)) return family
  const parts = family.split(',').map((p) => p.trim()).filter(Boolean)
  const generic = parts.length && /^(monospace|ui-monospace)$/.test(parts[parts.length - 1]) ? parts.pop()! : 'monospace'
  return [...parts, sym, generic].join(', ')
}

export function termTheme(theme: Theme, opacity = 100) {
  return {
    ...theme.colors,
    // See-through: the terminal draws no background of its own (the area
    // behind it is tinted once); alpha 0 keeps the RGB for reverse video.
    background: opacity < 100 ? bgAt(theme, 0.0) : theme.colors.background,
    scrollbarSliderBackground: theme.light ? '#00000022' : '#ffffff22',
    scrollbarSliderHoverBackground: theme.light ? '#00000044' : '#ffffff44',
    scrollbarSliderActiveBackground: theme.light ? '#00000055' : '#ffffff55',
  }
}

/** The command on a line that looks like a shell prompt ("user@host:~$ ls",
 * "% make", "❯ git status"), or "" if it doesn't look like one. */
export function guessCommand(line: string): string {
  const m = /^(?:\S.{0,80}?)?[$%#❯›»➜λ]\s+(\S.*)$/u.exec(line.trimEnd())
  if (!m) return ''
  const cmd = m[1].trim()
  // Not a prompt: "Total: 15 $ 20", "# comment", prices…
  if (/^[\d.,]+\b/.test(cmd) || cmd.length > 500) return ''
  return cmd
}

/** The machine an ssh command connects to: "ssh -p 22 me@prod-db ls" → "prod-db". */
export function sshTarget(command: string): string {
  const words = command.trim().split(/\s+/)
  if (words[0] === 'sudo') words.shift()
  if (!/(^|\/)(ssh|mosh|autossh)$/.test(words[0] ?? '')) return ''
  const withArg = new Set(['-b', '-c', '-D', '-E', '-e', '-F', '-I', '-i', '-J', '-L', '-l', '-m', '-O', '-o', '-p', '-Q', '-R', '-S', '-W', '-w', '--ssh', '--port'])
  for (let i = 1; i < words.length; i++) {
    const w = words[i]
    if (w.startsWith('-')) {
      if (withArg.has(w)) i++
      continue
    }
    return w.replace(/^[^@]*@/, '').replace(/:.*$/, '')
  }
  return ''
}

/** Does host match a pattern like "prod-*, *.example.com"? */
export function hostMatches(host: string, pattern: string): boolean {
  return pattern.split(',').map((p) => p.trim()).filter(Boolean).some((p) => {
    const re = new RegExp('^' + p.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*') + '$', 'i')
    return re.test(host)
  })
}
