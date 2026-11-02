// Shapes of the data exchanged with the Go side (hand-written so the
// window doesn't depend on generated classes). Keep in sync with the Go
// structs named in each comment.

/** Go: ai.Account */
export interface Account {
  signedIn: boolean
  profile: string
  workspace: string
}

/** Go: ai.ProviderInfo */
export interface ProviderInfo {
  id: string
  name: string
  label: string
  company: string
  keyUrl: string
  keyWhere: string
  keyHint: string
  billing: string
  modelField: string
  autoModel: string
  signIn: string
}

/** Go: ai.Request */
export interface AIRequest {
  id: string
  chat: string
  mode: string
  prompt: string
  context: string
  shell: string
}

/** Go: main.UpdateInfo */
export interface UpdateInfo {
  available: boolean
  current: string
  latest: string
  notes: string
  url: string
  canInstall: boolean
  reason: string
}

/** Go: settings.Settings */
export interface Settings {
  theme: string
  fontFamily: string
  fontSize: number
  lineHeight: number
  cursorStyle: string
  opacity: number
  blur: boolean
  cursorBlink: boolean
  scrollback: number
  shell: string
  shellIntegration: boolean
  arrowBrowse: boolean
  restoreSession: boolean
  notifyLongCommands: boolean
  notifyAfterSeconds: number
  screenReader: boolean
  promptKeys: string
  confirmRerun: boolean
  optionIsMeta: boolean
  keepProblemLog: boolean
  autoUpdate: boolean
  aiEnabled: boolean
  aiProvider: string
  aiModel: string
  openaiModel: string
  deepseekModel: string
  xaiModel: string
  openrouterModel: string
  aiShareOutput: boolean
}
