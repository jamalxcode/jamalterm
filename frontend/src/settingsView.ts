import { SetAPIKey, HasSavedAPIKey, AnthropicAccount, BlurActive, ForgetSession, SetProviderKey, HasSavedProviderKey, ListProviderModels, SignInOpenRouter, ShowProblemLog, FindLocalServers, KeychainKeysWaiting, MoveKeysFromKeychain, KeysUnreadable, UpdateStatus, CheckForUpdate } from './backend'
import type { UpdateStatus as UpdateStatusT } from '../bindings/jamalterm/models'
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

// Settings lives in its own window (settings.html, settingsPage.ts) and,
// as on the Mac, every change applies at once: there's no Save button.
export interface SettingsHost {
  current(): Settings
  save(s: Settings): void // called (debounced) after each change
  close(): void
  checkUpdates(): void
  runInNewTab(command: string): void
  diagnostics(): void
  setTitle(title: string): void // the window title names the pane
}

/** The Settings panes, in toolbar order: [id, title, SF-Symbol-like icon]. */
export const settingsPages = [
  ['appearance', 'Appearance', 'paintpalette'],
  ['shell', 'Shell', 'terminal'],
  ['ai', 'AI', 'sparkles'],
  ['privacy', 'Privacy', 'hand.raised'],
  ['updates', 'Updates', 'arrow.triangle.2.circlepath'],
] as const

// Simple line icons in the style of SF Symbols (drawn as SVG, since web
// pages can't use the system's symbols).
const icons: Record<string, string> = {
  paintpalette: '<path d="M12 3a9 9 0 1 0 0 18c1.1 0 1.6-.8 1.6-1.6 0-.5-.2-.8-.4-1.1-.3-.3-.4-.7-.4-1.1 0-.9.7-1.6 1.6-1.6H16a5 5 0 0 0 5-5c0-4.2-4-7.6-9-7.6Z"/><circle cx="7.5" cy="11" r="1.2"/><circle cx="10" cy="7" r="1.2"/><circle cx="14.5" cy="7" r="1.2"/><circle cx="17" cy="11" r="1.2"/>',
  terminal: '<rect x="3" y="4.5" width="18" height="15" rx="3"/><path d="m7.5 9.5 3 2.5-3 2.5M12.5 15h4"/>',
  sparkles: '<path d="M10 3.5 11.6 8 16 9.6 11.6 11.2 10 15.7 8.4 11.2 4 9.6 8.4 8Z"/><path d="M17.5 13.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8Z"/>',
  'hand.raised': '<path d="M8 12V6.5a1.5 1.5 0 0 1 3 0V11V4.8a1.5 1.5 0 0 1 3 0V11V6a1.5 1.5 0 0 1 3 0v7.5c0 4-2.5 6.5-6 6.5-2.5 0-3.8-1.2-5.3-3.4L4 13.3a1.4 1.4 0 0 1 2.2-1.7L8 13.5"/>',
  'arrow.triangle.2.circlepath': '<path d="M5 12a7 7 0 0 1 12-4.9L19 9M19 4.5V9h-4.5M19 12a7 7 0 0 1-12 4.9L5 15M5 19.5V15h4.5"/>',
}
const icon = (name: string) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${icons[name] ?? ''}</svg>`
export type SettingsPage = (typeof settingsPages)[number][0]

export class SettingsView {
  private page: SettingsPage = 'appearance'
  private saveTimer = 0
  /** True while something here was changed and not yet saved. */
  pending = false

  constructor(private host: SettingsHost, private root: HTMLElement) {}

  /** Builds the window on a pane; find highlights the setting whose label
   * contains that text (used by the command palette). */
  async open(page?: SettingsPage, find?: string) {
    if (page) this.page = page
    const s = { ...this.host.current() }
    // Every change is saved shortly after it's made (no Save button).
    const changed = () => {
      this.pending = true
      clearTimeout(this.saveTimer)
      this.saveTimer = window.setTimeout(() => {
        this.pending = false
        this.host.save({ ...s })
      }, 200)
    }
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

    const live = changed

    // Settings most people never touch sit under a closed "Advanced"
    // disclosure at the bottom of each pane (opened by search when needed).
    const advanced = (...children: HTMLElement[]) =>
      el('details', { class: 'advanced' }, el('summary', { text: 'Advanced' }), ...children)

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
      placeholder: savedKey ? '•••••••• saved (encrypted on this Mac)' : 'sk-ant-…  (optional if you signed in)',
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
      live()
      updateBlurNote()
    })
    const opacityRow = el('div', { class: 'field' },
      el('span', { class: 'label', text: 'Window opacity' }),
      el('div', { class: 'row slider-row' }, el('span', { class: 'dim', text: 'See-through' }), slider, el('span', { class: 'dim', text: 'Solid' }), pct),
      el('label', { class: 'check' }, check(s.blur, (v) => { s.blur = v; updateBlurNote() }), ' Blur what’s behind the window'),
      blurNote,
    )
    updateBlurNote()

    // Host colors: tabs connected to matching machines get a colored edge
    // and the machine's name.
    const hostColorsField = () => {
      const colors = ['red', 'orange', 'yellow', 'green', 'blue', 'purple']
      const list = el('div', { class: 'host-rules' })
      const rows = () => {
        list.textContent = ''
        ;(s.hostColors ??= []).forEach((h, i) => {
          const pat = el('input', { type: 'text', placeholder: 'prod-*, *.example.com', spellcheck: 'false', 'aria-label': 'Machine names' }) as HTMLInputElement
          pat.value = h.pattern
          pat.addEventListener('input', () => (h.pattern = pat.value))
          const col = select(colors.map((c) => [c, c[0].toUpperCase() + c.slice(1)]), h.color, (v) => (h.color = v))
          col.setAttribute('aria-label', 'Color')
          const dot = el('span', { class: 'host-swatch' })
          dot.style.background = `var(--${h.color})`
          col.addEventListener('change', () => (dot.style.background = `var(--${col.value})`))
          const del = el('button', { type: 'button', class: 'ghost small', title: 'Remove' }, '✕')
          del.addEventListener('click', () => { s.hostColors?.splice(i, 1); rows() })
          list.append(el('div', { class: 'keyrow' }, pat, dot, col, del))
        })
      }
      rows()
      const add = el('button', { type: 'button', class: 'ghost small' }, '+ Add a machine')
      add.addEventListener('click', () => { (s.hostColors ??= []).push({ pattern: '', color: 'red' }); rows() })
      return el('div', { class: 'field' },
        el('span', { class: 'label', text: 'Host colors' }),
        list, el('div', {}, add),
        el('span', { class: 'help', text: 'Tabs connected (ssh) to a matching machine get this color and show its name, e.g. red for production.' }))
    }

    const appearance = el(
      'section', { 'data-page': 'appearance' },
      el('h3', { text: 'Appearance' }),
      swatches,
      el('div', { class: 'row' },
        field('Font', select(fonts, s.fontFamily, (v) => { s.fontFamily = v; live() })),
        field('Size', number(s.fontSize, 8, 48, 1, (v) => { s.fontSize = v; live() })),
      ),
      el('div', { class: 'row' },
        field('Cursor', select([['block', 'Block ▇'], ['bar', 'Bar ▏'], ['underline', 'Underline ▁']], s.cursorStyle, (v) => { s.cursorStyle = v; live() })),
        el('label', { class: 'check' }, check(s.cursorBlink, (v) => { s.cursorBlink = v; live() }), ' Blinking cursor'),
      ),
      opacityRow,
      el('label', { class: 'check' }, check(s.showTabBar, (v) => { s.showTabBar = v; live() }), ' Always show the tab bar (otherwise it appears when there are two tabs, as in Terminal)'),
      advanced(
      field('Line height', number(s.lineHeight, 1, 2, 0.05, (v) => { s.lineHeight = v; live() })),
      field('Window frame', select([['system', 'Match macOS (light or dark, your accent color)'], ['theme', 'Match the terminal theme']], s.windowFrame, (v) => { s.windowFrame = v; live() }),
        'The tab bar, panels and dialogs around the terminal. The terminal itself always uses the theme above.'),
      el('label', { class: 'check' }, check(s.ligatures, (v) => { s.ligatures = v; live() }), ' Font ligatures (=> != ->) in fonts that have them, like JetBrains Mono and Fira Code (drawn without the GPU)'),
      hostColorsField(),
      ),
    )

    const forget = el('button', { type: 'button', class: 'ghost small' }, 'Forget saved tabs and output')
    forget.addEventListener('click', async () => {
      await ForgetSession()
      toast('Saved tabs and output deleted')
    })

    const shellSec = el(
      'section', { 'data-page': 'shell' },
      el('h3', { text: 'Shell' }),
      el('label', { class: 'check' }, check(s.arrowBrowse, (v) => (s.arrowBrowse = v)), ' ↑ at an empty prompt moves the free-floating cursor (off: ↑ goes to shell history; ⌘↑ always browses)'),
      el('label', { class: 'check' }, check(s.restoreSession, (v) => (s.restoreSession = v)), ' Reopen tabs and their recent output after a restart or update'),
      el('label', { class: 'check' }, check(s.confirmRerun, (v) => (s.confirmRerun = v)), ' Ask before re-running a command from browse mode (always asked when it ran in a different folder)'),
      el('label', { class: 'check' }, check(s.optionIsMeta, (v) => { s.optionIsMeta = v; live() }), ' Use Option as Meta (⌥B / ⌥F jump words)'),
      el('div', { class: 'row', style: 'align-items:center' },
        el('label', { class: 'check' }, check(s.notifyLongCommands, (v) => (s.notifyLongCommands = v)), ' Notify me when a command that takes longer than'),
        number(s.notifyAfterSeconds, 1, 3600, 1, (v) => (s.notifyAfterSeconds = v)),
        el('span', { text: 'seconds finishes while jamalterm is in the background' })),
      el('label', { class: 'check' }, check(s.screenReader, (v) => { s.screenReader = v; live() }), ' Screen reader support (VoiceOver can read the terminal)'),
      advanced(
        field('Shell', text(s.shell, 'Your login shell (leave empty)', (v) => (s.shell = v.trim())), 'For example /bin/zsh, /bin/bash, /bin/tcsh or /opt/homebrew/bin/fish. Applies to new tabs.'),
        el('div', { class: 'row' },
          field('Scrollback lines', number(s.scrollback, 1000, 200000, 1000, (v) => (s.scrollback = v))),
        ),
        el('label', { class: 'check' }, check(s.shellIntegration, (v) => (s.shellIntegration = v)), ' Shell integration — lets jamalterm find each command (needed for browse mode and AI). zsh, bash, tcsh and fish.'),
        el('div', { class: 'row' }, field('Prompt editing keys', select([['emacs', 'Emacs (default: Ctrl+A, Ctrl+E…)'], ['vi', 'Vi (bindkey -v / set -o vi)']], s.promptKeys, (v) => (s.promptKeys = v)), 'How jamalterm clears your prompt line before typing a command into it.')),
      ),
    )

    // Sign in with Anthropic (OAuth via the ant CLI) — no key to copy.
    const runAndClose = (cmd: string) => {
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
      field('Or use an API key', el('div', { class: 'keyrow' }, keyInput, removeKey), 'From console.anthropic.com → API Keys. Stored encrypted on this Mac, readable only by you. A saved key is used instead of the sign-in.'),
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
      const key = el('input', { type: 'password', placeholder: saved ? '•••••••• saved (encrypted on this Mac)' : p.keyHint, spellcheck: 'false', autocomplete: 'off' }) as HTMLInputElement
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
            note.textContent = '✓ Signed in. Your OpenRouter key is saved, encrypted, on this Mac.'
            note.classList.add('ok')
            key.placeholder = '•••••••• saved (encrypted on this Mac)'
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
        field(p.signIn ? 'Or paste an API key' : `${info.company} API key`, el('div', { class: 'keyrow' }, key, remove), `From ${info.url}. Stored encrypted on this Mac. ${info.bill}`),
        field('Model', el('div', { class: 'keyrow' }, modelInput, load, datalist)),
        modelHelp,
      )
    }
    const boxes: Record<string, HTMLElement> = { anthropic: claudeBox }
    for (const p of providers) if (p.id !== 'anthropic') boxes[p.id] = await compatBox(p.id)
    const showBox = () => Object.entries(boxes).forEach(([id, b]) => (b.hidden = id !== s.aiProvider))
    showBox()

    // Keys an older version left in the Keychain: moved only if you ask
    // (reading them makes macOS ask for your password one last time).
    const keyNotes: HTMLElement[] = []
    if (await KeychainKeysWaiting()) {
      const move = el('button', { type: 'button', class: 'primary small' }, 'Move Keys from Keychain…')
      const note = el('span', { class: 'help', text: 'An older version of jamalterm kept your API keys in the macOS Keychain. jamalterm no longer uses it. Move them into jamalterm’s own encrypted file (macOS asks for your password one last time), or just enter your keys again below.' })
      const card = el('div', { class: 'provider-box notice' }, note, el('div', { class: 'keyrow' }, move))
      move.addEventListener('click', async () => {
        try {
          const n = await MoveKeysFromKeychain()
          note.textContent = n ? `✓ Moved ${n} key${n === 1 ? '' : 's'}. The Keychain item is deleted; jamalterm won’t ask for your password again.` : '✓ Nothing left to move. The Keychain item is deleted.'
          note.classList.add('ok')
          move.remove()
        } catch (err) {
          note.textContent = String(err)
        }
      })
      keyNotes.push(card)
    }
    if (await KeysUnreadable()) {
      keyNotes.push(el('div', { class: 'provider-box notice' }, el('span', { class: 'help', text: 'Your saved API keys can’t be unlocked on this Mac (the settings folder was probably copied from another computer). Enter them again below.' })))
    }

    const aiSec = el(
      'section', { 'data-page': 'ai' },
      el('h3', { text: 'AI assistant' }),
      ...keyNotes,
      el('label', { class: 'check' }, check(s.aiEnabled, (v) => (s.aiEnabled = v)), ' Turn on AI features'),
      field('Assistant', select(providers.map((p) => [p.id, p.label]), s.aiProvider, (v) => { s.aiProvider = v; showBox() })),
      ...Object.values(boxes),
      el('p', { class: 'help', text: 'What gets sent with questions is under Privacy.' }),
    )

    // What the last check found, always visible, and a check right here.
    const checkNow = el('button', { type: 'button', class: 'ghost small' }, 'Check Now')
    const status = el('div', { class: 'update-status', role: 'status' })
    const showStatus = (st: UpdateStatusT, checking = false) => {
      const when = st.checkedAt ? new Date(st.checkedAt).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }) : ''
      status.textContent = ''
      status.append(el('strong', { text: `jamalterm ${st.current}` }), ' · ')
      if (checking) status.append('Checking…')
      else if (!st.checkedAt) status.append('Not checked yet')
      else if (st.error) status.append(el('span', { class: 'bad', text: `Couldn’t check (${when}): ${st.error}` }))
      else if (st.available) {
        const show = el('button', { type: 'button', class: 'primary small' }, `Install ${st.latest}…`)
        show.addEventListener('click', () => this.host.checkUpdates())
        status.append(el('span', { class: 'ok', text: `${st.latest} is available (checked ${when}). ` }), show)
      } else status.append(el('span', { class: 'ok', text: `You’re up to date (checked ${when}).` }))
    }
    void UpdateStatus().then((st) => showStatus(st))
    checkNow.addEventListener('click', async () => {
      showStatus(await UpdateStatus(), true)
      try { await CheckForUpdate() } catch { /* shown below */ }
      showStatus(await UpdateStatus())
    })
    const updates = el(
      'section', { 'data-page': 'updates' },
      el('h3', { text: 'Updates' }),
      status,
      el('div', { class: 'row' },
        el('label', { class: 'check' }, check(s.autoUpdate, (v) => (s.autoUpdate = v)), ' Check for new versions automatically'),
        checkNow,
      ),
      advanced(el('div', { class: 'row' },
        field('Update channel', select([['stable', 'Stable (recommended)'], ['beta', 'Beta: get test versions first']], s.updateChannel, (v) => (s.updateChannel = v)),
          'Beta versions have the newest changes before everyone else, and may have rough edges. You can always go back with jamalterm → Go Back to Previous Version.'))),
    )

    // Privacy: what stays on this Mac, and what you can delete.
    const showLog = el('button', { type: 'button', class: 'ghost small' }, 'Show problem log')
    showLog.addEventListener('click', () => ShowProblemLog())
    const diag = el('button', { type: 'button', class: 'ghost small' }, 'Copy diagnostics…')
    diag.addEventListener('click', () => this.host.diagnostics())
    const privacy = el(
      'section', { 'data-page': 'privacy' },
      el('h3', { text: 'Privacy' }),
      el('p', { class: 'help', text: 'jamalterm sends nothing anywhere on its own. AI questions go only to the assistant you choose, with secrets hidden first. Your API keys are kept in an encrypted file on this Mac that only your account can open, and that a copy can’t unlock on another computer; jamalterm never uses the Keychain, so it never asks for your password.' }),
      el('label', { class: 'check' }, check(s.aiShareOutput, (v) => (s.aiShareOutput = v)), ' Send recent command output with AI questions (otherwise only the commands)'),
      el('div', { class: 'field' },
        el('span', { class: 'label', text: 'Saved tabs' }),
        el('div', { class: 'keyrow' }, forget),
        el('span', { class: 'help', text: 'To reopen tabs after a restart, jamalterm keeps their recent output in a private file on this Mac (Shell → Reopen tabs).' })),
      advanced(
        el('label', { class: 'check' }, check(s.keepProblemLog, (v) => (s.keepProblemLog = v)), ' Keep a private problem log on this Mac (errors only: no commands, output or questions; turning it off deletes it)'),
        el('div', { class: 'row', style: 'margin: 0 0 10px 24px' }, showLog, diag),
      ),
    )

    // A toolbar of panes across the top, like Terminal's and Safari's.
    const nav = el('nav', { class: 'settings-toolbar', role: 'tablist', 'aria-label': 'Settings' })
    for (const [id, label, ic] of settingsPages) {
      const b = el('button', { type: 'button', role: 'tab', 'data-page': id })
      b.innerHTML = `${icon(ic)}<span>${label}</span>`
      b.addEventListener('click', () => this.show(id))
      nav.append(b)
    }

    // Any change to a field saves (keys are saved, encrypted, when you
    // leave the field).
    const body = el('div', { class: 'settings-pages' }, appearance, shellSec, aiSec, privacy, updates)
    body.addEventListener('input', (e) => {
      if ((e.target as HTMLInputElement).type !== 'password') changed()
    })
    body.addEventListener('change', (e) => {
      if ((e.target as HTMLInputElement).type !== 'password') changed()
    })
    const saveKey = async (input: HTMLInputElement, save: (v: string) => Promise<void>, then: () => void) => {
      const v = input.value.trim()
      if (!v) return
      try {
        await save(v)
        input.value = ''
        input.placeholder = '•••••••• saved (encrypted on this Mac)'
        then()
        toast('Key saved (encrypted on this Mac)')
      } catch (err) {
        toast(String(err), 'error')
      }
    }
    keyInput.addEventListener('change', () => void saveKey(keyInput, SetAPIKey, () => (removeKey.hidden = false)))
    for (const [id, input] of Object.entries(compatKeys)) {
      input.addEventListener('change', () => void saveKey(input, (v) => SetProviderKey(id, v), () => undefined))
    }
    this.root.textContent = ''
    this.root.append(nav, body)
    this.root.onkeydown = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        this.host.close()
      }
    }
    this.show(this.page, find)
    if (this.page === 'appearance' && !find) (swatches.querySelector('.on') as HTMLElement | null)?.focus()
  }

  /** Shows one page; find scrolls to and highlights a setting. */
  show(page: SettingsPage, find?: string) {
    this.page = page
    this.host.setTitle(settingsPages.find((p) => p[0] === page)?.[1] ?? 'Settings')
    this.root.querySelectorAll<HTMLElement>('.settings-pages > section').forEach((sec) => (sec.hidden = sec.dataset.page !== page))
    this.root.querySelectorAll<HTMLElement>('.settings-toolbar button').forEach((b) => {
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
    const fold = hit.closest('details')
    if (fold) fold.open = true
    hit.scrollIntoView({ block: 'center' })
    hit.classList.remove('flash')
    void hit.offsetWidth
    hit.classList.add('flash')
    hit.querySelector<HTMLElement>('input, select, button')?.focus()
  }
}
