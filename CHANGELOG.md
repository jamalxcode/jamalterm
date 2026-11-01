# Changelog

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
