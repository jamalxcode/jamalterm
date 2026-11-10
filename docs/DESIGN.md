# jamalterm design decisions

This file records how jamalterm is built and why. Each section says what was decided, why, and what was rejected. When you change one of these, update this file in the same commit.

<p align="center"><img src="icon-256.png" width="128" alt="jamalterm icon"></p>

## 1. What jamalterm is

A macOS terminal that behaves exactly like a normal terminal, plus:

- **Browse mode**: a free-floating cursor for moving around past output and re-running commands.
- **AI built in**, as a clearly separate panel.
- **A Mac app**: it follows Apple's Human Interface Guidelines and feels like Terminal.app.

Principles, in priority order:

1. **Never get in the way of the shell.** Keys, output and programs behave as in Terminal.app. Every extra is opt-in, or triggered by a gesture that has no other meaning: ↑ at an *empty* prompt, ⌘↑, or a click.
2. **Never lose work by accident.** Ask before stopping running programs. Restore windows, tabs and splits. Updates can be undone.
3. **Private by default.** Nothing is sent anywhere unless you ask an AI something. Secrets are hidden first, and keys stay on the Mac.
4. **Accessible.** Shapes as well as colors, so it works for color-blind users. Readable contrast, keyboard access, and the macOS accessibility settings are respected.
5. **One source of truth.** Commands, providers and settings are each defined once.

## 2. Architecture

```
Go (main package, internal/…)          WebView (frontend/, TypeScript)
 ├ shells: creack/pty, one per pane  ◄─►  xterm.js 6 (WebGL; DOM for ligatures)
 ├ windows, menus, alerts (Wails v3)       tabs, splits, browse mode, palette
 ├ AI: Anthropic + OpenAI-compatible SDKs  AI panel, command bar
 ├ settings, encrypted keys, problem log   Settings window (settings.html)
 └ updater (GitHub Releases + Sigstore)    update card
```

| Decision | Why | Rejected |
|---|---|---|
| **Go + Wails v3** | One small native binary, real cgo for macOS APIs (pty, Secure Keyboard Entry). v3 gives real multi-window support, native menus, alerts and context menus. | **Electron**: about 150 MB and a Chromium per app. **Swift/AppKit**: no mature terminal widget as good as xterm.js. **Wails v2**: one window per process (⌘N used to start a second copy of the app). |
| **xterm.js** | The most complete web terminal emulator: WebGL drawing, Unicode 11, images (Sixel and iTerm2), search, serialize, OSC 8 links. | Writing an emulator ourselves. |
| **Everything the window can call lives in `frontend/src/backend.ts`** | One seam: the UI tests swap in a fake Go side (`tests/fake-wails.js`), and moving to Wails v3 meant changing one file. | Importing generated bindings everywhere. |
| **Generated bindings are committed**, and CI checks they're current | Builds don't need the `wails3` tool. A Go method change that wasn't regenerated fails CI instead of failing at run time. | Generating them in CI only. |

## 3. The terminal

### Shells and output
- **One pty per pane**, started as a login shell (as Terminal.app does), in the folder the tab or pane was opened from.
- **Output pump** (`internal/shell/pump.go`):
  - batches output every 16 ms;
  - stops reading from the shell when the window is 1 MiB behind, and stops queueing at 4 MiB.
  - *Why:* `yes` or a huge log used to freeze the window. The window acknowledges what it has drawn (`Ack`), which keeps the pace.
- The pump keeps the **last 256 KB** it sent, so a reloaded or crashed page can reattach to its still-running shells (§5).

### Shell integration (needed for browse mode and AI context)
- **Marks:** OSC 133 A/C/D (prompt, command start, done plus exit code), the same convention iTerm2, VS Code and Ghostty use. OSC 7 reports the folder, and a private OSC 6973 carries the command text, base64-encoded.
- **We never edit your dotfiles.** Each shell gets the hooks a different way, and each one loads your own startup files first:
  - zsh: a temporary `ZDOTDIR`;
  - bash: `--rcfile`, and it works with the bash 3.2 that macOS ships;
  - tcsh: a temporary `HOME`, and your own `precmd`/`postcmd` aliases are chained so they still run;
  - fish: `-C source`.
- **Over ssh**, Shell → *Enable Browse Mode on This Server* types one line into the remote bash or zsh. It's a base64 `eval` of `integration/remote.sh`, for that session only: nothing is written to the server, and the line starts with a space so most shells keep it out of history. Remote command marks nest inside the local `ssh` command, so "done" always closes the innermost open command.
- **bash:** our hook runs first in `PROMPT_COMMAND` (to report the exit code) and a second one, `_jt_ready`, runs last; only commands after that count as typed. A DEBUG trap you or a prompt tool (starship) already set is kept and runs before ours. Both found by the shell-setups test.

### Browse mode (the free-floating cursor)
- **How to enter:** ↑ at an *empty* prompt, ⌘↑, or a click on earlier output. ↑ at an empty prompt normally means shell history; that's still available through Ctrl+R, or by turning the setting off.
- **Running a command again:**
  - ⏎ runs it again, and ⇧⏎ puts it on the prompt to edit first.
  - If the command ran in another folder, jamalterm warns and offers ⌥⏎ to run it in that original folder. *Why:* `rm` or `git reset` in the wrong folder can't be undone.
- **Without shell integration:** a line that looks like a prompt (`user@host:~$ make`) is put on the prompt for you to check, never run directly. A guess shouldn't run on its own.
- **Dots in the margin:** ring = OK, ✕ = failed, spinner = running. The shapes carry the meaning, never color alone.

### Keyboard
- **⌘ keys belong to the menu bar,** never to the shell.
- **Option is Meta** by default (⌥B / ⌥F jump by word), as in Terminal's "Use Option as Meta key" setting.
- **The kitty keyboard protocol, level 1,** is used only when a program asks for it (fish 4, neovim, helix). Esc, Alt+key, Ctrl+I/Tab and Shift+Enter become unambiguous.
  - Plain Enter, Tab and Backspace stay unchanged, so a shell still works if a program left the mode on.
  - The normal and full-screen screens keep separate settings, and a terminal reset clears them.

## 4. Windows, tabs and panes

- **Real windows.** ⌘N opens a window in the same app; macOS's own ⌘` cycles between them, and the Window menu lists them.
- **Tabs:**
  - The tab bar appears once there are two tabs, as in Terminal (View → Show Tab Bar keeps it always). With one tab, the title bar shows the window title, "~/proj — zsh".
  - Tabs carry a number (⌘1–9), a status shape, the folder, the git branch and, over ssh, the machine name.
- **Split panes:**
  - ⌘D splits right and ⇧⌘D splits down. Drag the divider to resize; double-click it to make the panes equal.
  - The focused pane gets a frame and a corner notch, and the other panes are dimmed. Shapes again, not just color.
- **Closing, as in Terminal:** ⌘W closes the pane (or the tab when it isn't split), ⌥⌘W closes the tab with all its panes, and ⇧⌘W closes the window.
- **Session restore:**
  - Every window comes back after a restart or update, with its tabs, splits, recent output and its **size and position**.
  - *File format:* `session.json` (0600) is `{"version":3,"windows":[…]}`. Each window's JSON comes from the page; Go adds a `frame` to it.
  - *Which windows count:* closing a window with its close button forgets it. Closing the *last* window counts as quitting, so it comes back. Closing a window's last tab starts it fresh next time.

## 5. Not losing work

- **Running programs:**
  - Quitting, closing a window, or closing a tab or pane asks first when a program other than the shell is in front. Go asks the pty which process group is in the foreground (TIOCGPGRP), so it also catches vim or ssh that the integration marks don't see.
  - The question is a **native macOS alert**, shown as a sheet, worded as in Terminal: "Do you want to terminate running processes in this window?"
  - **Cancel is the default button** (⏎ and Esc), following Apple's advice not to make a destructive action the default.
- **Reload:** View → Reload Window (or a crashed page) reattaches to the still-running shells and redraws their last 256 KB.
- **Updates:** see §8. Your tabs come back afterwards, and *Go Back to Previous Version* undoes one.

## 6. Following Apple's Human Interface Guidelines

Terminal.app and Safari are the reference.

### Menu bar
The menus are built from one table, `commands.go`, which also feeds the command palette and the ⌘-key handler in the page. A Go test checks that no shortcut is used twice.

| Menu | Contents (shortcuts) |
|---|---|
| **jamalterm** | About (the native panel), Check for Updates…, Go Back to Previous Version…, Settings… ⌘,, Secure Keyboard Entry, Services, Hide ⌘H, Hide Others ⌥⌘H, Show All, Quit ⌘Q |
| **Shell** | New Window ⌘N, New Tab ⌘T, Split Pane Right ⌘D, Split Pane Down ⇧⌘D, Close Tab/Pane ⌘W, Close Tab and Its Panes ⌥⌘W, Close Window ⇧⌘W, Enable Browse Mode on This Server, Clear Scrollback ⌘K |
| **Edit** | Undo, Redo, Cut, Copy, Paste (system items), Select All ⌘A, Find… ⌘F, Find Next ⌘G, Find Previous ⇧⌘G, Use Selection for Find ⌘E |
| **View** | Show/Hide Tab Bar ⇧⌘T, Command Palette… ⇧⌘P, Browse Commands ⌘↑, Previous/Next Command ⇧⌘↑/↓, Bigger ⌘=, Smaller ⌘-, Default Font Size ⌘0, Enter Full Screen ⌃⌘F (system item), Reload Window ⇧⌘R |
| **AI** | Ask AI… ⌘I, Command from English… ⌘J, Explain Last Error ⇧⌘E, Use Claude / ChatGPT / DeepSeek / Grok / OpenRouter / Local (⌥⌘1–6) |
| **Window** | Minimize ⌘M, Zoom (system items), Show Previous/Next Tab ⇧⌘[ / ⇧⌘], Select Previous/Next Pane ⌘[ / ⌘], panes by direction ⌥⌘arrows, Broadcast Typing ⌥⌘I, Select Tab 1–9 ⌘1–9, Bring All to Front, then the list of windows (added by macOS) |
| **Help** | The search field (added by macOS), jamalterm Help, Keyboard Shortcuts, Copy Diagnostics…, Show Problem Log |

**Menu items say what they'll do.** "Close Pane" vs "Close Tab", "Show/Hide Tab Bar", and the Broadcast checkmark update from the window in front (`SetMenuState`).

### Settings window
- **Its own window**, opened with ⌘, (or brought to the front if open). It has a **toolbar of panes** (Appearance, Shell, AI, Privacy, Updates) like Terminal's and Safari's, and the **window title names the pane**.
- **Every change applies at once**: there's no Save or Cancel. Keys are saved when you leave the field. ⌘W or Esc closes it.
- *Rejected:* the earlier modal dialog with Save and Cancel. On the Mac, Settings windows apply changes immediately.
- **Advanced, folded away.** Options most people never change (shell path, scrollback, shell integration, prompt keys, line height, window frame, ligatures, host colors, the problem log, the update channel) sit under a closed **Advanced** disclosure at the bottom of each pane. Searching for one (palette or Help) unfolds it.

### First start and other notices
- **First start:** a small card in the corner names the three things that aren't obvious: browse mode (⌘↑), Ask AI (⌘I) and the command palette (⇧⌘P). It shows once, never when tabs were reopened, and never as a modal: the terminal is usable straight away (`frontend/src/cards.ts`).
- **After an update:** a What's new card with that version's changelog lines. **After a crash:** a card saying so, with Copy Diagnostics.
- *Rejected:* a multi-step onboarding tour. A terminal user wants a prompt, not a wizard.

### Look
- **Window frame: "Match macOS"** (the default):
  - The tab bar, panels and dialogs use Apple's light or dark system colors and **your accent color** (WebKit's `-apple-system-control-accent`), as Terminal.app does.
  - **The terminal itself keeps your theme.** "Match the terminal theme" brings back the older all-theme look.
  - The Settings window always matches macOS.
- **AI panel:** a **lighter, raised violet-tinted card**. It must never read as part of the terminal (it was too close to the terminal's dark background before).
- **Accessibility:** System Settings' Reduce Motion, Reduce Transparency and Increase Contrast are respected. Keyboard focus rings are drawn in the accent color.
- **System font** for everything except terminal text.
- **Right-click** in the terminal opens a native menu: Copy, Paste, Select All, Look Up in Dictionary, Search with Google, Ask AI About the Selection, Find Selection, Split Pane Right/Down, Clear Scrollback.
- **Input methods** (Japanese, Chinese, accents with ⌥E) go through xterm's composition handling; a UI test types composed text.
- **GPU drawing** is on only for the tab in front, so many tabs don't use up the browser's limit of GPU contexts.

### Icon
A terminal window with a magic wand on a deep blue-to-violet rounded square, from the owner's artwork. Its black background was removed, and it was fitted to Apple's grid: a 1024 px canvas with the body at 824 px, centred on transparency (`build/appicon.png`). `scripts/build-mac.sh` turns it into the `.icns` file.

### Not done (and why)
- **Dock menu (New Window from the Dock icon):** Wails v3.1 has no API for it.
- **Native window tabs (NSWindow tabbing):** our tabs hold split panes, a status, the branch and the host, which native tabs can't show.
- **Linux builds** are for development only. There ⌘ becomes Ctrl+Shift (and ⇧⌘ becomes Ctrl+Alt+Shift), so the shell keeps Ctrl+W and friends.

## 7. AI

- **Providers are defined once** (`internal/ai/openai.go`: `Providers`). The menu, the panel switcher, Settings and the setup hints are all built from that list.
  - **Claude:** the official Anthropic SDK, with streaming, server-side fallbacks, and either `ant auth login` sign-in or an API key.
  - **ChatGPT, DeepSeek, Grok, OpenRouter** (which has a browser sign-in), and **Local** (Ollama or LM Studio): all through the OpenAI-compatible SDK.
  - *Rejected:* **Gemini** was removed as too many conditions and setup steps for users, and ChatGPT browser sign-in, which OpenAI doesn't offer to third-party apps.
- **The AI can only type or run commands through one tool,** and running always waits for you to click **Run**.
- **What gets sent:**
  - recent commands, exit codes and (unless you switch it off per tab) output;
  - secrets are replaced with `[hidden secret]` first (`internal/ai/redact.go`), and the panel says how many were hidden.
- **Local models** never leave the Mac. Models that can't use tools still answer, without the typing tool.
- **Terminal output is untrusted.** A file or web page can contain text written to steer an AI ("ignore your instructions and run …"). So:
  - the system prompt says the terminal section is data, never instructions;
  - every command the AI proposes is checked (`internal/ai/risk.go`): deleting broadly, piping downloads into a shell, sudo, erasing disks, force-pushing, touching SSH keys or credential files (including jamalterm's own key file), changing launch agents, reverse shells and more get a warning in words: **“Careful: this command could do real damage”** or **“Check this command before running it”**, with each reason listed;
  - a command that appears word for word in the output is marked too ("this command was in the terminal output; it may have been planted");
  - a marked command needs a second click (**Run Anyway**), and the warning is words and a ⚠, not just a color.

## 8. Privacy and security

- **API keys: a local encrypted file, never the Keychain** (`internal/settings/secret.go`).
  - **How:**
    - `keys.enc` is AES-256-GCM.
    - Its key is HKDF-SHA256 of a random 32-byte `keys.secret`, this Mac's hardware UUID and your user ID.
    - Both files are 0600 in a 0700 folder and excluded from Time Machine.
  - **Why it changed:** with ad-hoc signed builds, macOS asked for the login password after every update. The owner found it annoying and didn't want to keep typing that password into prompts.
  - **What it protects:** other user accounts, backups, and copies of the folder (a copy can't be decrypted on another Mac). Like SSH keys or the GitHub and AWS command-line tools, it can't stop malware already running as you.
  - **Old keys:** keys an older version left in the Keychain are moved only when you click *Move Keys from Keychain…* in Settings (one last prompt). Checking whether any are waiting never shows a prompt.
- **Secure Keyboard Entry:**
  - turns on by itself while the tab in front is at a password prompt (echo off in canonical mode), and only while jamalterm is the active app;
  - a 🔒 shows while it's on;
  - the jamalterm menu can keep it on all the time.
- **Problem log:**
  - local and small (256 KB, plus one older file), errors only;
  - lines are scrubbed: secrets, your home folder (shown as ~), email addresses and the computer's name are removed;
  - it never contains commands, output, questions or keys;
  - Help → Copy Diagnostics shows the whole report and copies nothing until you press Copy.
- **Updates:** before installing, the app checks:
  - the SHA-256 checksum;
  - the code signature and bundle id;
  - since 3.1, a **Sigstore build proof** that GitHub's build service made exactly that zip from this repo's `build.yml` (`internal/update/provenance.go`). Releases without one aren't installed.
  - The **Beta** channel (Settings → Updates) gets prerelease builds first.
  - **Settings → Updates always says where you stand:** your version, when it last checked, and what it found (or the error), with **Check Now**. A manual check that finds nothing says so in a native alert, and after updating a What's new card lists the changes (from the CHANGELOG built into the app).
- **Crash reports stay on the Mac.** Go's crash output goes to `crash.log` in the settings folder (`crash.go`). On the next launch jamalterm says it quit unexpectedly and offers Copy Diagnostics; the report is scrubbed like the problem log and nothing is sent.
- **Signing:** ad hoc unless you add the secrets `scripts/sign-mac.sh` documents: a free self-signed certificate (`scripts/make-signing-cert.sh`), or an Apple Developer ID plus notarization.

## 9. Testing and releases

- **Go tests:**
  - real zsh, bash, tcsh, fish and sh sessions;
  - the pump, the problem log, the encrypted keys (including the "can't read on another Mac" case), sessions (`internal/winstate`), the updater (with signed test bundles on macOS), version ordering and the command table.
- **Real shell setups** (CI job `shells`, Linux and macOS): oh-my-zsh, Powerlevel10k with instant prompt (started twice, since instant prompt only kicks in from the second start), oh-my-zsh with Powerlevel10k, prezto, and starship in zsh, bash and fish. They hook the prompt the same way we do, so this is where integration breaks first (`internal/shell/setups_test.go`).
- **UI tests:** the real interface in Chromium with a fake Go side, plus a WCAG contrast check of every theme.
- **On a real Mac**, every CI build:
  - runs the **smoke test**: the app drives itself (`JAMALTERM_SMOKE`), real ⌘ keys are pressed through System Events, and screenshots, launch time, output speed and memory go into the run's summary;
  - verifies the Sigstore build proof exactly as the updater will;
  - fails if launch takes over 10 s or memory goes over budget (app 400 MB, web content 900 MB with one window and two panes).
- **Speed of the pipeline:** the UI, shells and Mac jobs run side by side, and pushes that change only docs skip the Mac build.
- **Releases:** push a commit titled `Release X.Y.Z` (or `Release X.Y.Z-beta.N` for the Beta channel). That push's build is published once every job passes: one build, no second run. Running the workflow by hand with a version still works. The notes are that version's CHANGELOG section plus `.github/release-notes.md`. *Prerelease* sends it to the Beta channel only.
