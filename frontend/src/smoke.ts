// The self-test CI runs on a real Mac (JAMALTERM_SMOKE): the window drives
// itself through the main features and reports what worked, with
// screenshots. See smoke.go and .github/workflows/build.yml.
import type { Tab } from './tab'
import { SmokeShot, SmokeWaitKeys, SmokeReport, WindowCount, SetWindowOpacity } from './backend'

export interface SmokeHost {
  bootTimes: Record<string, number>
  active(): Tab | undefined
  tabCount(): number
  paneCount(): number
  run(cmd: string): void
}

interface Step { name: string; ok: boolean | null; ms: number; detail?: string }

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function until(f: () => boolean | Promise<boolean>, ms = 10000): Promise<boolean> {
  const end = Date.now() + ms
  while (Date.now() < end) {
    if (await f()) return true
    await sleep(50)
  }
  return false
}

function text(t: Tab | undefined): string {
  if (!t) return ''
  const b = t.term.buffer.active
  const out: string[] = []
  for (let i = Math.max(0, b.length - 300); i < b.length; i++) out.push(b.getLine(i)?.translateToString(true) ?? '')
  return out.join('\n')
}

export async function runSmoke(h: SmokeHost) {
  const steps: Step[] = []
  const step = async (name: string, f: () => Promise<boolean | null | string>) => {
    const t0 = performance.now()
    let ok: boolean | null = false, detail: string | undefined
    try {
      const r = await f()
      if (typeof r === 'string') { ok = false; detail = r } else ok = r
    } catch (e) {
      detail = String(e)
    }
    steps.push({ name, ok, ms: Math.round(performance.now() - t0), detail })
  }
  const shot = async (name: string) => {
    // Let the screen catch up before capturing it.
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
    await sleep(600)
    const err = await SmokeShot(name)
    if (err) steps.push({ name: `screenshot ${name}`, ok: false, ms: 0, detail: err })
  }
  const launchStart = 0 // performance.now() counts from the page starting to load
  let launchMs = 0

  await step('first prompt', async () => {
    const ok = await until(() => !!h.active()?.integration, 15000)
    launchMs = Math.round(performance.now() - launchStart)
    return ok
  })
  await shot('1-first-prompt')

  await step('run a command', async () => {
    h.active()!.sendCommand('echo smoke-$((6*7))', true)
    return until(() => /^smoke-42$/m.test(text(h.active())))
  })

  let seqMs = 0
  await step('fast output (seq 1 200000)', async () => {
    const t = h.active()!
    const t0 = performance.now()
    t.sendCommand('seq 1 200000', true)
    // (The command's own marker scrolls out of the 10,000-line scrollback.)
    const ok = await until(() => !t.runningBlock && /^200000$/m.test(text(t)), 60000)
    seqMs = Math.round(performance.now() - t0)
    return ok || `last finished: ${t.lastFinished?.command}; screen ends: ${text(t).trim().split('\n').slice(-3).join(' | ')}`
  })

  await step('browse mode', async () => {
    h.run('browse')
    const ok = await until(() => !!h.active()?.browse.active, 2000)
    h.active()?.exitBrowse()
    return ok
  })

  await step('split pane', async () => {
    h.run('splitRight')
    return until(() => h.paneCount() === 2 && !!h.active()?.integration)
  })
  await shot('2-split')

  await step('command palette', async () => {
    h.run('palette')
    const ok = await until(() => !!document.querySelector('#palette:not([hidden]) li'), 2000)
    document.querySelector('#palette input')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    return ok
  })

  await step('settings pages', async () => {
    h.run('settings')
    const ok = await until(() => !!document.querySelector('#settings:not([hidden]) .settings-nav'), 3000)
    ;(document.querySelector('.settings-nav button[data-page="privacy"]') as HTMLElement | null)?.click()
    await sleep(200)
    await shot('3-settings')
    ;(document.querySelector('#settings button.ghost') as HTMLElement | null)?.click() // Cancel
    return ok
  })

  await step('see-through window', async () => {
    document.documentElement.classList.add('see-through')
    await SetWindowOpacity(60)
    await sleep(400)
    await shot('4-see-through')
    await SetWindowOpacity(100)
    document.documentElement.classList.remove('see-through')
    return true
  })

  // Real key presses from CI (System Events), if macOS lets it press keys.
  await step('keyboard shortcuts (⌘T, ⌘D)', async () => {
    const tabs = h.tabCount()
    const pressed = await SmokeWaitKeys()
    if (!pressed) return null // skipped
    return until(() => h.tabCount() === tabs + 1 && h.paneCount() === 2, 5000)
  })
  await shot('5-after-keys')

  await step('new window', async () => {
    h.run('newWindow')
    return until(async () => (await WindowCount()) === 2, 8000)
  })
  await sleep(1500)
  await shot('6-two-windows')

  await SmokeReport(JSON.stringify({
    ok: steps.every((s) => s.ok !== false),
    launchMs, seqMs, steps, boot: h.bootTimes,
    userAgent: navigator.userAgent,
  }))
}
