import { SaveSettings, SetAPIKey, HasSavedAPIKey, HasAPIKey } from '../wailsjs/go/main/App'
import { settings } from '../wailsjs/go/models'
import { themes } from './themes'
import { el, toast } from './util'

const fonts = [
  ['"JetBrains Mono", monospace', 'JetBrains Mono (built in)'],
  ['ui-monospace, "SF Mono", Menlo, monospace', 'SF Mono'],
  ['Menlo, monospace', 'Menlo'],
  ['Monaco, monospace', 'Monaco'],
  ['"Fira Code", "JetBrains Mono", monospace', 'Fira Code (if installed)'],
  ['"Hack Nerd Font", "MesloLGS NF", "JetBrains Mono", monospace', 'Nerd Font (if installed)'],
]

const models = [
  ['claude-opus-5-5', 'Claude Opus 5.5 — best answers (default)'],
  ['claude-sonnet-5-5', 'Claude Sonnet 5.5 — faster, cheaper'],
  ['claude-haiku-5-5', 'Claude Haiku 5.5 — fastest, cheapest'],
]

export interface SettingsHost {
  current(): settings.Settings
  preview(s: settings.Settings): void // live preview while the window is open
  saved(s: settings.Settings): void
  closed(): void
}

export class SettingsView {
  private root = document.getElementById('settings')!
  private before: settings.Settings | null = null

  constructor(private host: SettingsHost) {
    this.root.addEventListener('mousedown', (e) => {
      if (e.target === this.root) this.cancel()
    })
  }

  get visible() {
    return !this.root.hidden
  }

  async open() {
    if (this.visible) return
    const s = settings.Settings.createFrom(this.host.current())
    this.before = settings.Settings.createFrom(s)
    const savedKey = await HasSavedAPIKey()
    const anyKey = await HasAPIKey()
    this.root.textContent = ''

    const field = (label: string, control: HTMLElement, help = '') =>
      el('label', { class: 'field' }, el('span', { class: 'label', text: label }), control, ...(help ? [el('span', { class: 'help', text: help })] : []))

    const select = (options: string[][], value: string, onChange: (v: string) => void) => {
      const sel = el('select') as HTMLSelectElement
      let found = false
      for (const [v, label] of options) {
        const o = el('option', { value: v, text: label })
        if (v === value) {
          o.selected = true
          found = true
        }
        sel.append(o)
      }
      if (!found) {
        const o = el('option', { value, text: value })
        o.selected = true
        sel.append(o)
      }
      sel.addEventListener('change', () => onChange(sel.value))
      return sel
    }

    const check = (value: boolean, onChange: (v: boolean) => void) => {
      const c = el('input', { type: 'checkbox' }) as HTMLInputElement
      c.checked = value
      c.addEventListener('change', () => onChange(c.checked))
      return c
    }

    const number = (value: number, min: number, max: number, step: number, onChange: (v: number) => void) => {
      const n = el('input', { type: 'number', min: String(min), max: String(max), step: String(step) }) as HTMLInputElement
      n.value = String(value)
      n.addEventListener('input', () => {
        const v = parseFloat(n.value)
        if (!isNaN(v) && v >= min && v <= max) onChange(v)
      })
      return n
    }

    const text = (value: string, placeholder: string, onChange: (v: string) => void) => {
      const t = el('input', { type: 'text', placeholder, spellcheck: 'false' }) as HTMLInputElement
      t.value = value
      t.addEventListener('input', () => onChange(t.value))
      return t
    }

    const live = () => this.host.preview(s)

    // Theme swatches
    const swatches = el('div', { class: 'swatches' })
    for (const th of themes) {
      const c = th.colors
      const sw = el('button', { class: 'swatch' + (th.id === s.theme ? ' on' : ''), title: th.name, type: 'button' })
      sw.style.background = c.background!
      sw.innerHTML = `<span style="color:${c.foreground}">${th.name}</span><i>${[c.red, c.green, c.yellow, c.blue, c.magenta, c.cyan]
        .map((x) => `<b style="background:${x}"></b>`)
        .join('')}</i>`
      sw.addEventListener('click', () => {
        s.theme = th.id
        swatches.querySelectorAll('.swatch').forEach((x) => x.classList.remove('on'))
        sw.classList.add('on')
        live()
      })
      swatches.append(sw)
    }

    const keyInput = el('input', {
      type: 'password',
      placeholder: savedKey ? '•••••••• saved in your Keychain' : anyKey ? 'Using ANTHROPIC_API_KEY from your environment' : 'sk-ant-…',
      spellcheck: 'false',
      autocomplete: 'off',
    }) as HTMLInputElement
    const removeKey = el('button', { type: 'button', class: 'ghost small' }, 'Remove saved key')
    removeKey.hidden = !savedKey
    removeKey.addEventListener('click', async () => {
      await SetAPIKey('')
      keyInput.placeholder = 'sk-ant-…'
      removeKey.hidden = true
      toast('API key removed')
    })

    const appearance = el(
      'section', {},
      el('h3', { text: 'Appearance' }),
      swatches,
      el('div', { class: 'row' },
        field('Font', select(fonts, s.fontFamily, (v) => { s.fontFamily = v; live() })),
        field('Size', number(s.fontSize, 8, 48, 1, (v) => { s.fontSize = v; live() })),
        field('Line height', number(s.lineHeight, 1, 2, 0.05, (v) => { s.lineHeight = v; live() })),
      ),
      el('div', { class: 'row' },
        field('Cursor', select([['block', 'Block ▇'], ['bar', 'Bar ▏'], ['underline', 'Underline ▁']], s.cursorStyle, (v) => { s.cursorStyle = v; live() })),
        el('label', { class: 'check' }, check(s.cursorBlink, (v) => { s.cursorBlink = v; live() }), ' Blinking cursor'),
      ),
    )

    const shellSec = el(
      'section', {},
      el('h3', { text: 'Shell' }),
      field('Shell', text(s.shell, 'Your login shell (leave empty)', (v) => (s.shell = v.trim())), 'For example /bin/zsh, /bin/bash, /bin/tcsh or /opt/homebrew/bin/fish. Applies to new tabs.'),
      el('div', { class: 'row' },
        field('Scrollback lines', number(s.scrollback, 1000, 200000, 1000, (v) => (s.scrollback = v))),
      ),
      el('label', { class: 'check' }, check(s.shellIntegration, (v) => (s.shellIntegration = v)), ' Shell integration — lets jamalterm find each command (needed for browse mode and AI). zsh, bash and tcsh.'),
      el('label', { class: 'check' }, check(s.confirmRerun, (v) => (s.confirmRerun = v)), ' Ask before re-running a command from browse mode'),
      el('label', { class: 'check' }, check(s.optionIsMeta, (v) => { s.optionIsMeta = v; live() }), ' Use Option as Meta (⌥B / ⌥F jump words)'),
    )

    const aiSec = el(
      'section', {},
      el('h3', { text: 'AI (Claude)' }),
      el('label', { class: 'check' }, check(s.aiEnabled, (v) => (s.aiEnabled = v)), ' Turn on AI features'),
      field('Claude API key', el('div', { class: 'keyrow' }, keyInput, removeKey), 'Get one at console.anthropic.com → API Keys. Stored in your macOS Keychain, never in a file.'),
      field('Model', select(models, s.aiModel, (v) => (s.aiModel = v))),
      el('label', { class: 'check' }, check(s.aiShareOutput, (v) => (s.aiShareOutput = v)), ' Send recent command output with questions (otherwise only the commands)'),
    )

    const cancel = el('button', { type: 'button', class: 'ghost' }, 'Cancel')
    const save = el('button', { type: 'submit', class: 'primary' }, 'Save')
    const form = el('form', { class: 'dialog settings' },
      el('div', { class: 'dialog-head' }, el('h2', { text: 'Settings' })),
      el('div', { class: 'dialog-body' }, appearance, shellSec, aiSec),
      el('div', { class: 'dialog-foot' }, cancel, save),
    )
    cancel.addEventListener('click', () => this.cancel())
    form.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        this.cancel()
      }
    })
    form.addEventListener('submit', async (e) => {
      e.preventDefault()
      try {
        if (keyInput.value.trim()) await SetAPIKey(keyInput.value.trim())
        const saved = await SaveSettings(s)
        this.before = null
        this.close()
        this.host.saved(saved)
        toast('Settings saved')
      } catch (err) {
        toast(String(err), 'error')
      }
    })
    this.root.append(form)
    this.root.hidden = false
    ;(swatches.querySelector('.on') as HTMLElement | null)?.focus()
  }

  cancel() {
    if (this.before) this.host.preview(this.before)
    this.before = null
    this.close()
  }

  private close() {
    this.root.hidden = true
    this.root.textContent = ''
    this.host.closed()
  }
}
