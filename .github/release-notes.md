## Install

1. Download **jamalterm-…dmg** below and open it.
2. Drag **jamalterm** onto **Applications**.
3. The app isn't notarized by Apple yet, so macOS blocks the first launch. Open Terminal once and run:
   ```
   xattr -dr com.apple.quarantine /Applications/jamalterm.app
   ```
   (Or: try to open it, then go to System Settings → Privacy & Security → **Open Anyway**.)
4. For AI features, add your Claude API key in jamalterm → Settings (⌘,).

Works on Apple silicon and Intel Macs, macOS 10.15 or later.

## What's inside

- Your normal shell (zsh, bash, tcsh) with 10 color themes, a blinking cursor, tabs, find, and font zoom
- **Browse mode**: ⌘↑ or click any earlier line, move with the arrow keys, press ⏎ to run that command again
- Green/red dots next to every command; click a red one to ask Claude why it failed
- ⌘I: ask Claude about your terminal; ⌘J: describe a command in plain English
