# Changelog

## 3.0.0

**Real windows and split panes**
- **⌘N** now opens a real window in the same app (before, each window was a separate copy of jamalterm). **⌘`** switches between windows
- All your windows come back after a restart or update, each with its tabs, splits and recent output
- **Split panes**: ⌘D splits right, ⇧⌘D splits down; drag the divider to resize, double-click it to even out
- Move between panes with ⌘] / ⌘[, ⌥⌘ arrows, or a click. ⌘W closes a pane, ⇧⌘W the whole tab

**Command palette**
- **⇧⌘P**: type a few letters of any action, setting, theme, assistant or tab. Settings you can switch on and off flip right there; others open Settings at the right place

**Settings**
- Now in pages with a sidebar: Appearance, Shell, AI, Privacy, Updates

**Private diagnostics**
- **Help → Copy Diagnostics…** shows a report for when something goes wrong, and copies it only when you press Copy. Nothing is ever sent anywhere
- It's built from a small problem log on your Mac: errors only, never commands, output, questions or keys. Secrets, your home folder, email addresses and the computer's name are removed before anything is written
- Turn the log off (which deletes it) in Settings → Privacy

**Under the hood**
- Moved to Wails v3. jamalterm now needs macOS 12 (Monterey) or newer


## 2.0.0

**Tabs survive restarts**
- Your tabs come back after a restart or update: same folders, recent output, active tab
- Updates: **Update now** (tabs reopen) or **When I quit** (installs; new version next launch)
- jamalterm menu → **Go Back to Previous Version** if an update misbehaves

**Privacy and security**
- Secrets in your terminal (API keys, tokens, passwords, private keys) are hidden before anything is sent to an AI; the panel says how many
- Per-tab **Share this tab's output** switch in the AI panel
- API keys live in one Keychain item that only jamalterm can read without asking (after an update, macOS asks once: choose Always Allow)

**Speed**
- Heavy output (big logs, `yes`, builds) no longer freezes the window

**New**
- **⌘N** opens a new window
- A notification when a long command finishes while you're in another app
- **⌘-click** file paths in the output to open them
- Copy from programs to your Mac clipboard, also over SSH/tmux (OSC 52)
- Pictures in the terminal (iTerm2 image protocol, Sixel)
- **fish** shell support for browse mode
- VoiceOver support (Settings)

**Browse mode**
- Re-running a command that ran in another folder asks first: ⏎ here, ⌥⏎ in its original folder
- Vi-style prompt keys option
- The AI panel shows which model answered and how many tokens it used

## 1.9.1

- Gemini is removed again to keep the assistant choices simple. If you had picked Gemini, jamalterm switches back to Claude

## 1.8.0

- **OpenRouter** joins the assistants (AI menu → Use OpenRouter, ⌥⌘5): hundreds of models through one account
- **Sign in with OpenRouter**: approve in your browser and jamalterm saves the key for you; or paste an API key
- By default OpenRouter's Auto Router picks the model; or choose any model your account can use
- The AI panel's welcome text names the assistant you switched to

## 1.7.0

- **Grok** (xAI) joins Claude, ChatGPT and DeepSeek: AI menu → Use Grok (⌥⌘4), or Settings → AI assistant
- Uses an API key from console.x.ai (stored in your Keychain); the model is picked automatically from the ones your key can use, or choose one

## 1.6.1

- Switch assistants on the fly from the **AI** menu: Use Claude (⌥⌘1), Use ChatGPT (⌥⌘2), Use DeepSeek (⌥⌘3), with a checkmark on the active one
- Or click the assistant's name in the AI panel header to switch
- A note in the chat shows where you switched; each assistant keeps its own conversation

## 1.6.0

- Choose your AI assistant: **Claude**, **ChatGPT** (OpenAI) or **DeepSeek** in Settings → AI assistant
- ChatGPT and DeepSeek use an API key from your account (stored in your Keychain); the model is picked automatically, or choose one from the list your key can use
- Everything works with each assistant: the chat panel, typing and running commands (with your OK), the ⌘J command bar and explaining errors

## 1.5.0

- See-through window: Settings → Appearance → **Window opacity** (30–100%) shows your desktop behind the terminal, with a live preview while you drag
- **Blur what's behind the window** (on by default) gives the frosted macOS look; turning blur on or off applies after a restart
- Reverse-video text (vim's status bar, selections) stays readable when see-through

## 1.4.1

- Background tabs are easy to see: each is an outlined pill with readable text
- Every tab shows its number (1–9, the same as ⌘1–⌘9), so tabs in the same folder can be told apart
- The active tab is brighter, with a thicker outline, bold text and an underline

## 1.4.0

- Claude can now type into your terminal: ask it to "paste" or "type" a command and it puts it on your prompt
- Ask it to "run" something and the AI panel asks you first (Run / Cancel). Claude then sees the exit code and output and can carry on, for example to fix an error
- Nothing runs without your click; typing replaces whatever was on the prompt

## 1.3.0

- Tabs are easier to tell apart, including with color blindness: the active tab is a bright outlined pill with bold text and an underline; background tabs are dimmed
- Tabs show the folder (shortened like ~/…/src/app), the git branch, and the running program's name; hover for the full path and the last command's result
- Status shapes instead of color alone: a spinner while running, ✕ when the last command failed, a ring for success (in the margin too)
- The AI panel is its own tinted, rounded card, so it doesn't look like part of the terminal
- Only real shell commands from Claude get Run / Insert; text and ASCII art get Copy

## 1.2.0

- **Sign in with Anthropic**: connect Claude through your browser instead of pasting an API key (Settings → AI, or the AI panel). It uses Anthropic's `ant` CLI, which jamalterm installs with Homebrew if needed, and the sign-in refreshes itself
- Sign out from Settings; a saved API key still works and takes priority
- The update card shows only what's new

## 1.1.0

- Update from inside the app: jamalterm checks for new versions and shows a card. **Update & Restart** downloads, verifies and installs the new version (jamalterm → Check for Updates…)
- The repo is now public, so updates need no GitHub login

## 1.0.1

- Press ↑ at an empty prompt to start the free-floating cursor (⌘↑ and clicking still work too)
- Nerd Font icons from Powerlevel10k, eza and starship show up with any font
- Status dots sit on the prompt line, not the blank line above it
- Tabs line up with the window buttons

## 1.0.0

- First release: browse mode, shell integration for zsh, bash and tcsh, tabs, 10 themes, Claude chat, the command bar and explaining errors
