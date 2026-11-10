import { CheckForUpdate, InstallUpdate, OpenURL, Version, PreviousVersion, RevertUpdate, Notice } from './backend'
import { EventsOn } from './backend'
import { el, toast, stored, store } from './util'
import type { UpdateInfo } from './types'

const SIX_HOURS = 6 * 60 * 60 * 1000

/** The "a new version is available" card and the update flow. */
export class Updater {
  private root = document.getElementById('update')!
  private info: UpdateInfo | null = null
  private busy = false

  constructor(private saveSession: () => Promise<void>) {
    EventsOn('update:progress', (p: { done: number; total: number }) => {
      const bar = this.root.querySelector('.progress i') as HTMLElement | null
      const label = this.root.querySelector('.up-status')
      if (!bar || !label) return
      const pct = p.total > 0 ? Math.round((p.done / p.total) * 100) : 0
      bar.style.width = `${pct}%`
      label.textContent = pct >= 100 ? 'Installing…' : `Downloading… ${pct}%`
    })
  }

  /** Background checks; quiet unless there is something new. */
  async startAuto(enabled: () => boolean) {
    const version = await Version()
    if (/dev|test/.test(version)) return // development builds don't update
    const run = () => {
      if (enabled()) void this.check(false)
    }
    setTimeout(run, 4000)
    setInterval(run, SIX_HOURS)
  }

  async check(manual: boolean) {
    if (this.busy) return
    try {
      const info = await CheckForUpdate()
      this.info = info
      if (!info.available) {
        // Asked by hand: always answer, with a native alert (like other Mac apps).
        if (manual) void Notice('You’re up to date', `jamalterm ${info.current} is the newest version.`)
        return
      }
      if (!manual && stored('jt-skip') === info.latest) return
      this.show()
    } catch (err) {
      if (manual) void Notice('Couldn’t check for updates', `${err}\n\nCheck your internet connection and try again.`)
    }
  }

  private show() {
    const info = this.info!
    this.root.textContent = ''
    // Only the "What's new" part of the release notes (up to the next heading).
    const whatsNewPart = (info.notes || '').split(/^## (?!What)/m)[0]
    const notes = whatsNewPart
      .split('\n')
      .filter((l) => /^\s*[-*] /.test(l))
      .slice(0, 4)
      .map((l) => el('li', { text: l.replace(/^\s*[-*]\s+/, '').replace(/\*\*/g, '') }))
    const later = el('button', { class: 'ghost small' }, 'Later')
    const whatsNew = el('button', { class: 'ghost small' }, "What's new")
    const onQuit = el('button', { class: 'ghost small', title: 'Install now; the new version starts next time you open jamalterm' }, 'When I quit')
    onQuit.hidden = !info.canInstall
    const go = el('button', { class: 'primary small' }, info.canInstall ? 'Update now' : 'Download')
    const status = el('div', { class: 'up-status dim' })
    status.textContent = info.canInstall
      ? `You have ${info.current}. Your tabs reopen after the restart (running commands stop).`
      : info.reason
    this.root.append(
      el('div', { class: 'up-head' }, el('span', { class: 'spark' }, '✦'), el('strong', { text: `jamalterm ${info.latest} is available` })),
      ...(notes.length ? [el('ul', {}, ...notes)] : []),
      status,
      el('div', { class: 'progress' }, el('i')),
      el('div', { class: 'up-actions' }, later, whatsNew, onQuit, go),
    )
    later.addEventListener('click', () => {
      store('jt-skip', info.latest)
      this.hide()
    })
    whatsNew.addEventListener('click', () => OpenURL(info.url))
    go.addEventListener('click', () => (info.canInstall ? void this.install(go, later, true) : OpenURL(info.url)))
    onQuit.addEventListener('click', () => void this.install(onQuit, later, false))
    this.root.hidden = false
  }

  private async install(go: HTMLButtonElement, later: HTMLButtonElement, relaunch: boolean) {
    this.busy = true
    this.root.querySelectorAll('.up-actions button').forEach((b) => ((b as HTMLButtonElement).disabled = true))
    this.root.classList.add('installing')
    const status = this.root.querySelector('.up-status')!
    status.textContent = 'Downloading…'
    try {
      if (relaunch) await this.saveSession()
      await InstallUpdate(relaunch)
      if (relaunch) {
        status.textContent = 'Restarting…'
      } else {
        this.busy = false
        this.root.classList.remove('installing')
        status.textContent = `✓ Installed. jamalterm ${this.info?.latest} starts next time you open it.`
        this.root.querySelector('.up-actions')!.replaceChildren(el('button', { class: 'ghost small' }, 'OK'))
        this.root.querySelector('.up-actions button')!.addEventListener('click', () => this.hide())
      }
    } catch (err) {
      this.root.querySelectorAll('.up-actions button').forEach((b) => ((b as HTMLButtonElement).disabled = false))
      this.busy = false
      go.disabled = later.disabled = false
      this.root.classList.remove('installing')
      status.textContent = String(err)
      status.classList.add('err')
    }
  }

  private hide() {
    this.root.hidden = true
  }

  /** "Go Back to Previous Version…" from the jamalterm menu. */
  async offerRevert() {
    const prev = await PreviousVersion()
    if (!prev) {
      toast('No earlier version kept yet — one is kept after each update.')
      return
    }
    this.root.textContent = ''
    const cancel = el('button', { class: 'ghost small' }, 'Cancel')
    const back = el('button', { class: 'primary small' }, 'Go back')
    const status = el('div', { class: 'up-status dim', text: 'Your tabs reopen after the restart (running commands stop).' })
    this.root.append(
      el('div', { class: 'up-head' }, el('span', { class: 'spark' }, '↶'), el('strong', { text: `Go back to jamalterm ${prev}?` })),
      status,
      el('div', { class: 'up-actions' }, cancel, back),
    )
    cancel.addEventListener('click', () => this.hide())
    back.addEventListener('click', async () => {
      back.disabled = cancel.disabled = true
      status.textContent = 'Switching…'
      try {
        await this.saveSession()
        await RevertUpdate()
      } catch (err) {
        status.textContent = String(err)
        status.classList.add('err')
        back.disabled = cancel.disabled = false
      }
    })
    this.root.hidden = false
  }
}
