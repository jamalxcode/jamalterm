<p align="center"><img src="docs/icon-256.png" width="128" alt="jamalterm icon"></p>

# jamalterm

A terminal for macOS with a **free-floating cursor** and **AI built in**. Move up through everything you've run with the arrow keys or the mouse, press Enter, and the command runs again. Ask Claude (or ChatGPT, DeepSeek, Grok, OpenRouter, or a model running on your own Mac) why something failed, or describe a command in plain English.

It runs your normal shell (zsh, bash, tcsh, fish…), so everything you already use keeps working, and it looks and behaves like a Mac app: the standard menus and shortcuts, a Settings window that applies changes immediately, native alerts and right-click menus, and light or dark to match your Mac. How and why it's built this way: **[docs/DESIGN.md](docs/DESIGN.md)**.

## Download

**[Latest release →](https://github.com/jamalxcode/jamalterm/releases/latest)**: download the `.dmg`, open it, and drag jamalterm into Applications.

The app isn't notarized by Apple (that needs a paid developer account), so macOS blocks the first launch. Run this once:

```bash
xattr -dr com.apple.quarantine /Applications/jamalterm.app
```

Or try to open it, then click **Open Anyway** in System Settings → Privacy & Security. Universal build: Apple silicon and Intel, macOS 12 (Monterey) or newer.

### Updating

You only install by hand once (after that, open **jamalterm** from Applications or Spotlight). After that, jamalterm checks GitHub at launch and every 6 hours and offers new versions. Before installing one it checks:

- the SHA-256 checksum, the code signature and the bundle id;
- since 3.1, a **build proof**: a Sigstore attestation that GitHub's own build service made exactly that file from this repository's workflow. A release without one isn't installed.

Your tabs come back after the update, a short *What's new* card lists the changes, and **jamalterm → Go Back to Previous Version** undoes it. **Settings → Updates** shows your version, when it last checked and what it found, with *Check Now*, and has a **Beta** channel if you'd like test versions first.

**Optional, once:** to sign every release with the same certificate, run `scripts/make-signing-cert.sh` on your Mac (it needs the GitHub CLI). It makes a free self-signed certificate and saves it as a GitHub secret, so every release is signed the same way. With a paid Apple Developer ID (add `MAC_SIGNING_P12` and the `APPLE_*` secrets), releases are also notarized and the `xattr` step goes away.

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

### AI assistant: Claude, ChatGPT, DeepSeek, Grok or OpenRouter

Pick the assistant in **Settings (⌘,) → AI**, or switch on the fly from the **AI** menu (⌥⌘1 Claude, ⌥⌘2 ChatGPT, ⌥⌘3 DeepSeek, ⌥⌘4 Grok, ⌥⌘5 OpenRouter) or the name at the top of the AI panel. Claude can use **Sign in with Anthropic** or an API key, and OpenRouter can use **Sign in with OpenRouter** (your browser approves and jamalterm saves the key) or an API key. ChatGPT, DeepSeek and Grok use an API key from [platform.openai.com](https://platform.openai.com/api-keys), [platform.deepseek.com](https://platform.deepseek.com/api_keys) or [console.x.ai](https://console.x.ai). **Local** uses Ollama or LM Studio on your Mac, with no key. The model is chosen automatically (the newest general model your key can use), or you can pick one with **Show available models**. Usage is billed to that provider's API account; a ChatGPT Plus subscription doesn't cover API use.


| Key | What it does |
|---|---|
| **⌘I** | Chat panel. Claude sees your recent commands, exit codes and output. Commands in answers have **Run** and **Insert** buttons. Ask it to *paste* a command and it types it onto your prompt; ask it to *run* one and you get a Run / Cancel prompt first, then Claude sees the result. |
| **⌘J** | Command bar: type "find big files in Downloads", get the command, press ⏎ to put it on the prompt (⌘⏎ runs it). |
| ⇧⌘E | Explain the last error |

To connect Claude, click **Sign in with Anthropic** in Settings (⌘,) or the AI panel. A tab opens, installs Anthropic's `ant` command-line tool with Homebrew if you don't have it, and signs you in with your browser (`ant auth login`). There's no key to copy, and the sign-in refreshes itself. Usage is billed to the Anthropic Console account you sign in with. You can also paste an API key from [console.anthropic.com](https://console.anthropic.com) instead, and a saved key takes priority over the sign-in.

**Where keys are kept.** API keys are stored in an encrypted file on your Mac (AES-256), readable only by your user account and tied to this computer: a copy of the file can't be opened elsewhere. jamalterm doesn't use the Keychain, so it never asks for your password. If an older version saved keys in the Keychain, Settings → AI offers to move them (macOS asks one last time), or you can simply enter them again. The default model is Claude Opus 5.5; Sonnet and Haiku are faster and cheaper options. You can turn off sending command output, or turn AI off completely.

### Everything else

- **⇧⌘P command palette**: type a few letters of any action, setting, theme, assistant or tab and press ⏎.
- **Reading long output**: a sticky header names the command you're scrolled into (with Fix it, Copy output and Skip to end), and markers beside the scrollbar show every command, failures in red.
- **Fix it**: one click on a failed command and the AI suggests a corrected one.
- **Files and folders**: drop files on the window to type their paths; press Space in browse mode for Quick Look; ⇧⌘O opens a recent folder; tabs in the same git project share a color.
- **Themes**: hover one in Settings to try it in your terminal, or pick *Match my wallpaper* or *Theme of the week*. Cursor effects: Smooth or Glow.
- **Small moments**: long commands get a "✓ done in 3m" when they finish, and Help → Your Day in jamalterm shows your stats (kept only on your Mac).
- **Split panes**: ⌘D splits right, ⇧⌘D splits down. Drag the line between panes to resize (double-click it to even them out). ⌘] / ⌘[ or ⌥⌘ arrows move between panes, or click one. ⌘W closes a pane, ⇧⌘W the whole tab.
- **Moving tabs**: drag a tab to reorder it, out of the window for a new window, or onto another window's tab bar; what's running keeps running. Window → Merge All Windows gathers them back.
- **Menu bar and Dock**: jamalterm's menu bar icon and its Dock menu (right-click) list every window and tab, so any terminal is one click away.
- **Windows**: ⌘N opens a new window in the current folder, ⌘` switches windows. Every window, with its tabs, splits and recent output, comes back after a restart or update. Turn this off in Settings → Shell; forget saved tabs in Settings → Privacy.
- **Nothing gets lost by accident**: quitting, closing a window, or closing a tab or pane while a program runs (vim, a build, ssh) asks first and names what's running. If a window's page reloads (View → Reload Window, ⇧⌘R) its shells keep running and reconnect.
- **Password prompts are protected**: when a program asks for a password (sudo, ssh…), macOS Secure Keyboard Entry turns on so other apps can't read your typing, and a 🔒 shows. jamalterm menu → Secure Keyboard Entry keeps it on all the time.
- **Over ssh**: Shell → Enable Browse Mode on This Server turns browse mode on in the remote bash or zsh for that session (nothing is written to the server). Without it, a line that looks like a prompt (`user@host:~$ make`) can still be put back on the prompt with ⏎.
- **Host colors** (Settings → Appearance): tabs connected to machines you name (`prod-*`) get a color and always show the machine's name.
- **Broadcast typing** (⌥⌘I) to every pane in a tab, for running the same thing on several servers.
- You get a notification when a long command (10 s or more, adjustable) finishes while jamalterm is in the background.
- ⌘-click file paths in the output to open them. Programs can copy to your clipboard, also over SSH/tmux (OSC 52), and show pictures (iTerm2 image protocol, Sixel).
- **Privacy**: jamalterm sends nothing anywhere on its own. If something goes wrong, **Help → Copy Diagnostics…** shows a report (version, macOS, settings and recent errors, with secrets, your home folder, email addresses and the computer's name removed) and copies it only when you press Copy. The problem log behind it stays on your Mac, holds errors only (never commands, output, questions or keys), and can be turned off in Settings → Privacy.
- **Local AI**: pick *Local model on this Mac* in Settings → AI to use Ollama or LM Studio. No key, and questions never leave your computer.
- **The AI can't be talked into anything quietly**: terminal output is treated as data, never instructions. Risky commands it suggests (deleting broadly, `curl … | sh`, sudo, erasing disks, force-pushing, touching SSH keys…) get a warning that says why, and need a second click; so does any command that appeared word for word in the output.
- **If jamalterm crashes**, the next launch says so and offers Copy Diagnostics. The report stays on your Mac.
- Right-click selected text for **Look Up in Dictionary** and **Search with Google**.
- Tested with **oh-my-zsh, Powerlevel10k (instant prompt), prezto and starship** in zsh, bash and fish.
- Before anything is sent to an AI, secrets in the terminal text (API keys, tokens, passwords, private keys) are replaced with `[hidden secret]`. A per-tab switch keeps a tab's output out of AI requests entirely.

- Tabs: ⌘T, ⇧⌘W, ⌘1–9, ⇧⌘[ / ⇧⌘]. New tabs open in the current folder.
- ⌘F find, ⌘K clear, ⌘= / ⌘- / ⌘0 font size.
- 10 themes (Jamal Night, Catppuccin, Dracula, Tokyo Night, One Dark, Nord, Gruvbox, Solarized dark and light, GitHub Light), with a font, cursor style and blink setting.
- See-through window: Settings → Window opacity (30–100%), with optional blur of what's behind.
- Font ligatures (=> != ->) for fonts that have them; clickable links that programs print (OSC 8, ⌘-click); the kitty keyboard protocol, so fish, neovim and helix can tell Esc from Alt and Shift+Enter from Enter.
- Every theme is checked for readable contrast (WCAG), and the focused pane has a frame and a corner notch, not just a color.
- Option works as Meta (⌥B / ⌥F jump words). Turn it off in Settings.
- Nerd Font icons (Powerlevel10k, eza, starship…) show up with any font. The icon set is built in.
- If you'd rather ↑ go to shell history at an empty prompt, turn that off in Settings. ⌘↑ always starts browse mode.

## How it works

```
┌────────────────── jamalterm.app (Wails) ──────────────────┐
│ frontend/src/   xterm.js terminal, tabs, browse mode,     │
│                 AI panel, command bar, settings           │
├────────────────── events / bound methods ─────────────────┤
│ app.go, windows.go  windows, sessions, AI bridge          │
│ commands.go         menu bar, palette and ⌘ keys (1 list)│
│ internal/shell      PTY + your shell + shell integration  │
│ internal/ai         Claude + OpenAI-compatible providers  │
│ internal/settings   settings.json + encrypted API keys    │
└───────────────────────────────────────────────────────────┘
```

**Shell integration.** To know where each command starts and ends, jamalterm loads a small hook script into zsh, bash and tcsh. It never edits your dotfiles: zsh gets a temporary `ZDOTDIR`, bash an `--rcfile`, and tcsh a temporary `HOME`. Each one hands straight back to your own startup files. The hooks print invisible marks (OSC 133, the same convention iTerm2, VS Code and Ghostty use), plus the command text and current folder. The scripts are in `internal/shell/integration/`. Other shells (fish, nu…) work normally. Browse mode can still move around and copy lines there, but can't re-run commands. You can turn integration off in Settings.

**Claude.** Requests go from Go straight to the Claude API, using streaming, adaptive thinking and server-side fallbacks. The key never reaches the web view.

## Develop

Built with [Wails v3](https://v3.wails.io). Requires Go 1.26+, Node 20+, and the Xcode command-line tools.

```bash
make mac && open build/bin/jamalterm.app
```

```bash
make test
```

On Linux (for development), install GTK 3 and WebKitGTK 4.1 (`libgtk-3-dev libwebkit2gtk-4.1-dev`) and run `make linux`; shortcuts use Ctrl+Shift instead of ⌘. After changing the Go methods the window calls, regenerate `frontend/bindings` with `make bindings` (needs `go install github.com/wailsapp/wails/v3/cmd/wails3@v3.1.0`).

The Go tests start real zsh, bash, tcsh, fish and sh sessions and check the integration marks, exit codes and folders. The AI tests use fake streaming servers, and the key tests check the encrypted file (including that a copy can't be read on another computer). The UI tests run the real interface in Chromium with a fake Go side:

```bash
cd frontend && npm run build && npm run test:ui
```

Every push builds the app on GitHub Actions, runs a smoke test of the real app on a Mac (screenshots, real ⌘ shortcuts, launch time, output speed and memory are in the run's summary and artifacts), and proves the build with Sigstore. To publish a test version, tick *prerelease* (only the Beta channel gets it). To publish a release with the `.dmg`, run the *Build macOS app* workflow with a version number (Actions → Run workflow), or push a tag like `v1.2.0`.

Debug switches: `JAMALTERM_NO_GPU=1` draws without WebGL; `JAMALTERM_ANTHROPIC_BASE_URL` points AI requests at another server.

## License

MIT. See [LICENSE](LICENSE).

## Contributing notes

[CLAUDE.md](CLAUDE.md) has the conventions (build, tests, the one command table, releases); [docs/DESIGN.md](docs/DESIGN.md) has the design decisions.
