// The only place the window talks to the Go side (Wails v3). The UI tests
// replace it with a pretend Go side by setting window.__jtFake before the
// page loads (tests/fake-wails.js).
import * as Real from '../bindings/jamalterm/app'
import { Events, Window } from '@wailsio/runtime'

type AppApi = typeof Real
interface Fake {
  App: AppApi
  on(name: string, cb: (data: any) => void): () => void
  setTitle(title: string): void
}
const fake = (window as unknown as { __jtFake?: Fake }).__jtFake

const App: AppApi = fake?.App ?? Real

/** Listens for an event from Go; cb gets the event's data. */
export function EventsOn(name: string, cb: (data: any) => void): () => void {
  if (fake) return fake.on(name, cb)
  return Events.On(name, (ev: { data: any }) => cb(ev.data))
}

/** Sets this window's title. */
export function WindowSetTitle(title: string) {
  if (fake) fake.setTitle(title)
  else void Window.SetTitle(title)
}

export const {
  AIAsk, AICancel, AIProviders, AIReset, AIToolResult, Ack,
  AnthropicAccount, Attach, BlurActive, BusyTabs, CheckForUpdate, CloseTab,
  CloseWindow, CloseWindowNow, Diagnostics, ForgetSession, GPUOff, GitBranch,
  HasAPIKey, HasSavedAPIKey, HasSavedProviderKey, HomeDir, InstallUpdate, IsPrimaryWindow,
  ListProviderModels, LiveTabs, LoadSession, LogProblem, NewWindow, Notify,
  OpenPath, OpenURL, PreviousVersion, ProblemLogPath, Quit, QuitNow,
  Resize, ResolvePath, RevertUpdate, SaveSession, SaveSettings, SecureInputActive,
  SetAIProvider, SetAPIKey, SetActiveTab, SetProviderKey, SetWindowOpacity, Settings,
  ShellName, ShowProblemLog, SignInOpenRouter, Start, StartDir, Version,
  WindowID, Write,
} = App
