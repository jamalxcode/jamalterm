// What's drawn over the terminal: a mark beside each command (✓ = OK,
// ✕ = failed, spinner = running) and, in browse mode, the free-floating
// cursor with its line and command highlighted.
import type { Tab, Block } from './tab'
import { el, duration, copyText, toast } from './util'

/** The first line of a block that has text (prompts often start with a blank line). */
export function dotLine(t: Tab, b: Block): number {
  const last = b.start && !b.start.isDisposed ? b.start.line - 1 : b.prompt.line + 3
  for (let i = b.prompt.line; i <= last; i++) if (t.lineText(i).trim()) return i
  return b.prompt.line
}

/** Draws the overlay; returns what it drew (to skip redrawing the same). */
export interface OverlayActions {
  explain(b: Block): void
  fix(b: Block): void
}

/** The command whose output fills the top of the view (its prompt has
 * scrolled away), for the sticky header. */
export function stickyBlock(t: Tab, all: Block[], vy: number): Block | undefined {
  for (let i = all.length - 1; i >= 0; i--) {
    const b = all[i]
    if (b.prompt.line >= vy) continue
    if (!b.command || !b.start) return undefined
    return t.blockEnd(b) >= vy ? b : undefined
  }
  return undefined
}

export function renderOverlay(t: Tab, lastKey: string, actions: OverlayActions): string {
  const explain = actions.explain
  const g = t.geometry()
  if (!g || !g.ch) return lastKey
  const vy = t.term.buffer.active.viewportY
  const rows = t.term.rows
  const alt = t.term.buffer.active.type === 'alternate'
  const all = alt ? [] : t.liveBlocks()
  const visible = all.filter((b) => b.prompt.line >= vy - 200 && b.prompt.line < vy + rows)
  const sticky = stickyBlock(t, all, vy)
  const total = t.term.buffer.active.length
  const br = t.browse
  const key = [
    sticky ? `s${sticky.id}:${sticky.end ? sticky.exitCode : 'r'}` : '',
    `${total}:${all.length}:${all.map((b) => (b.end ? (b.exitCode === 0 ? 'o' : 'x') : 'r')).join('')}`,
    vy, rows, g.left, g.top, g.ch, g.cw,
    visible.map((b) => `${b.id}:${b.prompt.line}:${b.end ? b.exitCode : 'r'}`).join(','),
    br.active ? `${br.row}:${br.col}:${br.confirm?.id ?? ''}` : '',
  ].join('|')
  if (key === lastKey) return lastKey
  t.overlay.textContent = ''

  const y = (line: number) => g.top + (line - vy) * g.ch

  // Sticky header: which command the output at the top belongs to.
  if (sticky && !br.active) {
    const ok = sticky.end && sticky.exitCode === 0, bad = sticky.end && sticky.exitCode !== 0
    const took = sticky.finishedAt && sticky.startedAt ? duration(sticky.finishedAt - sticky.startedAt) : ''
    const mark = el('span', { class: `mark ${ok ? 'ok' : bad ? 'bad' : 'run'}`, 'aria-label': ok ? 'succeeded' : bad ? 'failed' : 'running' }, ok ? '✓' : bad ? '✕' : '…')
    const cmd = el('button', { class: 'cmd', title: 'Go to the command' }, sticky.command)
    cmd.addEventListener('click', () => t.term.scrollToLine(Math.max(0, sticky.prompt.line - 1)))
    const info = el('span', { class: 'info', text: bad ? `exit ${sticky.exitCode}${took ? ` · ${took}` : ''}` : took || (sticky.end ? '' : 'running') })
    const copy = el('button', { class: 'act', title: 'Copy this command’s output' }, 'Copy output')
    copy.addEventListener('click', () => { void copyText(t.blockOutput(sticky)); toast('Copied the output') })
    const end = el('button', { class: 'act', title: 'Jump past this output' }, 'Skip to end ↓')
    end.addEventListener('click', () => t.term.scrollToLine(Math.max(0, t.blockEnd(sticky) - rows + 3)))
    const bar = el('div', { class: 'sticky-cmd' + (bad ? ' bad' : '') }, mark, cmd, info)
    if (bad) {
      const fix = el('button', { class: 'act fix', title: 'Ask AI for a corrected command' }, '✦ Fix it')
      fix.addEventListener('click', () => actions.fix(sticky))
      bar.append(fix)
    }
    bar.append(copy, end)
    bar.style.top = `${g.top}px`
    bar.style.left = `${g.left - 6}px`
    bar.style.width = `${g.cw * t.term.cols + 8}px`
    t.overlay.append(bar)
  }

  // Markers beside the scrollbar: one per command, failures in red, so you
  // can see where the errors are in a long scrollback and jump to them.
  if (all.length > 1 && total > rows) {
    const rail = el('div', { class: 'cmd-rail', 'aria-hidden': 'true' })
    const h = rows * g.ch
    rail.style.top = `${g.top}px`
    rail.style.height = `${h}px`
    for (const b of all.slice(-400)) {
      if (!b.command) continue
      const state = !b.end ? 'run' : b.exitCode === 0 ? 'ok' : 'bad'
      const tick = el('button', { class: `tick ${state}`, title: `${state === 'bad' ? `Failed (exit ${b.exitCode})` : state === 'ok' ? 'OK' : 'Running'}: ${b.command}` })
      tick.style.top = `${Math.min(h - 3, (b.prompt.line / total) * h)}px`
      tick.addEventListener('click', () => t.term.scrollToLine(Math.max(0, b.prompt.line - 1)))
      rail.append(tick)
    }
    t.overlay.append(rail)
  }

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
