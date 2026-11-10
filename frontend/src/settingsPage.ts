// The Settings window (⌘,): its own window, like Terminal's and Safari's.
// Every change is saved at once and every terminal window applies it.
import '@fontsource/jetbrains-mono/400.css'
import './style.css'
import { Settings, SaveSettings, CloseSettings, ToTerminal, PreviewTheme } from './backend'
import { EventsOn, WindowSetTitle } from './backend'
import { SettingsView, type SettingsPage } from './settingsView'
import { loadProviders } from './providers'
import { applySystemLook } from './system'
import { applySystemFrame } from './themes'
import { toast, isMac } from './util'
import type { Settings as Prefs } from './types'

document.body.classList.toggle('mac', isMac)
// Settings always looks like the Mac's own Settings windows (light or dark).
applySystemLook(() => applySystemFrame())
applySystemFrame()

let cfg: Prefs
const root = document.getElementById('settings-root')!
const view = new SettingsView({
  current: () => cfg,
  save: async (s) => {
    try {
      cfg = await SaveSettings(s)
    } catch (err) {
      toast(String(err), 'error')
    }
  },
  close: () => void CloseSettings(),
  checkUpdates: () => void ToTerminal('checkUpdates'),
  runInNewTab: (cmd) => void ToTerminal('run:' + cmd),
  diagnostics: () => void ToTerminal('diagnostics'),
  setTitle: (t) => WindowSetTitle(t),
}, root)

// Opened on a pane: settings.html#ai:Assistant
function fromHash(): [SettingsPage | undefined, string | undefined] {
  const [page, find] = decodeURIComponent(location.hash.slice(1)).split(':')
  return [(page || undefined) as SettingsPage | undefined, find || undefined]
}

async function boot() {
  ;[cfg] = await Promise.all([Settings(), loadProviders()]) as [Prefs, void]
  const [page, find] = fromHash()
  await view.open(page, find)
}

// Asked to show a pane while open (from the command palette).
EventsOn('settings:show', (e: { page: SettingsPage; find: string }) => view.show(e.page, e.find || undefined))
// Changed elsewhere (the AI menu, the palette): show the new values, unless
// you're in the middle of editing here.
EventsOn('settings:changed', (s: Prefs) => {
  const same = JSON.stringify(s) === JSON.stringify(cfg)
  cfg = s
  if (!same && !view.pending && !root.contains(document.activeElement)) void view.open()
})
// ⌘W and Esc close the window; ⌘A and ⌘F act on the fields here.
EventsOn('menu', (cmd: string) => {
  if (cmd === 'selectAll') document.execCommand('selectAll')
})

void boot()

// Closing Settings mid-hover must not leave a previewed theme behind.
addEventListener('beforeunload', () => void PreviewTheme(''))
addEventListener('blur', () => void PreviewTheme(''))
