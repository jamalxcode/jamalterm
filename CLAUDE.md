# jamalterm

A macOS terminal in Go (Wails v3) with xterm.js: browse mode (re-run past commands), split panes, and an AI panel. Read **docs/DESIGN.md** before changing behaviour; it records every design decision and why. Update it in the same commit when you make or change one.

## Layout

- `*.go` (main package): app service (`app.go`), windows and sessions (`windows.go`), the command table that drives the menu bar, palette and ⌘ keys (`commands.go`, `menu.go`), Settings window (`settingswindow.go`), safety (`safety.go`: busy check, native alerts, Secure Keyboard Entry, reattach), right-click menu and window frames (`contextmenu.go`), self-test (`smoke.go`).
- `internal/`: `shell` (pty, pump, integration scripts), `ai`, `settings` (settings.json, encrypted keys), `update` (updater and Sigstore check), `winstate`, `diag` (problem log), `secureinput`.
- `frontend/`: two pages, `index.html` (terminal window, `src/main.ts`) and `settings.html` (`src/settingsPage.ts`). All calls to Go go through `src/backend.ts`.

## Build and test

```bash
cd frontend && npm ci && npm run build && npm run test:ui   # UI tests + theme contrast
```
```bash
go test ./internal/... && go test .      # `go test .` needs frontend/dist built
```

- **Bindings:** after changing an exported `App` method, run `make bindings` (needs `wails3`, `go install github.com/wailsapp/wails/v3/cmd/wails3@v3.1.0`). It regenerates `frontend/bindings` and the method list in `src/backend.ts`. CI fails if they're stale.
- **Linux (dev only):** use `-tags gtk3` (`make linux`). There ⌘ becomes Ctrl+Shift and ⇧⌘ becomes Ctrl+Alt+Shift.
- **Real-app check without a Mac:** run under `xvfb-run` with `JAMALTERM_SMOKE=<dir>` (the app tests itself and writes `report.json` and screenshots).

## Rules

- **Commands and shortcuts live only in `commands.go`.** Never add a separate key map or palette list. Every plain command needs a case in `runMenu` (`frontend/src/main.ts`).
- **Providers live only in `internal/ai/openai.go` (`Providers`).**
- **Never log or send** commands, terminal output, AI questions or keys. The problem log holds errors only, scrubbed.
- **API keys go in the encrypted file (`internal/settings/secret.go`), never the Keychain.** Nothing may trigger a password prompt unless the user clicks for it.
- **Follow Apple's Human Interface Guidelines and Terminal.app:**
  - native alerts, with Cancel as the default for destructive actions;
  - Settings changes apply instantly;
  - menu items say what they will do;
  - don't rely on color alone (the owner is color-blind).
- **Keep a test with each change:** Go tests next to the code, UI tests in `frontend/tests/ui.test.mjs` (with the fake Go side in `tests/fake-wails.js`).

## Commits and releases

- **Commit as the user,** with short, specific messages, and push to `main`:
  ```
  git -c user.name="jamal xcode" -c user.email="j.mutawa@gmail.com" commit -m "..."
  ```
- **CI** (`.github/workflows/build.yml`) runs on a real Mac. It runs:
  - the Go and UI tests;
  - the bindings check;
  - a universal build;
  - the smoke test;
  - the Sigstore proof and its check.
- **To release:** add a `## X.Y.Z` section to `CHANGELOG.md`, then run the workflow with `version` (and `prerelease` for the Beta channel). Release notes come from that section plus `.github/release-notes.md`.
