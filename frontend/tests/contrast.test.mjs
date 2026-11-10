// Every theme's text is readable: WCAG contrast of the main text (4.5:1,
// the AA level for normal text) and of the 12 ANSI colors programs use
// for errors, git, ls and prompts (3:1), against the theme's background.
// Run: node --test --experimental-strip-types tests/contrast.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { themes, wallpaperTheme, setWallpaperColors, weeklyTheme } from '../src/themes.ts'

const lum = (hex) => {
  const h = hex.replace('#', '').slice(0, 6)
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}
export const contrast = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p)
  return (x + 0.05) / (y + 0.05)
}

const ansi = ['red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan']

for (const th of themes) {
  test(`${th.name} is readable`, () => {
    const c = th.colors
    const bad = []
    const fg = contrast(c.foreground, c.background)
    if (fg < 4.5) bad.push(`text ${c.foreground} ${fg.toFixed(2)}:1 (needs 4.5)`)
    for (const k of ansi) {
      const r = contrast(c[k], c.background)
      if (r < 3) bad.push(`${k} ${c[k]} ${r.toFixed(2)}:1 (needs 3)`)
    }
    assert.deepEqual(bad, [], `${th.name} on ${c.background}:\n  ` + bad.join('\n  '))
  })
}

// "Match my wallpaper" makes a theme from any picture: it must still read.
const wallpapers = {
  'a bright beach': ['#d8c7a6', '#8a7a5c', '#f4b860', '#3fa7d6', '#ffffff'],
  'a dark forest': ['#1f2a1e', '#050805', '#5f8f3a', '#2d4a2a', '#a3c45f'],
  'a pink sunset': ['#c06c84', '#355c7d', '#f67280', '#f8b195', '#6c5b7b'],
  'plain grey': ['#808080', '#7a7a7a'],
}
for (const [name, colors] of Object.entries(wallpapers)) {
  test(`a theme from ${name} is readable`, () => {
    setWallpaperColors(colors)
    const c = wallpaperTheme().colors
    const bad = []
    if (contrast(c.foreground, c.background) < 4.5) bad.push(`text ${contrast(c.foreground, c.background).toFixed(2)}`)
    for (const k of ansi) if (contrast(c[k], c.background) < 3) bad.push(`${k} ${c[k]} ${contrast(c[k], c.background).toFixed(2)}`)
    assert.deepEqual(bad, [])
  })
}

test('theme of the week changes weekly and is a real theme', () => {
  const a = weeklyTheme(new Date('2026-11-09T10:00:00Z')) // a Monday
  const b = weeklyTheme(new Date('2026-11-15T10:00:00Z')) // that Sunday
  const c = weeklyTheme(new Date('2026-11-16T10:00:00Z')) // next Monday
  assert.equal(a.colors.background, b.colors.background, 'same all week')
  assert.notEqual(a.colors.background, c.colors.background, 'new the next week')
})
