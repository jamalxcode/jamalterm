# Changelog

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
