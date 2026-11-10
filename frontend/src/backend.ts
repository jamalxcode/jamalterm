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
  AnthropicAccount, Attach, BlurActive, BusyTabs, CheckForUpdate, CloseSettings,
  CloseTab, CloseWindow, CloseWindowNow, CommandRisk, Commands, Confirm,
  Diagnostics, DismissCrash, FindLocalServers, ForgetSession, GPUOff, GitBranch,
  HasAPIKey, HasSavedAPIKey, HasSavedProviderKey, HomeDir, InstallUpdate, IsPrimaryWindow,
  IsSettingsWindow, KeychainKeysWaiting, KeysUnreadable, LastCrash, ListProviderModels, LiveTabs,
  LoadSession, LogProblem, MoveKeysFromKeychain, NewWindow, Notice, Notify,
  OpenPath, OpenSettings, OpenSettingsAt, OpenURL, PreviousVersion, ProblemLogPath,
  Quit, QuitNow, RemoteSetupLine, Resize, ResolvePath, RevertUpdate,
  RunCommand, SaveSession, SaveSettings, SecureInputActive, SetAIProvider, SetAPIKey,
  SetActiveTab, SetMenuState, SetProviderKey, SetWindowOpacity, Settings, SettingsOpen,
  ShellName, ShowProblemLog, SignInOpenRouter, SmokeMode, SmokeReport, SmokeShot,
  SmokeWaitKeys, Start, StartDir, ToTerminal, UpdateStatus, Version,
  WhatsNew, WindowCount, WindowID, Write,
} = App
