// Split panes: each tab in the tab bar is a Group of one or more terminals
// laid out as a tree of left/right and top/bottom splits.
import type { Tab, SavedTab } from './tab'
import { el } from './util'

export type Dir = 'row' | 'column' // row: side by side; column: one above the other
type Leaf = { tab: Tab }
type Split = { dir: Dir; ratio: number; a: PaneNode; b: PaneNode }
export type PaneNode = Leaf | Split

export type SavedNode = { tab: SavedTab } | { dir: Dir; ratio: number; a: SavedNode; b: SavedNode }

const isLeaf = (n: PaneNode): n is Leaf => 'tab' in n

export class Group {
  readonly el = el('div', { class: 'group' })
  root: PaneNode
  focused: Tab
  /** Typing in one pane goes to every pane in this tab. */
  broadcast = false

  constructor(parent: HTMLElement, tab: Tab, private onResize: (g: Group) => void) {
    this.root = { tab }
    this.focused = tab
    parent.append(this.el)
    this.render()
  }

  /** The terminals, left to right and top to bottom. */
  tabs(): Tab[] {
    const out: Tab[] = []
    const walk = (n: PaneNode): void => {
      if (isLeaf(n)) out.push(n.tab)
      else { walk(n.a); walk(n.b) }
    }
    walk(this.root)
    return out
  }

  has(tab: Tab) {
    return this.tabs().includes(tab)
  }

  /** Puts fresh next to target (right of it for 'row', below for 'column'). */
  split(target: Tab, fresh: Tab, dir: Dir, ratio = 0.5) {
    const replace = (n: PaneNode): PaneNode => {
      if (isLeaf(n)) return n.tab === target ? { dir, ratio, a: n, b: { tab: fresh } } : n
      return { ...n, a: replace(n.a), b: replace(n.b) }
    }
    this.root = replace(this.root)
    this.render()
  }

  /** Takes a terminal out; returns the one that should get the focus
   * (undefined when the group is now empty). */
  remove(tab: Tab): Tab | undefined {
    const order = this.tabs()
    const i = order.indexOf(tab)
    if (isLeaf(this.root)) return undefined
    const drop = (n: PaneNode): PaneNode => {
      if (isLeaf(n)) return n
      if (isLeaf(n.a) && n.a.tab === tab) return n.b
      if (isLeaf(n.b) && n.b.tab === tab) return n.a
      return { ...n, a: drop(n.a), b: drop(n.b) }
    }
    this.root = drop(this.root)
    this.render()
    const left = this.tabs()
    const next = left[Math.max(0, Math.min(i - 1, left.length - 1))] ?? left[0]
    if (this.focused === tab) this.focused = next
    return next
  }

  /** The neighbour in a direction, by where the panes are on screen. */
  neighbour(from: Tab, dx: number, dy: number): Tab | undefined {
    const r = from.pane.getBoundingClientRect()
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2
    let best: Tab | undefined, bestD = Infinity
    for (const t of this.tabs()) {
      if (t === from) continue
      const o = t.pane.getBoundingClientRect()
      const ox = o.left + o.width / 2, oy = o.top + o.height / 2
      const ahead = dx ? (ox - cx) * dx > 1 && o.bottom > r.top && o.top < r.bottom : (oy - cy) * dy > 1 && o.right > r.left && o.left < r.right
      if (!ahead) continue
      const d = Math.abs(ox - cx) + Math.abs(oy - cy)
      if (d < bestD) { bestD = d; best = t }
    }
    return best
  }

  get count() {
    return this.tabs().length
  }

  snapshot(): SavedNode {
    const save = (n: PaneNode): SavedNode => (isLeaf(n) ? { tab: n.tab.snapshot() } : { dir: n.dir, ratio: n.ratio, a: save(n.a), b: save(n.b) })
    return save(this.root)
  }

  setActive(on: boolean) {
    this.el.classList.toggle('active', on)
  }

  setFocused(tab: Tab) {
    this.focused = tab
    for (const t of this.tabs()) t.pane.classList.toggle('focused', t === tab)
    this.el.classList.toggle('has-split', !isLeaf(this.root))
  }

  render() {
    const build = (n: PaneNode): HTMLElement => {
      if (isLeaf(n)) return n.tab.pane
      const a = build(n.a), b = build(n.b)
      const divider = el('div', { class: 'divider', role: 'separator', 'aria-orientation': n.dir === 'row' ? 'vertical' : 'horizontal' })
      const box = el('div', { class: `split split-${n.dir}` }, a, divider, b)
      const size = () => {
        a.style.flex = `${n.ratio} 1 0`
        b.style.flex = `${1 - n.ratio} 1 0`
      }
      size()
      divider.addEventListener('pointerdown', (e) => {
        e.preventDefault()
        divider.setPointerCapture(e.pointerId)
        box.classList.add('dragging')
        const move = (ev: PointerEvent) => {
          const r = box.getBoundingClientRect()
          const at = n.dir === 'row' ? (ev.clientX - r.left) / r.width : (ev.clientY - r.top) / r.height
          n.ratio = Math.min(0.9, Math.max(0.1, at))
          size()
          this.onResize(this)
        }
        const up = () => {
          divider.removeEventListener('pointermove', move)
          box.classList.remove('dragging')
          this.onResize(this)
        }
        divider.addEventListener('pointermove', move)
        divider.addEventListener('pointerup', up, { once: true })
      })
      // Double-click a divider: back to equal sizes.
      divider.addEventListener('dblclick', () => {
        n.ratio = 0.5
        size()
        this.onResize(this)
      })
      return box
    }
    for (const t of this.tabs()) t.pane.style.flex = ''
    this.el.textContent = ''
    this.el.append(build(this.root))
    this.setFocused(this.has(this.focused) ? this.focused : this.tabs()[0])
    this.onResize(this)
  }
}
