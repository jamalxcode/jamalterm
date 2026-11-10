# Changelog

## 4.0.0 (in progress, not released)

**Rewritten in Rust, with a native interface.** No web view: lighter and faster to start. So far: your shell with jamalterm's hooks, tabs, the 10 themes, the ✓/✕ command marks, scrollback, selection, copy and paste, and your existing settings. Now also: **browse mode** (↑ at an empty prompt), **find** (⌘F, ⌘G), the **Mac menu bar**, and ✓/✕ marks that stay on their commands when the scrollback is full. Then **split panes**, **several windows**, **tabs you can drag** (to reorder, or out into another window), and your **windows coming back** after a restart, from the same session file as 3.x. The rest of 3.5.0 comes back before this is released.

## 3.5.0

**Tabs that move**
- **Drag tabs** to reorder them
- Drag a tab **out of the window**: let go over another jamalterm window and it moves there, anywhere else and it opens in a new window right there. Whatever is running keeps running
- Esc while dragging puts the tab back
- Window → **Move Tab to New Window** and **Merge All Windows**

**Your terminals, one click away**
- **jamalterm in the menu bar**: every window and tab, plus New Window, New Tab and Show All Windows (hide it in Settings → Appearance)
- **Right-click the Dock icon** for the same list

## 3.4.0

**Clearer at a glance**
- A failed tab shows a red **!** (the ✕ is only for closing), tab numbers are readable, and idle tabs have no dot
- Commands that worked get a ✓ in the margin (✕ for failures, a spinner while running)
- Tabs say "Home" and "2 panes" in words, and no longer stretch across the whole window
- Browse mode's key bar shows the everyday keys; press **?** for all of them
- Tips no longer cover the AI panel or the palette

**Working with output**
- A **sticky header** names the command whose output you're reading, with its result, **Fix it**, **Copy output** and **Skip to end**
- **Markers beside the scrollbar** for every command, failures in red
- **Fix it**: the AI suggests a corrected command for a failed one (also F in browse mode)
- **Drop files** on the window to type their paths
- **Space** in browse mode opens **Quick Look** for the file under the cursor
- **Open Recent Folder** (⇧⌘O)
- Tabs in the same git project share a color

**Nicer to look at**
- Settings → Appearance: hover a theme to see it in your terminal; a live preview of your font and cursor
- New themes: **Match my wallpaper** and **Theme of the week**
- Cursor effects: **Smooth** or **Glow**
- The AI panel names the model ("Claude Opus 5.5") and has a clear assistant switcher
- The command palette shows Recent, Tabs and Actions with a symbol for each

**Just for fun**
- When a command that took over a minute finishes, a small "✓ done in 3m 12s" moment (sound optional)
- Help → **Your Day in jamalterm**: your commands today and this week (kept only on your Mac, program names only)

## 3.3.1

- The Window menu lists only the tabs that are open, each with its name ("Tab 2: jamalterm — vim"), instead of always showing Select Tab 1–9
- Windows are named in words ("Home — zsh"), so the list of windows in the Window menu no longer shows "~ — ~"

## 3.3.0

**Updates you can see**
- Settings → Updates shows your version, when jamalterm last checked and what it found, with **Check Now**
- Checking by hand when you're up to date says so; after an update, a short **What's new** card lists the changes
- New versions are published straight from the build that tested them, so they arrive sooner

**AI safety**
- Commands the AI suggests are checked: deleting broadly, piping a download into a shell, sudo, erasing disks, force-pushing, touching SSH keys or saved credentials and more get a warning that says why, and need a second click (**Run Anyway**)
- A suggested command that appears word for word in the terminal output is flagged, since a file or web page may have planted it
- The AI is told that terminal output is data, never instructions

**Crashes leave a trace (on your Mac only)**
- If jamalterm quits unexpectedly, the next launch says so and offers Copy Diagnostics. Nothing is sent

**Feels more like a Mac app**
- Right-click → **Look Up in Dictionary** and **Search with Google**
- Many tabs no longer slow drawing down (only the tab in front uses the GPU)
- Japanese, Chinese and accented typing are tested

**Less crowded**
- A small welcome card on first start: browse mode, Ask AI and the command palette
- Rarely used settings are under **Advanced** in each Settings pane (search still finds them)

**Works with your shell setup**
- Browse mode is now tested with oh-my-zsh, Powerlevel10k (instant prompt), prezto and starship (zsh, bash and fish)
- Fixed: in bash with starship (or anything else in `PROMPT_COMMAND`), browse mode recorded each command one step late, and starship's command timer stopped working

## 3.2.0

**A proper Mac app** (Apple's Human Interface Guidelines, Terminal.app as the model)
- The standard menu bar: native About, Services, Hide Others / Show All; Shell menu like Terminal's (⌘W close tab or pane, ⇧⌘W close window); Edit has Find Next ⌘G, Find Previous ⇧⌘G and Use Selection for Find ⌘E; the Window menu lists your windows; Help has the search field
- Menu items say what they'll do: "Close Pane" or "Close Tab", "Show Tab Bar" or "Hide Tab Bar"
- **Settings is its own window** (⌘,) with a toolbar of panes, and changes apply immediately: no Save button
- Confirmations are native macOS alerts, worded like Terminal's, with Cancel as the default
- Right-click in the terminal for Copy, Paste, Select All, Ask AI About the Selection, Split Pane and Clear Scrollback
- The window frame follows macOS: light or dark, and your accent color (the terminal keeps its theme; Settings → Appearance → Window frame to change it)
- With one tab, the title bar shows "~/folder — zsh" and the tab bar appears with the second tab, like Terminal (View → Show Tab Bar keeps it)
- Windows reopen at their size and position
- Respects Reduce Motion, Reduce Transparency and Increase Contrast; keyboard focus is always visible

**AI panel**
- A lighter, raised card, so it never looks like part of the terminal

**API keys: no more Keychain**
- Keys are kept in an encrypted file on your Mac (AES-256), readable only by you and useless if copied to another computer
- jamalterm never asks for your password anymore. Keys an older version saved in the Keychain: Settings → AI → *Move Keys from Keychain…* (macOS asks one last time), or enter them again

**New icon**


## 3.1.0

**Nothing lost by accident**
- Quitting, closing a window, or closing a tab or pane while a program is running (vim, a build, ssh…) asks first and names what's running
- When a window's page reloads (View → Reload Window, or if it crashes) its shells keep running and reconnect, with their recent output

**Privacy and security**
- Password prompts turn on macOS **Secure Keyboard Entry** (other apps can't read your typing) and show a 🔒; or keep it on all the time from the jamalterm menu
- Updates are installed only with a **build proof** (Sigstore): GitHub's build service proves it made exactly that download from jamalterm's own workflow
- **Local AI**: Ollama or LM Studio on your Mac. No key, and your questions never leave the computer
- Optional: a stable signing certificate (`scripts/make-signing-cert.sh`) so macOS stops asking about the Keychain after updates

**Over ssh**
- **Shell → Enable Browse Mode on This Server**: browse mode in the remote bash or zsh for that session, without writing anything there
- Lines that look like a prompt (`user@host:~$ make`) can be put back on the prompt even without it
- **Host colors**: tabs connected to machines you name get a color and always show the machine's name
- **Broadcast typing** (⌥⌘I) to every pane in a tab

**Terminal**
- Links programs print (OSC 8) open with ⌘-click
- Font ligatures (Settings → Appearance)
- The kitty keyboard protocol: fish, neovim and helix can tell Esc from Alt+key and Shift+Enter from Enter
- Find shows "2 of 7"

**Accessibility**
- The focused pane has a frame and a corner notch, not just a color
- Every theme is checked for readable contrast; Gruvbox's red and Solarized Light's green, yellow and cyan are a touch darker or lighter to pass

**Updates**
- A **Beta** update channel (Settings → Updates) for getting test versions first

**Under the hood**
- Every build is smoke-tested on a real Mac (screenshots, real shortcuts, timings, memory)
- Starts faster: the icon font no longer holds up the first tab
- One command list drives the menu bar, the palette and the shortcuts


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
