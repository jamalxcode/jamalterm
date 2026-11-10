// What's drawn over the terminal: a mark beside each command (✓ = OK,
// ✕ = failed, spinner = running) and, in browse mode, the free-floating
// cursor with its line and command highlighted.
import type { Tab, Block } from './tab'
import { el, duration } from './util'

/** The first line of a block that has text (prompts often start with a blank line). */
export function dotLine(t: Tab, b: Block): number {
  const last = b.start && !b.start.isDisposed ? b.start.line - 1 : b.prompt.line + 3
  for (let i = b.prompt.line; i <= last; i++) if (t.lineText(i).trim()) return i
  return b.prompt.line
}

/** Draws the overlay; returns what it drew (to skip redrawing the same). */
export function renderOverlay(t: Tab, lastKey: string, explain: (b: Block) => void): string {
  const g = t.geometry()
  if (!g || !g.ch) return lastKey
  const vy = t.term.buffer.active.viewportY
  const rows = t.term.rows
  const alt = t.term.buffer.active.type === 'alternate'
  const visible = alt ? [] : t.liveBlocks().filter((b) => b.prompt.line >= vy - 200 && b.prompt.line < vy + rows)
  const br = t.browse
  const key = [
    vy, rows, g.left, g.top, g.ch, g.cw,
    visible.map((b) => `${b.id}:${b.prompt.line}:${b.end ? b.exitCode : 'r'}`).join(','),
    br.active ? `${br.row}:${br.col}:${br.confirm?.id ?? ''}` : '',
  ].join('|')
  if (key === lastKey) return lastKey
  t.overlay.textContent = ''

  const y = (line: number) => g.top + (line - vy) * g.ch

  if (br.active) {
    const block = t.blockAt(br.row)
    if (block) {
      const from = Math.max(block.prompt.line, vy)
      const to = Math.min(t.blockEnd(block), vy + rows - 1)
      if (to >= from) {
        const hl = el('div', { class: 'block-hl' + (block.end && block.exitCode !== 0 ? ' bad' : '') })
        hl.style.top = `${y(from)}px`
        hl.style.height = `${(to - from + 1) * g.ch}px`
        hl.style.left = `${g.left - 6}px`
        hl.style.width = `${g.cw * t.term.cols + 8}px`
        t.overlay.append(hl)
      }
    }
    if (br.row >= vy && br.row < vy + rows) {
      const line = el('div', { class: 'line-hl' })
      line.style.top = `${y(br.row)}px`
      line.style.height = `${g.ch}px`
      line.style.left = `${g.left - 6}px`
      line.style.width = `${g.cw * t.term.cols + 8}px`
      const cur = el('div', { class: 'float-cursor' })
      cur.style.top = `${y(br.row)}px`
      cur.style.left = `${g.left + br.col * g.cw}px`
      cur.style.width = `${g.cw}px`
      cur.style.height = `${g.ch}px`
      t.overlay.append(line, cur)
    }
  }

  for (const b of visible) {
    const line = dotLine(t, b)
    if (line < vy || line >= vy + rows) continue
    const state = !b.end ? 'run' : b.exitCode === 0 ? 'ok' : 'bad'
    // Shapes as well as colors: ✓ = OK, ✕ = failed, spinner = running.
    const dot = el('button', { class: `dot ${state}`, 'aria-label': state === 'bad' ? 'failed' : state === 'ok' ? 'succeeded' : 'running' }, state === 'bad' ? '✕' : state === 'ok' ? '✓' : '')
    dot.style.top = `${y(line) + g.ch / 2 - 4}px`
    dot.style.left = `${Math.max(2, g.left - 12)}px`
    const took = b.finishedAt && b.startedAt ? ` · took ${duration(b.finishedAt - b.startedAt)}` : ''
    dot.title =
      state === 'bad'
        ? `${b.command}\nFailed with exit ${b.exitCode}${took}\nClick to ask Claude why`
        : `${b.command}\n${state === 'run' ? 'Running' : 'OK'}${took}\nClick to select`
    dot.addEventListener('click', (e) => {
      e.stopPropagation()
      if (state === 'bad') explain(b)
      else t.enterBrowse(line, 0)
    })
    t.overlay.append(dot)
  }
  return key
}
