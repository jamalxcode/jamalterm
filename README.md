# jamalterm

A macOS terminal written in Go. It runs your normal shell (zsh, bash, tcsh, fish…) and will grow a **free-floating cursor**: move up through earlier commands with the arrow keys or the mouse, press Enter, and run them again. AI add-ons come later.

Built with [Wails](https://wails.io) (Go backend, native macOS window) and [xterm.js](https://xtermjs.org) (the terminal engine inside VS Code).

## What works now (step 1)

- Starts your login shell from `$SHELL` (`zsh` on a default Mac) inside a real pseudo-terminal, so `vim`, `htop`, `less`, `tmux`, colors and the arrow-key history all behave as usual.
- **Jamal Night** color theme with truecolor and 256-color support, plus a blinking pink block cursor.
- GPU (WebGL) rendering, emoji and CJK widths, 10,000 lines of scrollback, clickable links.
- Mac shortcuts: Cmd+C / Cmd+V copy and paste, Cmd+K clears, Cmd+= / Cmd+- / Cmd+0 change the font size, and Option works as Meta (Option+B / Option+F jump words).
- The window title follows the shell (`user@mac: ~/code`). When the shell exits, any key starts a new one.

## Run it on your Mac

You need [Go](https://go.dev/dl/) 1.25+, [Node](https://nodejs.org) 20+ and the Xcode command-line tools (`xcode-select --install`).

```bash
go install github.com/wailsapp/wails/v2/cmd/wails@v2.17.0
```

```bash
git clone https://github.com/jamalxcode/jamalterm && cd jamalterm
```

```bash
~/go/bin/wails dev
```

`wails dev` opens the app with live reload: edit anything in `frontend/src` and the window updates instantly. To make a real app:

```bash
~/go/bin/wails build -platform darwin/universal
```

```bash
open build/bin/jamalterm.app
```

### Or download a build

Every push to `main` builds `jamalterm.app` on GitHub (Actions → *Build macOS app* → `jamalterm-macos` artifact). The app isn't signed yet, so after unzipping, clear the quarantine flag once:

```bash
xattr -dr com.apple.quarantine ~/Downloads/jamalterm.app
```

## How it fits together

```
┌──────────── jamalterm.app (Wails) ───────────┐
│ frontend/src/main.ts   xterm.js: draws the   │
│ frontend/src/theme.ts  screen, keys, colors  │
├──────────── events / bound methods ──────────┤
│ app.go                 Start, Write, Resize  │
│ internal/shell/        PTY + your shell      │
└──────────────────────────────────────────────┘
```

- `internal/shell` starts the shell in a PTY (via [creack/pty](https://github.com/creack/pty)) and streams its output. It has no GUI code, and its tests run real `zsh`, `bash`, `tcsh` and `sh`: `go test ./internal/...`
- `app.go` connects it to the window: output goes to the frontend as `pty:data` events, keystrokes come back through `Write`.
- `frontend/src/main.ts` sets up xterm.js. Change colors in `theme.ts`.

## Roadmap

1. ~~Skeleton: a shell in a window, colors, blinking cursor~~ ✅
2. Settings, themes, fonts, tabs
3. Shell integration (OSC 133): know where each command and its output start and end
4. **Browse mode**: a free-floating cursor that moves through past commands with the arrows or the mouse; Enter re-runs a command
5. AI: explain an error, turn plain English into a command, fix and re-run
