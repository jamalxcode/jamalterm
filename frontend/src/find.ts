// The find bar (⌘F): searches the focused pane's output.
import type { Tab } from './tab'
import { el } from './util'

const decorations = {
  matchBackground: '#ffc77755',
  activeMatchBackground: '#ff8fb1aa',
  matchOverviewRuler: '#ffc777',
  activeMatchColorOverviewRuler: '#ff8fb1',
}

export class FindBar {
  private input = el('input', { type: 'text', placeholder: 'Find', spellcheck: 'false', 'aria-label': 'Find in output' }) as HTMLInputElement
  private count = el('span', { class: 'count', role: 'status' })
  private watching = new WeakSet<Tab>()

  constructor(private box: HTMLElement, private active: () => Tab | undefined) {
    box.append(this.input, this.count)
    this.input.addEventListener('input', () => {
      this.watch()
      if (!this.input.value) this.count.textContent = ''
      this.active()?.search.findNext(this.input.value, { decorations, incremental: true })
    })
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') this.close()
      else if (e.key === 'Enter') {
        if (e.shiftKey) this.active()?.search.findPrevious(this.input.value, { decorations })
        else this.active()?.search.findNext(this.input.value, { decorations })
      }
    })
  }

  /** Shows "2 of 7" for the pane being searched. */
  private watch() {
    const t = this.active()
    if (!t || this.watching.has(t)) return
    this.watching.add(t)
    t.search.onDidChangeResults(({ resultIndex, resultCount }) => {
      if (this.active() !== t) return
      this.count.textContent = !this.input.value ? '' : resultCount === 0 ? 'No matches' : `${resultIndex + 1} of ${resultCount}`
    })
  }

  open() {
    this.box.hidden = false
    this.input.select()
    this.input.focus()
  }

  close() {
    if (this.box.hidden) return
    this.box.hidden = true
    this.active()?.search.clearDecorations()
    this.active()?.term.focus()
  }
}
