// A stand-in for the Go side of jamalterm, injected before the UI loads,
// so the real interface can be tested in a plain browser. It includes a tiny
// pretend shell that prints the same marks as the real shell integration.
;(() => {
  const listeners = {}
  const emit = (name, data) => (listeners[name] || []).forEach((cb) => cb(data))
  const b64 = (s) => btoa(unescape(encodeURIComponent(s)))
  const calls = (window.__calls = [])
  const state = (window.__fake = {
    settings: Object.assign({
      theme: 'jamal-night', fontFamily: 'monospace', fontSize: 14, lineHeight: 1.15, cursorStyle: 'block', cursorBlink: false,
      opacity: 100, blur: true, scrollback: 10000, shell: '', shellIntegration: true, arrowBrowse: true, restoreSession: true,
      promptKeys: 'emacs', notifyLongCommands: true, notifyAfterSeconds: 10, screenReader: false, confirmRerun: false,
      optionIsMeta: true, autoUpdate: false, keepProblemLog: true, aiEnabled: true, aiProvider: 'anthropic', aiModel: 'claude-opus-5-5',
      openaiModel: '', deepseekModel: '', xaiModel: '', openrouterModel: '', localUrl: 'http://localhost:11434/v1', localModel: '', ligatures: false, hostColors: [], windowFrame: 'system', showTabBar: true, aiShareOutput: true,
    }, window.__settings || {}),
    session: window.__session || '',
    live: window.__live || [],
    shells: {},
    aiReply: 'Here you go.',
    aiTool: null,
  })

  // The pretend shell: echoes typing; Enter runs "echo …", "cd …", "osc52 …".
  function prompt(id) {
    const sh = state.shells[id]
    emit('pty:data', { id, data: b64(`\x1b]7;file://host${sh.cwd}\x07\x1b]133;A\x07${sh.cwd} $ `) })
  }
  function run(id, line) {
    const sh = state.shells[id]
    let out = ''
    let code = 0
    const [cmd, ...rest] = line.trim().split(' ')
    if (cmd === 'echo') out = rest.join(' ') + '\r\n'
    else if (cmd === 'cd') sh.cwd = rest[0] || '/home/u'
    else if (cmd === 'false') code = 1
    else if (cmd === 'seq') out = Array.from({ length: +rest[0] || 10 }, (_, i) => `${i + 1}\r\n`).join('')
    else if (cmd === 'build') { out = Array.from({ length: +rest[0] || 80 }, (_, i) => `step ${i + 1}\r\n`).join('') + 'error: it broke\r\n'; code = 2 }
    else if (cmd === 'osc52') out = `\x1b]52;c;${btoa(rest.join(' '))}\x07`
    else if (cmd) { out = `${cmd}: command not found\r\n`; code = 127 }
    emit('pty:data', { id, data: b64(`\r\n\x1b]6973;cmd;${b64(line)}\x07\x1b]133;C\x07${out}\x1b]133;D;${code}\x07`) })
    prompt(id)
  }
  function write(id, data) {
    const sh = state.shells[id]
    for (const ch of data) {
      if (ch === '\r') { const l = sh.line; sh.line = ''; run(id, l) }
      else if (ch === '\x15') sh.line = ''
      else if (ch === '\x05' || ch === '\x1b') { /* line editing keys */ }
      else { sh.line += ch; emit('pty:data', { id, data: b64(ch) }) }
    }
  }

  const providers = [
    { id: 'anthropic', name: 'Claude', label: 'Claude (Anthropic)', company: 'Anthropic', keyUrl: '', keyWhere: 'console', keyHint: 'sk-ant-…', billing: '', modelField: 'aiModel', autoModel: 'x', signIn: 'anthropic' },
    { id: 'local', name: 'Local', label: 'Local model on this Mac (Ollama, LM Studio)', company: 'your own computer', keyUrl: '', keyWhere: '', keyHint: '', billing: 'Free', modelField: 'localModel', autoModel: 'first', signIn: '', local: true },
    { id: 'openai', name: 'ChatGPT', label: 'ChatGPT (OpenAI)', company: 'OpenAI', keyUrl: '', keyWhere: 'platform', keyHint: 'sk-…', billing: '', modelField: 'openaiModel', autoModel: 'auto', signIn: '' },
  ]

  const app = {
    Settings: () => state.settings,
    SaveSettings: (s) => { state.settings = s; emit('settings:changed', s); return s },
    ShellName: () => 'zsh', HomeDir: () => '/home/u', GPUOff: () => true, Version: () => 'test',
    StartDir: () => '', IsPrimaryWindow: () => true, WindowID: () => 0,
    LoadSession: () => state.session,
    SaveSession: (d) => { state.session = d },
    AIProviders: () => providers,
    HasAPIKey: () => true, HasSavedAPIKey: () => false, HasSavedProviderKey: () => false,
    AnthropicAccount: () => ({ signedIn: false }), BlurActive: () => false,
    BusyTabs: (ids) => (state.busy || []).filter((b) => ids.includes(b.id)),
    LiveTabs: () => state.live || [],
    Attach: (id) => b64(`reattached-${id}\r\n`),
    FindLocalServers: () => [{ name: 'Ollama', url: 'http://localhost:11434/v1', models: ['llama3.2:3b', 'qwen3:8b'] }],
    Commands: () => [
      { id: 'newTab', title: 'New Tab', menu: 'Shell', keys: 'Cmd+T', palette: true, native: false },
      { id: 'splitRight', title: 'Split Right', menu: 'Shell', keys: 'Cmd+D', palette: true, native: false },
      { id: 'settings', title: 'Settings…', menu: 'jamalterm', keys: 'Cmd+,', palette: true, native: false },
      { id: 'palette', title: 'Command Palette…', menu: 'View', keys: 'Cmd+Shift+P', palette: false, native: false },
      { id: 'reloadWindow', title: 'Reload Window', menu: 'View', keys: 'Cmd+Shift+R', palette: true, native: true },
    ],
    Confirm: (title, message, action) => { (state.confirms ||= []).push({ title, message, action }); return state.confirmAnswer ?? false },
    KeychainKeysWaiting: () => !!(state.keychainWaiting ?? window.__keychainWaiting),
    MoveKeysFromKeychain: () => { state.keychainWaiting = false; return 2 },
    KeysUnreadable: () => false,
    CommandRisk: (cmd) => /rm -rf|\|\s*sh\b/.test(cmd) ? { level: 'danger', reasons: ['deletes files recursively from a broad location (/, ~, * or ..)'] } : { level: '', reasons: null },
    UpdateStatus: () => state.updateStatus || { current: 'test', checkedAt: 0, latest: '', available: false, error: '', channel: 'stable' },
    CheckForUpdate: () => { state.updateStatus = { current: 'test', checkedAt: Date.now(), latest: '9.9.9', available: true, error: '', channel: 'stable' }; return { available: true, current: 'test', latest: '9.9.9', notes: '', url: '', canInstall: false, reason: 'dev build' } },
    Diagnostics: () => 'jamalterm diagnostics\nVersion:   test\nRecent problems: none\n',
    GitBranch: () => '', ResolvePath: () => '', PreviousVersion: () => '',
    Start: (id, cols, rows, cwd) => { state.shells[id] = { cwd: cwd || '/home/u', line: '' }; setTimeout(() => prompt(id), 10) },
    Write: (id, data) => write(id, data),
    AIAsk: (req) => {
      setTimeout(() => {
        if (state.aiTool) {
          emit('ai:tool', { id: req.id, toolId: 't1', command: state.aiTool.command, run: state.aiTool.run, risk: state.aiTool.risk })
          return
        }
        emit('ai:delta', { id: req.id, text: state.aiReply })
        emit('ai:usage', { id: req.id, model: 'claude-opus-5-5', input: 1200, output: 34 })
        emit('ai:done', { id: req.id, text: state.aiReply })
      }, 20)
    },
  }
  // src/backend.ts uses this instead of the real Go side.
  window.__jtFake = {
    App: new Proxy(app, {
      get: (t, k) => (...args) => { calls.push([k, ...args]); return Promise.resolve(typeof t[k] === 'function' ? t[k](...args) : undefined) },
    }),
    on: (name, cb) => { (listeners[name] ||= []).push(cb); return () => {} },
    setTitle: (t) => { state.title = t },
  }
  window.__emit = emit
})()
