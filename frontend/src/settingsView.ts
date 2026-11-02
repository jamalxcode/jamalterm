import { SaveSettings, SetAPIKey, HasSavedAPIKey, AnthropicAccount, BlurActive, ForgetSession, SetProviderKey, HasSavedProviderKey, ListProviderModels, SignInOpenRouter, ShowProblemLog, FindLocalServers } from './backend'
import { themes } from './themes'
import { providers, providerInfo, modelOf, setModelOf } from './providers'
import { el, toast, SIGN_IN_COMMAND, SIGN_OUT_COMMAND } from './util'
import type { Settings } from './types'

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
  current(): Settings
  preview(s: Settings): void // live preview while the window is open
  saved(s: Settings): void
  closed(): void
  checkUpdates(): void
  runInNewTab(command: string): void
  diagnostics(): void
}

/** The Settings pages, in sidebar order. */
export const settingsPages = [
  ['appearance', 'Appearance', '🎨'],
  ['shell', 'Shell', '⌨'],
  ['ai', 'AI', '✦'],
  ['privacy', 'Privacy', '🔒'],
  ['updates', 'Updates', '↻'],
] as const
export type SettingsPage = (typeof settingsPages)[number][0]

export class SettingsView {
  private root = document.getElementById('settings')!
  private before: Settings | null = null
  private page: SettingsPage = 'appearance'

  constructor(private host: SettingsHost) {
    this.root.addEventListener('mousedown', (e) => {
      if (e.target === this.root) this.cancel()
    })
  }

  get visible() {
    return !this.root.hidden
  }

  /** Opens Settings on a page; find highlights the setting whose label
   * contains that text (used by the command palette). */
  async open(page?: SettingsPage, find?: string) {
    if (this.visible) {
      if (page) this.show(page, find)
      return
    }
    if (page) this.page = page
    const s = { ...this.host.current() }
    this.before = { ...s }
    const account = await AnthropicAccount()
    const savedKey = await HasSavedAPIKey()
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
      placeholder: savedKey ? '•••••••• saved in your Keychain' : 'sk-ant-…  (optional if you signed in)',
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

    // Window opacity: see the desktop through the window.
    const blurNow = await BlurActive()
    const pct = el('span', { class: 'pct', text: `${s.opacity}%` })
    const slider = el('input', { type: 'range', min: '30', max: '100', step: '1', class: 'slider' }) as HTMLInputElement
    slider.value = String(s.opacity)
    const blurNote = el('span', { class: 'help' })
    const updateBlurNote = () => {
      const want = s.blur && s.opacity < 100
      blurNote.textContent = want === blurNow ? '' : 'Blur changes after you restart jamalterm (⌘Q, then open it again).'
    }
    slider.addEventListener('input', () => {
      s.opacity = parseInt(slider.value, 10)
      pct.textContent = `${s.opacity}%`
      this.root.classList.toggle('peek', true) // lighten the dialog backdrop to show the effect
      live()
      updateBlurNote()
    })
    slider.addEventListener('change', () => this.root.classList.remove('peek'))
    const opacityRow = el('div', { class: 'field' },
      el('span', { class: 'label', text: 'Window opacity' }),
      el('div', { class: 'row slider-row' }, el('span', { class: 'dim', text: 'See-through' }), slider, el('span', { class: 'dim', text: 'Solid' }), pct),
      el('label', { class: 'check' }, check(s.blur, (v) => { s.blur = v; updateBlurNote() }), ' Blur what’s behind the window'),
      blurNote,
    )
    updateBlurNote()

    const appearance = el(
      'section', { 'data-page': 'appearance' },
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
      opacityRow,
    )

    const forget = el('button', { type: 'button', class: 'ghost small' }, 'Forget saved tabs and output')
    forget.addEventListener('click', async () => {
      await ForgetSession()
      toast('Saved tabs and output deleted')
    })

    const shellSec = el(
      'section', { 'data-page': 'shell' },
      el('h3', { text: 'Shell' }),
      field('Shell', text(s.shell, 'Your login shell (leave empty)', (v) => (s.shell = v.trim())), 'For example /bin/zsh, /bin/bash, /bin/tcsh or /opt/homebrew/bin/fish. Applies to new tabs.'),
      el('div', { class: 'row' },
        field('Scrollback lines', number(s.scrollback, 1000, 200000, 1000, (v) => (s.scrollback = v))),
      ),
      el('label', { class: 'check' }, check(s.shellIntegration, (v) => (s.shellIntegration = v)), ' Shell integration — lets jamalterm find each command (needed for browse mode and AI). zsh, bash and tcsh.'),
      el('label', { class: 'check' }, check(s.arrowBrowse, (v) => (s.arrowBrowse = v)), ' ↑ at an empty prompt moves the free-floating cursor (off: ↑ goes to shell history; ⌘↑ always browses)'),
      el('label', { class: 'check' }, check(s.restoreSession, (v) => (s.restoreSession = v)), ' Reopen tabs and their recent output after a restart or update'),
      el('label', { class: 'check' }, check(s.confirmRerun, (v) => (s.confirmRerun = v)), ' Ask before re-running a command from browse mode (always asked when it ran in a different folder)'),
      el('div', { class: 'row' }, field('Prompt editing keys', select([['emacs', 'Emacs (default: Ctrl+A, Ctrl+E…)'], ['vi', 'Vi (bindkey -v / set -o vi)']], s.promptKeys, (v) => (s.promptKeys = v)), 'How jamalterm clears your prompt line before typing a command into it.')),
      el('label', { class: 'check' }, check(s.optionIsMeta, (v) => { s.optionIsMeta = v; live() }), ' Use Option as Meta (⌥B / ⌥F jump words)'),
      el('div', { class: 'row', style: 'align-items:center' },
        el('label', { class: 'check' }, check(s.notifyLongCommands, (v) => (s.notifyLongCommands = v)), ' Notify me when a command that takes longer than'),
        number(s.notifyAfterSeconds, 1, 3600, 1, (v) => (s.notifyAfterSeconds = v)),
        el('span', { text: 'seconds finishes while jamalterm is in the background' })),
      el('label', { class: 'check' }, check(s.screenReader, (v) => { s.screenReader = v; live() }), ' Screen reader support (VoiceOver can read the terminal)'),
    )

    // Sign in with Anthropic (OAuth via the ant CLI) — no key to copy.
    const runAndClose = (cmd: string) => {
      this.cancel()
      this.host.runInNewTab(cmd)
    }
    const signInBtn = el('button', { type: 'button', class: 'primary small' }, account.signedIn ? 'Sign in again' : 'Sign in with Anthropic')
    signInBtn.addEventListener('click', () => runAndClose(SIGN_IN_COMMAND))
    const signOutBtn = el('button', { type: 'button', class: 'ghost small' }, 'Sign out')
    signOutBtn.hidden = !account.signedIn
    signOutBtn.addEventListener('click', () => runAndClose(SIGN_OUT_COMMAND))
    const accountText = account.signedIn
      ? `✓ Signed in with your Anthropic account (profile “${account.profile}”${account.workspace ? `, workspace ${account.workspace}` : ''})`
      : 'Not signed in. Opens your browser to sign in to your Anthropic Console account; usage is billed to that account.'
    const accountRow = el('div', { class: 'field' },
      el('span', { class: 'label', text: 'Anthropic account' }),
      el('div', { class: 'keyrow' }, signInBtn, signOutBtn),
      el('span', { class: 'help' + (account.signedIn ? ' ok' : ''), text: accountText }),
    )

    // Claude: sign-in or key, and a model.
    const claudeBox = el('div', { class: 'provider-box' },
      accountRow,
      field('Or use an API key', el('div', { class: 'keyrow' }, keyInput, removeKey), 'From console.anthropic.com → API Keys. Stored in your macOS Keychain, never in a file. A saved key is used instead of the sign-in.'),
      field('Model', select(models, s.aiModel, (v) => (s.aiModel = v))),
    )

    // ChatGPT and DeepSeek: an API key and a model (blank = automatic).
    const compatKeys: Record<string, HTMLInputElement> = {}
    // A model on this Mac: a server address instead of a key.
    const localBox = () => {
      const url = text(s.localUrl, 'http://localhost:11434/v1', (v) => (s.localUrl = v.trim()))
      const datalist = el('datalist', { id: 'models-local' })
      const model = el('input', { type: 'text', list: 'models-local', placeholder: 'Automatic: the first model the server lists', spellcheck: 'false' }) as HTMLInputElement
      model.value = s.localModel
      model.addEventListener('input', () => (s.localModel = model.value.trim()))
      const find = el('button', { type: 'button', class: 'ghost small' }, 'Find on this Mac')
      const status = el('span', { class: 'help', text: 'Questions go only to this server and never leave your Mac. Install Ollama (ollama.com) or LM Studio (lmstudio.ai), then download a model, e.g. ollama pull llama3.2' })
      find.addEventListener('click', async () => {
        status.textContent = 'Looking…'
        const found = (await FindLocalServers()) ?? []
        if (!found.length) {
          status.textContent = 'No Ollama or LM Studio running on this Mac. Start one and try again.'
          return
        }
        const f = found[0]
        url.value = f.url
        s.localUrl = f.url
        datalist.textContent = ''
        for (const m of f.models ?? []) datalist.append(el('option', { value: m }))
        status.textContent = `✓ Found ${found.map((x) => x.name).join(' and ')}. Using ${f.name}: ${(f.models ?? []).length} models (${(f.models ?? []).slice(0, 3).join(', ') || 'none downloaded yet'}).`
        status.classList.add('ok')
      })
      return el('div', { class: 'provider-box' },
        field('Server address', el('div', { class: 'keyrow' }, url, find)),
        field('Model', el('div', { class: 'keyrow' }, model, datalist)),
        status)
    }

    const compatBox = async (id: string) => {
      const p = providerInfo(id)!
      if (p.local) return localBox()
      const info = { name: p.name, company: p.company, url: p.signIn === 'openrouter' ? `${p.keyWhere}, or use Sign in with ${p.name} above` : p.keyWhere, bill: p.billing, auto: `Automatic: ${p.autoModel}` }
      const saved = await HasSavedProviderKey(id)
      const key = el('input', { type: 'password', placeholder: saved ? '•••••••• saved in your Keychain' : p.keyHint, spellcheck: 'false', autocomplete: 'off' }) as HTMLInputElement
      compatKeys[id] = key
      const remove = el('button', { type: 'button', class: 'ghost small' }, 'Remove saved key')
      remove.hidden = !saved
      remove.addEventListener('click', async () => {
        await SetProviderKey(id, '')
        key.placeholder = p.keyHint
        remove.hidden = true
        toast(`${info.name} key removed`)
      })
      const listId = `models-${id}`
      const datalist = el('datalist', { id: listId })
      const modelInput = el('input', { type: 'text', list: listId, placeholder: info.auto, spellcheck: 'false' }) as HTMLInputElement
      modelInput.value = modelOf(s, id)
      modelInput.addEventListener('input', () => setModelOf(s, id, modelInput.value.trim()))
      const load = el('button', { type: 'button', class: 'ghost small' }, 'Show available models')
      const modelHelp = el('span', { class: 'help', text: 'Leave empty for automatic, or pick one your key can use.' })
      load.addEventListener('click', async () => {
        if (key.value.trim()) await SetProviderKey(id, key.value.trim()) // save first so the list can load
        try {
          const list = (await ListProviderModels(id)) ?? []
          datalist.textContent = ''
          for (const m of list) datalist.append(el('option', { value: m }))
          modelHelp.textContent = list.length ? `${list.length} models available — click the field to choose (newest first): ${list.slice(0, 4).join(', ')}…` : 'No chat models found for this key.'
          modelInput.focus()
        } catch (err) {
          modelHelp.textContent = String(err)
        }
      })
      // OpenRouter has a public browser sign-in that hands back a key.
      const signIn: HTMLElement[] = []
      if (p.signIn === 'openrouter') {
        const btn = el('button', { type: 'button', class: 'primary small' }, 'Sign in with OpenRouter')
        const note = el('span', { class: 'help', text: 'Opens your browser; approve, and the key is saved for you. No key to copy.' })
        btn.addEventListener('click', async () => {
          btn.setAttribute('disabled', '')
          note.textContent = 'Waiting for you to approve in the browser…'
          try {
            await SignInOpenRouter()
            note.textContent = '✓ Signed in. Your OpenRouter key is saved in your Keychain.'
            note.classList.add('ok')
            key.placeholder = '•••••••• saved in your Keychain'
            remove.hidden = false
          } catch (err) {
            note.textContent = String(err)
          } finally {
            btn.removeAttribute('disabled')
          }
        })
        signIn.push(el('div', { class: 'field' }, el('span', { class: 'label', text: 'OpenRouter account' }), el('div', { class: 'keyrow' }, btn), note))
      }
      return el('div', { class: 'provider-box' },
        ...signIn,
        field(p.signIn ? 'Or paste an API key' : `${info.company} API key`, el('div', { class: 'keyrow' }, key, remove), `From ${info.url}. Stored in your macOS Keychain. ${info.bill}`),
        field('Model', el('div', { class: 'keyrow' }, modelInput, load, datalist)),
        modelHelp,
      )
    }
    const boxes: Record<string, HTMLElement> = { anthropic: claudeBox }
    for (const p of providers) if (p.id !== 'anthropic') boxes[p.id] = await compatBox(p.id)
    const showBox = () => Object.entries(boxes).forEach(([id, b]) => (b.hidden = id !== s.aiProvider))
    showBox()

    const aiSec = el(
      'section', { 'data-page': 'ai' },
      el('h3', { text: 'AI assistant' }),
      el('label', { class: 'check' }, check(s.aiEnabled, (v) => (s.aiEnabled = v)), ' Turn on AI features'),
      field('Assistant', select(providers.map((p) => [p.id, p.label]), s.aiProvider, (v) => { s.aiProvider = v; showBox() })),
      ...Object.values(boxes),
      el('p', { class: 'help', text: 'What gets sent with questions is under Privacy.' }),
    )

    const checkNow = el('button', { type: 'button', class: 'ghost small' }, 'Check now')
    checkNow.addEventListener('click', () => this.host.checkUpdates())
    const updates = el(
      'section', { 'data-page': 'updates' },
      el('h3', { text: 'Updates' }),
      el('div', { class: 'row' },
        el('label', { class: 'check' }, check(s.autoUpdate, (v) => (s.autoUpdate = v)), ' Check for new versions automatically'),
        checkNow,
      ),
      el('div', { class: 'row' },
        field('Update channel', select([['stable', 'Stable (recommended)'], ['beta', 'Beta: get test versions first']], s.updateChannel, (v) => (s.updateChannel = v)),
          'Beta versions have the newest changes before everyone else, and may have rough edges. You can always go back with jamalterm → Go Back to Previous Version.')),
    )

    // Privacy: what stays on this Mac, and what you can delete.
    const showLog = el('button', { type: 'button', class: 'ghost small' }, 'Show problem log')
    showLog.addEventListener('click', () => ShowProblemLog())
    const diag = el('button', { type: 'button', class: 'ghost small' }, 'Copy diagnostics…')
    diag.addEventListener('click', () => {
      this.cancel()
      this.host.diagnostics()
    })
    const privacy = el(
      'section', { 'data-page': 'privacy' },
      el('h3', { text: 'Privacy' }),
      el('p', { class: 'help', text: 'jamalterm sends nothing anywhere on its own. AI questions go only to the assistant you choose, with secrets hidden first. Your keys stay in the macOS Keychain.' }),
      el('label', { class: 'check' }, check(s.keepProblemLog, (v) => (s.keepProblemLog = v)), ' Keep a private problem log on this Mac (errors only: no commands, output or questions; turning it off deletes it)'),
      el('div', { class: 'row', style: 'margin: 0 0 10px 24px' }, showLog, diag),
      el('label', { class: 'check' }, check(s.aiShareOutput, (v) => (s.aiShareOutput = v)), ' Send recent command output with AI questions (otherwise only the commands)'),
      el('div', { class: 'field' },
        el('span', { class: 'label', text: 'Saved tabs' }),
        el('div', { class: 'keyrow' }, forget),
        el('span', { class: 'help', text: 'To reopen tabs after a restart, jamalterm keeps their recent output in a private file on this Mac (Shell → Reopen tabs).' })),
    )

    const nav = el('nav', { class: 'settings-nav', role: 'tablist' })
    for (const [id, label, icon] of settingsPages) {
      const b = el('button', { type: 'button', role: 'tab', 'data-page': id }, el('span', { class: 'icon', text: icon }), label)
      b.addEventListener('click', () => this.show(id))
      nav.append(b)
    }

    const cancel = el('button', { type: 'button', class: 'ghost' }, 'Cancel')
    const save = el('button', { type: 'submit', class: 'primary' }, 'Save')
    const form = el('form', { class: 'dialog settings' },
      el('div', { class: 'dialog-head' }, el('h2', { text: 'Settings' })),
      el('div', { class: 'settings-main' }, nav, el('div', { class: 'dialog-body settings-pages' }, appearance, shellSec, aiSec, privacy, updates)),
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
        for (const [id, input] of Object.entries(compatKeys)) if (input.value.trim()) await SetProviderKey(id, input.value.trim())
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
    this.show(this.page, find)
    if (this.page === 'appearance' && !find) (swatches.querySelector('.on') as HTMLElement | null)?.focus()
  }

  /** Shows one page; find scrolls to and highlights a setting. */
  show(page: SettingsPage, find?: string) {
    this.page = page
    this.root.querySelectorAll<HTMLElement>('.settings-pages > section').forEach((sec) => (sec.hidden = sec.dataset.page !== page))
    this.root.querySelectorAll<HTMLElement>('.settings-nav button').forEach((b) => {
      const on = b.dataset.page === page
      b.classList.toggle('on', on)
      b.setAttribute('aria-selected', String(on))
    })
    if (!find) return
    const want = find.toLowerCase()
    const sec = this.root.querySelector<HTMLElement>(`.settings-pages > section[data-page="${page}"]`)
    const hit = Array.from(sec?.querySelectorAll<HTMLElement>('.field, .check') ?? []).find((n: HTMLElement) => {
      const label = n.classList.contains('field') ? n.querySelector('.label')?.textContent : n.textContent
      return (label ?? '').toLowerCase().includes(want)
    })
    if (!hit) return
    hit.scrollIntoView({ block: 'center' })
    hit.classList.remove('flash')
    void hit.offsetWidth
    hit.classList.add('flash')
    hit.querySelector<HTMLElement>('input, select, button')?.focus()
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
