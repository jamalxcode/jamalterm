import type { ITheme } from '@xterm/xterm'

// "Jamal Night": a deep blue-black background with soft, high-contrast
// colors. Every color the shell asks for (ls, git, prompts) comes from here.
export const jamalNight: ITheme = {
  background: '#15161e',
  foreground: '#c8d3f5',
  cursor: '#ff8fb1',
  cursorAccent: '#15161e',
  selectionBackground: '#3d59a1aa',
  selectionInactiveBackground: '#3d59a155',

  black: '#1b1d2b',
  red: '#ff757f',
  green: '#c3e88d',
  yellow: '#ffc777',
  blue: '#82aaff',
  magenta: '#c099ff',
  cyan: '#86e1fc',
  white: '#a9b8e8',

  brightBlack: '#545c7e',
  brightRed: '#ff8d94',
  brightGreen: '#c7fb6d',
  brightYellow: '#ffd8ab',
  brightBlue: '#9ab8ff',
  brightMagenta: '#caabff',
  brightCyan: '#b2ebff',
  brightWhite: '#d0daff',
}

// Window chrome (title bar) uses the same palette.
export const chrome = {
  background: jamalNight.background as string,
  title: '#7a88cf',
}
