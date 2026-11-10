import type { ITheme } from '@xterm/xterm'

export interface Theme {
  id: string
  name: string
  light?: boolean
  colors: ITheme
}

const t = (id: string, name: string, c: ITheme, light = false): Theme => ({ id, name, colors: c, light })

// Every color the shell asks for (ls, git, prompts) comes from these.
export const themes: Theme[] = [
  t('jamal-night', 'Jamal Night', {
    background: '#15161e', foreground: '#c8d3f5', cursor: '#ff8fb1', cursorAccent: '#15161e',
    selectionBackground: '#3d59a1aa',
    black: '#1b1d2b', red: '#ff757f', green: '#c3e88d', yellow: '#ffc777',
    blue: '#82aaff', magenta: '#c099ff', cyan: '#86e1fc', white: '#a9b8e8',
    brightBlack: '#545c7e', brightRed: '#ff8d94', brightGreen: '#c7fb6d', brightYellow: '#ffd8ab',
    brightBlue: '#9ab8ff', brightMagenta: '#caabff', brightCyan: '#b2ebff', brightWhite: '#d0daff',
  }),
  t('catppuccin-mocha', 'Catppuccin Mocha', {
    background: '#1e1e2e', foreground: '#cdd6f4', cursor: '#f5e0dc', cursorAccent: '#1e1e2e',
    selectionBackground: '#585b7099',
    black: '#45475a', red: '#f38ba8', green: '#a6e3a1', yellow: '#f9e2af',
    blue: '#89b4fa', magenta: '#f5c2e7', cyan: '#94e2d5', white: '#bac2de',
    brightBlack: '#585b70', brightRed: '#f38ba8', brightGreen: '#a6e3a1', brightYellow: '#f9e2af',
    brightBlue: '#89b4fa', brightMagenta: '#f5c2e7', brightCyan: '#94e2d5', brightWhite: '#a6adc8',
  }),
  t('dracula', 'Dracula', {
    background: '#282a36', foreground: '#f8f8f2', cursor: '#f8f8f2', cursorAccent: '#282a36',
    selectionBackground: '#44475a',
    black: '#21222c', red: '#ff5555', green: '#50fa7b', yellow: '#f1fa8c',
    blue: '#bd93f9', magenta: '#ff79c6', cyan: '#8be9fd', white: '#f8f8f2',
    brightBlack: '#6272a4', brightRed: '#ff6e6e', brightGreen: '#69ff94', brightYellow: '#ffffa5',
    brightBlue: '#d6acff', brightMagenta: '#ff92df', brightCyan: '#a4ffff', brightWhite: '#ffffff',
  }),
  t('tokyo-night', 'Tokyo Night', {
    background: '#1a1b26', foreground: '#c0caf5', cursor: '#c0caf5', cursorAccent: '#1a1b26',
    selectionBackground: '#33467c',
    black: '#15161e', red: '#f7768e', green: '#9ece6a', yellow: '#e0af68',
    blue: '#7aa2f7', magenta: '#bb9af7', cyan: '#7dcfff', white: '#a9b1d6',
    brightBlack: '#414868', brightRed: '#f7768e', brightGreen: '#9ece6a', brightYellow: '#e0af68',
    brightBlue: '#7aa2f7', brightMagenta: '#bb9af7', brightCyan: '#7dcfff', brightWhite: '#c0caf5',
  }),
  t('one-dark', 'One Dark', {
    background: '#282c34', foreground: '#abb2bf', cursor: '#528bff', cursorAccent: '#282c34',
    selectionBackground: '#3e4451',
    black: '#282c34', red: '#e06c75', green: '#98c379', yellow: '#e5c07b',
    blue: '#61afef', magenta: '#c678dd', cyan: '#56b6c2', white: '#abb2bf',
    brightBlack: '#5c6370', brightRed: '#e06c75', brightGreen: '#98c379', brightYellow: '#e5c07b',
    brightBlue: '#61afef', brightMagenta: '#c678dd', brightCyan: '#56b6c2', brightWhite: '#ffffff',
  }),
  t('nord', 'Nord', {
    background: '#2e3440', foreground: '#d8dee9', cursor: '#d8dee9', cursorAccent: '#2e3440',
    selectionBackground: '#434c5e',
    black: '#3b4252', red: '#bf616a', green: '#a3be8c', yellow: '#ebcb8b',
    blue: '#81a1c1', magenta: '#b48ead', cyan: '#88c0d0', white: '#e5e9f0',
    brightBlack: '#4c566a', brightRed: '#bf616a', brightGreen: '#a3be8c', brightYellow: '#ebcb8b',
    brightBlue: '#81a1c1', brightMagenta: '#b48ead', brightCyan: '#8fbcbb', brightWhite: '#eceff4',
  }),
  t('gruvbox-dark', 'Gruvbox Dark', {
    background: '#282828', foreground: '#ebdbb2', cursor: '#ebdbb2', cursorAccent: '#282828',
    selectionBackground: '#504945',
    black: '#282828', red: '#d23d37', green: '#98971a', yellow: '#d79921',
    blue: '#458588', magenta: '#b16286', cyan: '#689d6a', white: '#a89984',
    brightBlack: '#928374', brightRed: '#fb4934', brightGreen: '#b8bb26', brightYellow: '#fabd2f',
    brightBlue: '#83a598', brightMagenta: '#d3869b', brightCyan: '#8ec07c', brightWhite: '#ebdbb2',
  }),
  t('solarized-dark', 'Solarized Dark', {
    background: '#002b36', foreground: '#93a1a1', cursor: '#93a1a1', cursorAccent: '#002b36',
    selectionBackground: '#073642',
    black: '#073642', red: '#dc322f', green: '#859900', yellow: '#b58900',
    blue: '#268bd2', magenta: '#d33682', cyan: '#2aa198', white: '#eee8d5',
    brightBlack: '#586e75', brightRed: '#cb4b16', brightGreen: '#859900', brightYellow: '#b58900',
    brightBlue: '#268bd2', brightMagenta: '#6c71c4', brightCyan: '#2aa198', brightWhite: '#fdf6e3',
  }),
  t('github-light', 'GitHub Light', {
    background: '#ffffff', foreground: '#1f2328', cursor: '#0969da', cursorAccent: '#ffffff',
    selectionBackground: '#0969da33',
    black: '#24292f', red: '#cf222e', green: '#116329', yellow: '#4d2d00',
    blue: '#0969da', magenta: '#8250df', cyan: '#1b7c83', white: '#6e7781',
    brightBlack: '#57606a', brightRed: '#a40e26', brightGreen: '#1a7f37', brightYellow: '#633c01',
    brightBlue: '#218bff', brightMagenta: '#a475f9', brightCyan: '#3192aa', brightWhite: '#8c959f',
  }, true),
  t('solarized-light', 'Solarized Light', {
    background: '#fdf6e3', foreground: '#586e75', cursor: '#586e75', cursorAccent: '#fdf6e3',
    selectionBackground: '#eee8d5',
    black: '#073642', red: '#dc322f', green: '#819400', yellow: '#b08500',
    blue: '#268bd2', magenta: '#d33682', cyan: '#299c93', white: '#eee8d5',
    brightBlack: '#657b83', brightRed: '#cb4b16', brightGreen: '#819400', brightYellow: '#b08500',
    brightBlue: '#268bd2', brightMagenta: '#6c71c4', brightCyan: '#299c93', brightWhite: '#002b36',
  }, true),
]

export function themeById(id: string): Theme {
  return themes.find((x) => x.id === id) ?? themes[0]
}

// ---- Window colors, derived from the terminal colors -------------------------

function hex(c: string): [number, number, number] {
  const h = c.replace('#', '').slice(0, 6)
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)]
}

function mix(a: string, b: string, amount: number): string {
  const [r1, g1, b1] = hex(a)
  const [r2, g2, b2] = hex(b)
  const m = (x: number, y: number) => Math.round(x + (y - x) * amount)
  return `rgb(${m(r1, r2)}, ${m(g1, g2)}, ${m(b1, b2)})`
}

function rgba(color: string, alpha: number): string {
  const m = /^rgb\((\d+), (\d+), (\d+)\)$/.exec(color)
  const [r, g, b] = m ? [+m[1], +m[2], +m[3]] : hex(color)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/** The terminal background at the chosen window opacity (0.3–1). */
export function bgAt(theme: Theme, alpha: number): string {
  return alpha >= 1 ? theme.colors.background! : rgba(theme.colors.background!, Math.max(0, alpha))
}

/** Sets CSS variables so the tabs, panels and dialogs match the theme.
 *  opacity < 1 makes the window see-through (the desktop shows behind). */
/** Colors for the window around the terminal. frame "system" follows
 * macOS (light/dark, accent); "theme" uses the terminal theme's colors. */
export function applyChrome(theme: Theme, opacity = 1, frame: 'system' | 'theme' = 'theme') {
  const c = theme.colors
  const bg = c.background!, fg = c.foreground!
  const s = document.documentElement.style
  const a = Math.max(0.3, Math.min(1, opacity))
  // The tab bar and side areas stay a little more solid than the terminal.
  const barA = Math.min(1, a + 0.12)
  s.setProperty('--bg-a', bgAt(theme, a))
  s.setProperty('--panel-a', rgba(mix(bg, fg, theme.light ? 0.04 : 0.05), barA))
  s.setProperty('--titlebar-bg', rgba(mix(bg, '#000000', theme.light ? 0.14 : 0.38), barA))
  document.documentElement.classList.toggle('see-through', a < 1)
  s.setProperty('--bg', bg)
  s.setProperty('--fg', fg)
  s.setProperty('--muted', mix(fg, bg, 0.45))
  s.setProperty('--panel', mix(bg, fg, theme.light ? 0.04 : 0.05))
  s.setProperty('--panel-2', mix(bg, fg, theme.light ? 0.08 : 0.1))
  s.setProperty('--border', mix(bg, fg, theme.light ? 0.14 : 0.14))
  s.setProperty('--accent', c.cursor!)
  s.setProperty('--blue', c.blue!)
  s.setProperty('--green', c.green!)
  s.setProperty('--red', c.red!)
  s.setProperty('--yellow', c.yellow!)
  s.setProperty('--magenta', c.magenta!)
  s.setProperty('--highlight', theme.light ? 'rgba(9, 105, 218, 0.10)' : 'rgba(130, 170, 255, 0.10)')
  s.setProperty('--highlight-strong', theme.light ? 'rgba(9, 105, 218, 0.20)' : 'rgba(130, 170, 255, 0.20)')
  document.documentElement.dataset.light = theme.light ? '1' : ''
  // The AI panel: a lighter, raised surface with a violet tint, so it never
  // reads as part of the terminal.
  const aiBase = theme.light ? '#ffffff' : mix(mix(bg, fg, 0.05), '#ffffff', 0.13)
  s.setProperty('--ai', mix(aiBase, c.magenta!, theme.light ? 0.04 : 0.08))
  s.setProperty('--ai-2', mix(aiBase, c.magenta!, theme.light ? 0.08 : 0.13))
  s.removeProperty('--field')
  s.removeProperty('--accent-fg')
  if (frame === 'system') applySystemFrame(a, barA)
  document.documentElement.classList.toggle('frame-system', frame === 'system')
}

/** Overrides the frame colors with macOS's (the terminal keeps its theme). */
export function applySystemFrame(a = 1, barA = 1) {
  const dark = systemDark()
  const p = systemPalette(dark)
  const s = document.documentElement.style
  s.setProperty('--fg', p.fg)
  s.setProperty('--muted', p.muted)
  s.setProperty('--panel', p.panel)
  s.setProperty('--panel-2', p.panel2)
  s.setProperty('--border', p.border)
  s.setProperty('--panel-a', rgba(p.window, barA))
  s.setProperty('--titlebar-bg', rgba(p.titlebar, barA))
  s.setProperty('--red', p.red)
  s.setProperty('--green', p.green)
  s.setProperty('--yellow', p.yellow)
  s.setProperty('--blue', p.blue)
  s.setProperty('--magenta', p.purple)
  // The accent color you picked in System Settings (Safari/WebKit name),
  // else Apple's blue.
  s.setProperty('--accent', CSS.supports('color', '-apple-system-control-accent') ? '-apple-system-control-accent' : p.blue)
  s.setProperty('--accent-fg', '#ffffff')
  s.setProperty('--field', dark ? '#1e1e1e' : '#ffffff') // text fields
  s.setProperty('--highlight', dark ? 'rgba(10, 132, 255, 0.16)' : 'rgba(0, 122, 255, 0.10)')
  s.setProperty('--highlight-strong', dark ? 'rgba(10, 132, 255, 0.28)' : 'rgba(0, 122, 255, 0.20)')
  const ai = dark ? '#3a3540' : '#ffffff'
  s.setProperty('--ai', ai)
  s.setProperty('--ai-2', dark ? '#443e4c' : '#f7f3fc')
  document.documentElement.dataset.light = dark ? '' : '1'
  void a
}

export const systemDark = () => matchMedia('(prefers-color-scheme: dark)').matches

/** Apple's system colors (light / dark), from the Human Interface Guidelines. */
export function systemPalette(dark: boolean) {
  return dark
    ? {
        window: '#1e1e1e', titlebar: '#2a2a2a', panel: '#262626', panel2: '#333333', border: '#3d3d3d',
        fg: '#f5f5f7', muted: '#98989d', red: '#ff453a', green: '#30d158', yellow: '#ffd60a',
        blue: '#0a84ff', purple: '#bf5af2', orange: '#ff9f0a',
      }
    : {
        window: '#ececec', titlebar: '#e3e3e3', panel: '#f5f5f5', panel2: '#e8e8e8', border: '#d1d1d1',
        fg: '#1d1d1f', muted: '#6e6e73', red: '#ff3b30', green: '#28a745', yellow: '#b8860b',
        blue: '#007aff', purple: '#af52de', orange: '#ff9500',
      }
}
