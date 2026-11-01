import { CheckForUpdate, InstallUpdate, OpenURL, Version } from '../wailsjs/go/main/App'
import { EventsOn } from '../wailsjs/runtime/runtime'
import type { main } from '../wailsjs/go/models'
import { el, toast } from './util'

const SIX_HOURS = 6 * 60 * 60 * 1000

/** The "a new version is available" card and the update flow. */
export class Updater {
  private root = document.getElementById('update')!
  private info: main.UpdateInfo | null = null
  private busy = false

  constructor(private openTabs: () => number) {
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
        if (manual) toast(`You're up to date (jamalterm ${info.current}).`)
        return
      }
      if (!manual && localStorageGet('jt-skip') === info.latest) return
      this.show()
    } catch (err) {
      if (manual) toast(`Couldn't check for updates: ${err}`, 'error')
    }
  }

  private show() {
    const info = this.info!
    this.root.textContent = ''
    const notes = (info.notes || '')
      .split('\n')
      .filter((l) => /^\s*[-*] /.test(l))
      .slice(0, 4)
      .map((l) => el('li', { text: l.replace(/^\s*[-*]\s+/, '').replace(/\*\*/g, '') }))
    const later = el('button', { class: 'ghost small' }, 'Later')
    const whatsNew = el('button', { class: 'ghost small' }, "What's new")
    const go = el('button', { class: 'primary small' }, info.canInstall ? 'Update & Restart' : 'Download')
    const status = el('div', { class: 'up-status dim' })
    const tabs = this.openTabs()
    status.textContent = info.canInstall
      ? `You have ${info.current}. ${tabs > 1 ? `Restarting closes your ${tabs} tabs.` : 'Restarting closes this tab.'}`
      : info.reason
    this.root.append(
      el('div', { class: 'up-head' }, el('span', { class: 'spark' }, '✦'), el('strong', { text: `jamalterm ${info.latest} is available` })),
      ...(notes.length ? [el('ul', {}, ...notes)] : []),
      status,
      el('div', { class: 'progress' }, el('i')),
      el('div', { class: 'up-actions' }, later, whatsNew, go),
    )
    later.addEventListener('click', () => {
      localStorageSet('jt-skip', info.latest)
      this.hide()
    })
    whatsNew.addEventListener('click', () => OpenURL(info.url))
    go.addEventListener('click', () => (info.canInstall ? void this.install(go, later) : OpenURL(info.url)))
    this.root.hidden = false
  }

  private async install(go: HTMLButtonElement, later: HTMLButtonElement) {
    this.busy = true
    go.disabled = later.disabled = true
    this.root.classList.add('installing')
    const status = this.root.querySelector('.up-status')!
    status.textContent = 'Downloading…'
    try {
      await InstallUpdate()
      status.textContent = 'Restarting…'
    } catch (err) {
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
}

function localStorageGet(k: string): string | null {
  try {
    return localStorage.getItem(k)
  } catch {
    return null
  }
}

function localStorageSet(k: string, v: string) {
  try {
    localStorage.setItem(k, v)
  } catch {
    /* private mode */
  }
}
