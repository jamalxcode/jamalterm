# jamalterm

A terminal for macOS with a **free-floating cursor** and **Claude built in**. Move up through everything you've run with the arrow keys or the mouse, press Enter, and the command runs again. Ask Claude why something failed, or describe a command in plain English.

It runs your normal shell (zsh, bash, tcsh, fish…), so everything you already use keeps working.

## Download

**[Latest release →](https://github.com/jamalxcode/jamalterm/releases/latest)**: download the `.dmg`, open it, and drag jamalterm into Applications.

The app isn't notarized by Apple (that needs a paid developer account), so macOS blocks the first launch. Run this once:

```bash
xattr -dr com.apple.quarantine /Applications/jamalterm.app
```

Or try to open it, then click **Open Anyway** in System Settings → Privacy & Security. Universal build: Apple silicon and Intel, macOS 10.15+.

### Updating

You only install by hand once. After that, jamalterm checks GitHub at launch and every 6 hours. When there's a new version, a card appears: click **Update & Restart**. The app downloads the new version, checks it against the release's SHA-256 checksums, its code signature and its bundle id, swaps itself in place, and restarts. Open tabs close. You can also use **jamalterm → Check for Updates…** at any time, and turn automatic checks off in Settings.

## Using it

### Browse mode: the free-floating cursor

| Key | What it does |
|---|---|
| **↑** at an empty prompt, **⌘↑**, or click any earlier line | Enter browse mode: a pink cursor appears where you are |
| ↑ ↓ ← → (or j k) | Move the cursor anywhere in the scrollback |
| ⌥↑ ⌥↓ (or ⇧↑ ⇧↓) | Jump between commands |
| **⏎** | Run the selected command again |
| ⇧⏎ or E | Put it on the prompt to edit first |
| C / O | Copy the command / its output |
| A | Ask Claude about this command |
| esc, ↓ past the bottom, or just start typing | Back to the prompt |

Each command gets a dot in the margin: green for success, red for failure, and pulsing blue while it runs. Hover for how long it took. Click a red dot to ask Claude what went wrong.

### Claude

| Key | What it does |
|---|---|
| **⌘I** | Chat panel. Claude sees your recent commands, exit codes and output. Commands in answers have **Run** and **Insert** buttons. |
| **⌘J** | Command bar: type "find big files in Downloads", get the command, press ⏎ to put it on the prompt (⌘⏎ runs it). |
| ⇧⌘E | Explain the last error |

To connect Claude, click **Sign in with Anthropic** in Settings (⌘,) or the AI panel. A tab opens, installs Anthropic's `ant` command-line tool with Homebrew if you don't have it, and signs you in with your browser (`ant auth login`). There's no key to copy, and the sign-in refreshes itself. Usage is billed to the Anthropic Console account you sign in with. You can also paste an API key from [console.anthropic.com](https://console.anthropic.com) instead. It's stored in your macOS Keychain, and a saved key takes priority over the sign-in. The default model is Claude Opus 5.5; Sonnet and Haiku are faster and cheaper options. You can turn off sending command output, or turn AI off completely.

### Everything else

- Tabs: ⌘T, ⌘W, ⌘1–9, ⇧⌘[ / ⇧⌘]. New tabs open in the current folder.
- ⌘F find, ⌘K clear, ⌘= / ⌘- / ⌘0 font size.
- 10 themes (Jamal Night, Catppuccin, Dracula, Tokyo Night, One Dark, Nord, Gruvbox, Solarized dark and light, GitHub Light), with a font, cursor style and blink setting.
- Option works as Meta (⌥B / ⌥F jump words). Turn it off in Settings.
- Nerd Font icons (Powerlevel10k, eza, starship…) show up with any font. The icon set is built in.
- If you'd rather ↑ go to shell history at an empty prompt, turn that off in Settings. ⌘↑ always starts browse mode.

## How it works

```
┌────────────────── jamalterm.app (Wails) ──────────────────┐
│ frontend/src/   xterm.js terminal, tabs, browse mode,     │
│                 AI panel, command bar, settings           │
├────────────────── events / bound methods ─────────────────┤
│ app.go, menu.go     tabs, menu bar, settings, AI bridge   │
│ internal/shell      PTY + your shell + shell integration  │
│ internal/ai         Claude via the official Go SDK        │
│ internal/settings   settings.json + Keychain              │
└───────────────────────────────────────────────────────────┘
```

**Shell integration.** To know where each command starts and ends, jamalterm loads a small hook script into zsh, bash and tcsh. It never edits your dotfiles: zsh gets a temporary `ZDOTDIR`, bash an `--rcfile`, and tcsh a temporary `HOME`. Each one hands straight back to your own startup files. The hooks print invisible marks (OSC 133, the same convention iTerm2, VS Code and Ghostty use), plus the command text and current folder. The scripts are in `internal/shell/integration/`. Other shells (fish, nu…) work normally. Browse mode can still move around and copy lines there, but can't re-run commands. You can turn integration off in Settings.

**Claude.** Requests go from Go straight to the Claude API, using streaming, adaptive thinking and server-side fallbacks. The key never reaches the web view.

## Develop

Requires Go 1.25+, Node 20+, and the Xcode command-line tools.

```bash
go install github.com/wailsapp/wails/v2/cmd/wails@v2.17.0
```

```bash
~/go/bin/wails dev
```

```bash
go test ./...
```

The tests start real zsh, bash, tcsh and sh sessions and check the integration marks, exit codes and folders. The AI tests use a fake streaming server.

Every push builds the app on GitHub Actions. To publish a release with the `.dmg`, run the *Build macOS app* workflow with a version number (Actions → Run workflow), or push a tag like `v1.2.0`.

Debug switches: `JAMALTERM_NO_GPU=1` draws without WebGL; `JAMALTERM_ANTHROPIC_BASE_URL` points AI requests at another server.
