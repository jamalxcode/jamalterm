import { CommandRisk, AIAsk, AICancel, AIReset, HasAPIKey, AIToolResult, SetAIProvider, SignInOpenRouter } from './backend'
import { EventsOn } from './backend'
import type { Tab, Block } from './tab'
import { providers, providerInfo } from './providers'
import { el, renderMarkdown, copyText, toast, SIGN_IN_COMMAND } from './util'

type Risk = { level: string; reasons: string[] | null }
type ToolEvent = { id: string; toolId: string; command: string; run: boolean; risk?: Risk }
type Usage = { id: string; model: string; input: number; output: number }
type Handler = { delta(t: string): void; done(t: string): void; error(e: string): void; tool?(e: ToolEvent): void; redacted?(n: number): void; usage?(u: Usage): void }
const handlers = new Map<string, Handler>()
let seq = 0

EventsOn('ai:delta', (e: { id: string; text: string }) => handlers.get(e.id)?.delta(e.text))
EventsOn('ai:usage', (u: Usage) => handlers.get(u.id)?.usage?.(u))
EventsOn('ai:redacted', (e: { id: string; count: number }) => handlers.get(e.id)?.redacted?.(e.count))
EventsOn('ai:tool', (e: ToolEvent) => {
  const h = handlers.get(e.id)
  if (h?.tool) h.tool(e)
  else AIToolResult(e.toolId, 'The terminal window was not available.', true)
})
EventsOn('ai:done', (e: { id: string; text: string }) => {
  handlers.get(e.id)?.done(e.text)
  handlers.delete(e.id)
})
EventsOn('ai:error', (e: { id: string; error: string }) => {
  handlers.get(e.id)?.error(e.error)
  handlers.delete(e.id)
})

function ask(chat: string, mode: 'chat' | 'command', prompt: string, context: string, shell: string, h: Handler): string {
  const id = `q${++seq}`
  handlers.set(id, h)
  AIAsk({ id, chat, mode, prompt, context, shell })
  return id
}

export interface AIHost {
  activeTab(): Tab | undefined
  shellName(): string
  openSettings(): void
  /** The assistant answering: provider id, display name and model label. */
  assistant(): { id: string; name: string; model: string }
  runInNewTab(command: string): void
}

function fmtTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n)
}

// ---- Side panel (⌘I) ------------------------------------------------------------

export class AIPanel {
  private root = document.getElementById('ai-panel')!
  private list = el('div', { class: 'ai-list' })
  private input = el('textarea', { rows: '1', placeholder: 'Ask about your terminal…  (⏎ to send)' }) as HTMLTextAreaElement
  private sendBtn = el('button', { class: 'ai-send', title: 'Send' }, '↑')
  private modelLabel = el('span', { class: 'ai-model' })
  // The assistant's name is also a switcher (Claude / ChatGPT / DeepSeek).
  private shareBox = el('input', { type: 'checkbox' }) as HTMLInputElement
  private shareLabel = el('label', { class: 'share-toggle', title: 'When off, only the commands are sent with questions — not their output' })
  private titleEl = el('select', { class: 'ai-title ai-switch', title: 'Switch assistant (⌥⌘1, ⌥⌘2, …)' }) as HTMLSelectElement
  private streaming: string | null = null
  private chatOf = new Map<number, HTMLElement[]>() // per-tab conversation
  private shownTab = -1

  constructor(private host: AIHost) {
    const newBtn = el('button', { class: 'ghost', title: 'Start a new conversation' }, 'New chat')
    const closeBtn = el('button', { class: 'ghost icon', title: 'Close (esc)' }, '✕')
    const head = el('div', { class: 'ai-head' },
      el('span', { class: 'ai-badge', text: '✦' }),
      el('div', { class: 'ai-names' }, this.titleEl, el('span', { class: 'ai-sub' }, 'AI assistant · ', this.modelLabel)),
      el('span', { class: 'grow' }), newBtn, closeBtn)
    const form = el('div', { class: 'ai-input' }, this.input, this.sendBtn)
    // Per-tab: keep this tab's output out of AI requests.
    this.shareBox.addEventListener('change', () => {
      const tab = this.host.activeTab()
      if (tab) tab.shareOutput = this.shareBox.checked
      this.shareLabel.classList.toggle('off', !this.shareBox.checked)
    })
    this.shareLabel.append(this.shareBox, el('span', { text: ' Share this tab’s output' }))
    this.root.append(head, this.list, form, el('div', { class: 'ai-foot' }, this.shareLabel, el('span', { class: 'dim', text: 'Secrets are hidden automatically' })))

    newBtn.addEventListener('click', () => this.newChat())
    this.titleEl.addEventListener('change', () => void SetAIProvider(this.titleEl.value))
    closeBtn.addEventListener('click', () => this.hide())
    this.sendBtn.addEventListener('click', () => (this.streaming ? AICancel(this.streaming) : this.send()))
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
        e.preventDefault()
        this.send()
      } else if (e.key === 'Escape') {
        e.preventDefault()
        this.hide()
      }
    })
    this.input.addEventListener('input', () => this.autosize())
  }

  get visible() {
    return !this.root.hidden
  }

  /** Show the active assistant and model (after a switch from anywhere). */
  async refreshHeader() {
    if (!this.titleEl.options.length) {
      for (const p of providers) this.titleEl.append(el('option', { value: p.id, text: p.name }))
    }
    const who = this.host.assistant()
    if (this.titleEl.value !== who.id) {
      this.titleEl.value = who.id
      // The welcome and setup hints were for the previous assistant.
      this.list.querySelector('.ai-setup')?.remove()
      if (this.list.querySelector('.ai-welcome')) {
        this.list.querySelector('.ai-welcome')!.remove()
        this.welcome()
      }
      if (this.list.querySelector('.msg')) {
        this.list.append(el('div', { class: 'ai-divider', text: `Switched to ${who.name} — it starts fresh and doesn't see the messages above` }))
        this.scroll()
      }
      if (this.visible && !(await HasAPIKey())) this.showSetup()
    }
    this.modelLabel.textContent = who.model
  }

  async show(focus = true) {
    this.root.hidden = false
    document.body.classList.add('ai-open')
    this.refreshHeader()
    this.syncTab()
    if (!(await HasAPIKey())) this.showSetup()
    if (focus) this.input.focus()
    window.dispatchEvent(new Event('resize'))
  }

  hide() {
    this.root.hidden = true
    document.body.classList.remove('ai-open')
    window.dispatchEvent(new Event('resize'))
    this.host.activeTab()?.term.focus()
  }

  toggle() {
    if (this.visible) this.hide()
    else void this.show()
  }

  /** Each tab has its own conversation; switch the list when tabs change. */
  syncTab() {
    const tab = this.host.activeTab()
    if (tab) {
      this.shareBox.checked = tab.shareOutput
      this.shareLabel.classList.toggle('off', !tab.shareOutput)
    }
    if (!tab || tab.id === this.shownTab) return
    if (this.shownTab >= 0) this.chatOf.set(this.shownTab, Array.from(this.list.children) as HTMLElement[])
    this.shownTab = tab.id
    this.list.textContent = ''
    const saved = this.chatOf.get(tab.id)
    if (saved?.length) this.list.append(...saved)
    else this.welcome()
  }

  forgetTab(id: number) {
    this.chatOf.delete(id)
    AIReset(`tab-${id}`)
  }

  private newChat() {
    const tab = this.host.activeTab()
    if (!tab) return
    if (this.streaming) AICancel(this.streaming)
    AIReset(`tab-${tab.id}`)
    this.list.textContent = ''
    this.welcome()
    this.input.focus()
  }

  private welcome() {
    const card = el('div', { class: 'ai-welcome' })
    card.innerHTML = `<p><strong>Ask ${this.host.assistant().name} about anything in this tab.</strong> It sees your recent commands and their output.</p>
      <p class="dim">Try: “why did that fail?”, “undo my last git commit”, “what's using port 3000?”</p>`
    this.list.append(card)
  }

  private showSetup() {
    if (this.list.querySelector('.ai-setup')) return
    const who = this.host.assistant()
    if (who.id === 'openrouter') {
      const signIn = el('button', { class: 'primary' }, 'Sign in with OpenRouter')
      const status = el('p', { class: 'dim', text: 'Opens your browser — approve and the key is saved for you. Or paste a key from openrouter.ai/keys in Settings.' })
      const useKey = el('button', { class: 'ghost' }, 'Use an API key')
      useKey.addEventListener('click', () => this.host.openSettings())
      signIn.addEventListener('click', async () => {
        status.textContent = 'Waiting for you to approve in the browser…'
        try {
          await SignInOpenRouter()
          this.list.querySelector('.ai-setup')?.remove()
          toast('Signed in to OpenRouter')
          this.input.focus()
        } catch (err) {
          status.textContent = String(err)
        }
      })
      this.list.append(el('div', { class: 'ai-setup' }, el('p', {}, el('strong', { text: 'Connect OpenRouter to turn on AI.' })), status, el('div', { class: 'row' }, signIn, useKey)))
      return
    }
    if (who.id !== 'anthropic') {
      const open = el('button', { class: 'primary' }, 'Open Settings')
      open.addEventListener('click', () => this.host.openSettings())
      const info = providerInfo(who.id)
      const company = info ? `${info.company} (${info.keyWhere})` : who.name
      this.list.append(el('div', { class: 'ai-setup' },
        el('p', {}, el('strong', { text: `Add your ${who.name} API key to turn on AI.` })),
        el('p', { class: 'dim', text: `Create a key at ${company}, then paste it in Settings → AI assistant. It's stored encrypted on this Mac, readable only by you.` }),
        open))
      return
    }
    const signIn = el('button', { class: 'primary' }, 'Sign in with Anthropic')
    signIn.addEventListener('click', () => this.host.runInNewTab(SIGN_IN_COMMAND))
    const useKey = el('button', { class: 'ghost' }, 'Use an API key')
    useKey.addEventListener('click', () => this.host.openSettings())
    const card = el(
      'div',
      { class: 'ai-setup' },
      el('p', {}, el('strong', { text: 'Connect Claude to turn on AI.' })),
      el('p', { class: 'dim', text: 'Sign in with your Anthropic Console account in the browser — no key to copy. Or paste an API key in Settings.' }),
      el('div', { class: 'row' }, signIn, useKey),
    )
    this.list.append(card)
  }

  private autosize() {
    this.input.style.height = 'auto'
    this.input.style.height = `${Math.min(this.input.scrollHeight, 160)}px`
  }

  /** "Why did this fail?" from a red dot, browse mode or the AI menu. */
  async explain(tab: Tab, block: Block) {
    await this.show(false)
    const status = block.end ? (block.exitCode === 0 ? 'succeeded' : `failed with exit code ${block.exitCode}`) : 'is still running'
    const q = block.end && block.exitCode !== 0
      ? `Why did \`${block.command}\` fail (exit ${block.exitCode}), and how do I fix it?`
      : `Explain what \`${block.command}\` did and what its output means. (It ${status}.)`
    this.send(q, tab.aiContext(block))
  }

  send(text?: string, context?: string) {
    const tab = this.host.activeTab()
    if (!tab || this.streaming) return
    const prompt = (text ?? this.input.value).trim()
    if (!prompt) return
    if (text === undefined) {
      this.input.value = ''
      this.autosize()
    }
    this.list.querySelector('.ai-welcome')?.remove()
    this.list.append(el('div', { class: 'msg user', text: prompt }))
    const body = el('div', { class: 'msg-body' })
    const msg = el('div', { class: 'msg assistant thinking' }, body)
    this.list.append(msg)
    this.scroll()

    // The answer is a list of parts: text, then a tool card, then more text…
    let acc = ''
    let totalIn = 0, totalOut = 0, usedModel = ''
    const meta = el('div', { class: 'msg-meta' })
    let part: HTMLElement | null = null
    let sawText = false
    const render = () => {
      if (!part) {
        part = el('div', { class: 'msg-part' })
        body.append(part)
      }
      part.textContent = ''
      part.append(renderMarkdown(acc, (code, pre, lang) => this.codeActions(code, pre, lang)))
      this.scroll()
    }
    this.streaming = ask(`tab-${tab.id}`, 'chat', prompt, context ?? tab.aiContext(), this.host.shellName(), {
      delta: (t) => {
        msg.classList.remove('thinking')
        if (!part && !t.trim()) return // separator before a new part
        acc += t
        sawText = true
        render()
      },
      usage: (u) => {
        // Which model answered, and what it cost in tokens (summed over tool rounds).
        totalIn += u.input
        totalOut += u.output
        usedModel = u.model || usedModel
        meta.textContent = `${usedModel}${totalIn || totalOut ? ` · ${fmtTokens(totalIn)} in · ${fmtTokens(totalOut)} out` : ''}`
        if (!msg.contains(meta)) msg.append(meta)
        if (this.host.assistant().model === 'auto model' && usedModel) this.modelLabel.textContent = `auto → ${usedModel}`
      },
      redacted: (n) => {
        msg.append(el('div', { class: 'redacted-note', text: `🔒 Hid ${n} secret${n === 1 ? '' : 's'} from the terminal text sent` }))
      },
      tool: (e) => {
        msg.classList.remove('thinking')
        acc = ''
        part = null // text after the tool goes in a new part
        body.append(this.toolCard(tab, e))
        this.scroll()
      },
      done: () => {
        msg.classList.remove('thinking')
        if (!sawText && !body.childElementCount) body.append(el('p', { class: 'dim', text: '(no answer)' }))
        this.finish()
      },
      error: (e) => {
        msg.classList.remove('thinking')
        msg.classList.add('error')
        body.append(el('p', { class: 'err', text: e }))
        if (/API key|sign-in/i.test(e)) this.showSetup()
        this.finish()
      },
    })
    this.sendBtn.textContent = '■'
    this.sendBtn.title = 'Stop'
  }

  /** Claude wants to type (or run) a command: do it, or ask first for running. */
  private pendingTool: (() => void) | null = null

  private toolCard(tab: Tab, e: ToolEvent): HTMLElement {
    const reply = (result: string, isError = false) => {
      this.pendingTool = null
      AIToolResult(e.toolId, result, isError)
    }
    const cmd = el('code', { text: e.command })
    const status = el('div', { class: 'tool-status' })
    const label = el('div', { class: 'code-label', text: e.run ? `${this.host.assistant().name} wants to run` : 'Typed into your terminal' })
    const card = el('div', { class: 'tool-card' }, label, el('div', { class: 'tool-cmd' }, cmd), status)

    if (!e.run) {
      if (tab.sendCommand(e.command, false)) {
        status.textContent = '✓ On your prompt — press ⏎ in the terminal to run it'
        card.classList.add('done')
        reply('Typed the command onto the user\'s prompt (not run). They can press Enter to run it.')
      } else {
        status.textContent = '✕ Couldn\'t type it (see the message at the bottom)'
        card.classList.add('failed')
        reply('Could not type the command: the terminal is busy (a command is running or a full-screen program is open).', true)
      }
      return card
    }

    const runBtn = el('button', { class: 'primary small' }, '▶ Run')
    const noBtn = el('button', { class: 'ghost small' }, 'Cancel')
    const actions = el('div', { class: 'code-actions' }, runBtn, noBtn)
    // Risky commands (or ones copied from the output) say why and need a
    // second, deliberate click.
    const warn = riskWarning(e.risk)
    if (warn) card.append(warn)
    card.append(actions)
    card.classList.add('ask')
    this.pendingTool = () => {
      actions.remove()
      status.textContent = 'Stopped'
      reply('The user stopped the request; nothing was run.')
    }
    noBtn.addEventListener('click', () => {
      actions.remove()
      card.classList.replace('ask', 'failed')
      label.textContent = 'Not run'
      status.textContent = 'You cancelled it'
      reply('The user chose not to run this command.')
    })
    let armed = !warn
    runBtn.addEventListener('click', async () => {
      if (!armed) {
        armed = true
        runBtn.textContent = '⚠ Run Anyway'
        runBtn.classList.add('danger')
        return
      }
      actions.remove()
      card.classList.remove('ask')
      status.textContent = 'Running…'
      const res = await tab.runAndWait(e.command)
      if (!res.started) {
        card.classList.add('failed')
        status.textContent = '✕ Couldn\'t run it (see the message at the bottom)'
        reply('Could not run the command: the terminal is busy.', true)
        return
      }
      label.textContent = 'Ran in your terminal'
      card.classList.add(res.exitCode === 0 || res.exitCode === undefined ? 'done' : 'failed')
      status.textContent =
        res.exitCode === undefined ? '↻ Still running in your terminal' : res.exitCode === 0 ? '✓ Ran it' : `✕ Exited with ${res.exitCode}`
      reply(res.summary)
    })
    return card
  }

  private finish() {
    if (this.pendingTool) this.pendingTool()
    this.streaming = null
    this.sendBtn.textContent = '↑'
    this.sendBtn.title = 'Send'
    this.scroll()
  }

  private codeActions(code: string, pre: HTMLElement, lang: string) {
    const command = code.split('\n').filter((l) => !/^\s*#/.test(l)).join('\n').trim()
    // Only shell blocks get Run / Insert; text, ASCII art and other
    // languages just get Copy.
    const shellLang = /^(sh|bash|zsh|shell|console|tcsh|csh|fish)$/.test(lang)
    const looksLikeArt = /[|_\\/]{3,}|[█▀▄]/.test(code)
    const isCommand = !!command && (shellLang || (lang === '' && !looksLikeArt && command.split('\n').length <= 3))
    pre.classList.add(isCommand ? 'is-command' : 'is-text')
    pre.prepend(el('div', { class: 'code-label', text: isCommand ? 'Suggested command' : lang && lang !== 'text' ? lang : 'Text' }))
    const bar = el('div', { class: 'code-actions' })
    if (!isCommand) {
      const b = el('button', { title: 'Copy' }, 'Copy')
      b.addEventListener('click', () => {
        copyText(code)
        toast('Copied')
      })
      bar.append(b)
      pre.append(bar)
      return
    }
    const mk = (label: string, title: string, fn: () => void) => {
      const b = el('button', { title }, label)
      b.addEventListener('click', fn)
      bar.append(b)
    }
    mk('▶ Run', 'Run in the terminal', async () => {
      const tab = this.host.activeTab()
      if (!tab) return
      // The same check as the AI's own requests: risky commands warn first.
      const risk = await CommandRisk(command, tab.aiContext())
      if (risk.level) {
        const old = bar.previousElementSibling
        if (old?.classList.contains('risk')) {
          old.remove()
          tab.sendCommand(command, true)
          return
        }
        const w = riskWarning(risk)!
        w.append(el('div', { class: 'help', text: 'Press ▶ Run again to run it anyway, or Insert to look at it on the prompt first.' }))
        pre.insertBefore(w, bar)
        return
      }
      tab.sendCommand(command, true)
    })
    mk('Insert', 'Put on the prompt to edit first', () => this.host.activeTab()?.sendCommand(command, false))
    mk('Copy', 'Copy', () => {
      copyText(command)
      toast('Copied')
    })
    pre.append(bar)
  }

  private scroll() {
    this.list.scrollTop = this.list.scrollHeight
  }
}

// ---- Command bar (⌘J): plain English → one command ---------------------------------

export class CommandBar {
  private root = document.getElementById('cmdbar')!
  private input = el('input', { type: 'text', placeholder: 'Describe a command in plain English…', spellcheck: 'false' }) as HTMLInputElement
  private preview = el('div', { class: 'cmd-preview' })
  private help = el('div', { class: 'cmd-help' })
  private result = ''
  private pending: string | null = null

  constructor(private host: AIHost) {
    this.root.append(el('div', { class: 'cmd-row' }, el('span', { class: 'spark' }, '✦'), this.input), this.preview, this.help)
    this.input.addEventListener('keydown', (e) => this.onKey(e))
    this.input.addEventListener('input', () => {
      if (this.result) {
        this.result = ''
        this.render()
      }
    })
  }

  async open() {
    if (!(await HasAPIKey())) {
      toast(`Connect ${this.host.assistant().name} in Settings (⌘,) first`, 'error')
      this.host.openSettings()
      return
    }
    this.root.hidden = false
    this.input.value = ''
    this.result = ''
    this.render()
    this.input.focus()
  }

  close() {
    if (this.pending) AICancel(this.pending)
    this.pending = null
    this.root.hidden = true
    this.host.activeTab()?.term.focus()
  }

  private onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      this.close()
      return
    }
    if (e.key !== 'Enter' || e.isComposing) return
    e.preventDefault()
    const runIt = e.metaKey || e.ctrlKey
    if (this.result && !this.result.startsWith('#')) {
      const tab = this.host.activeTab()
      if (tab?.sendCommand(this.result, runIt)) this.close()
      return
    }
    this.generate()
  }

  private generate() {
    const tab = this.host.activeTab()
    const prompt = this.input.value.trim()
    if (!tab || !prompt || this.pending) return
    this.result = ''
    this.preview.className = 'cmd-preview loading'
    this.preview.textContent = 'Thinking…'
    this.help.textContent = ''
    let acc = ''
    this.pending = ask('commandbar', 'command', prompt, tab.aiContext(), this.host.shellName(), {
      delta: (t) => {
        acc += t
        this.preview.className = 'cmd-preview'
        this.preview.textContent = acc
      },
      done: (t) => {
        this.pending = null
        this.result = (t || acc).trim().replace(/^```\w*\n?|```$/g, '').replace(/^\$\s+/, '').trim()
        this.render()
      },
      error: (err) => {
        this.pending = null
        this.preview.className = 'cmd-preview error'
        this.preview.textContent = err
      },
    })
  }

  private render() {
    if (!this.result) {
      this.preview.className = 'cmd-preview empty'
      this.preview.textContent = ''
      this.help.innerHTML = `<kbd>⏎</kbd> ask ${this.host.assistant().name} <kbd>esc</kbd> cancel`
      return
    }
    const note = this.result.startsWith('#')
    this.preview.className = 'cmd-preview' + (note ? ' note' : '')
    this.preview.textContent = note ? this.result.replace(/^#\s*/, '') : this.result
    this.help.innerHTML = note
      ? 'Try describing it differently. <kbd>esc</kbd> cancel'
      : '<kbd>⏎</kbd> put on the prompt <kbd>⌘⏎</kbd> run now <kbd>esc</kbd> cancel · edit the text to ask again'
  }
}

/** A red (danger) or amber (caution) note saying why a command is risky. */
function riskWarning(risk?: Risk): HTMLElement | null {
  if (!risk?.level || !risk.reasons?.length) return null
  const title = risk.level === 'danger' ? '⚠ Careful: this command could do real damage' : '⚠ Check this command before running it'
  return el('div', { class: `risk ${risk.level}`, role: 'alert' },
    el('strong', { text: title }),
    el('ul', {}, ...risk.reasons.map((r) => el('li', { text: 'It ' + r.replace(/^it /, '') }))))
}
