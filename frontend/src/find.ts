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

  constructor(private box: HTMLElement, private active: () => Tab | undefined) {
    box.append(this.input, el('span', { class: 'count' }))
    this.input.addEventListener('input', () => this.active()?.search.findNext(this.input.value, { decorations, incremental: true }))
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') this.close()
      else if (e.key === 'Enter') {
        if (e.shiftKey) this.active()?.search.findPrevious(this.input.value, { decorations })
        else this.active()?.search.findNext(this.input.value, { decorations })
      }
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
