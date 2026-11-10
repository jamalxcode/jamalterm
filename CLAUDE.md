# jamalterm

A macOS terminal with browse mode (re-run past commands), split panes and an AI panel. **Being rewritten in Rust with a native UI** (4.x). The last Go/Wails release is 3.5.0; its code is in git history (tag `v3.5.0`) and its behaviour, recorded in **docs/DESIGN.md**, is the spec to port. Read DESIGN.md before changing behaviour, and update it in the same commit when you make or change a decision.

## Layout (Rust, `src/`)

- `main.rs` (window setup), `app.rs` (tabs, tab bar, keyboard/mouse, window title), `view.rs` (drawing the terminal: runs of cells, cursor, selection, ✓/✕ marks).
- `session.rs` (a tab's shell: PTY via `portable-pty`, emulator via `alacritty_terminal`, commands built from the marks), `marks.rs` (OSC 133 / 7 / 6973 scanner), `shell.rs` (which shell, its environment, loading the hooks), `input.rs` (keys to bytes, paste).
- `settings.rs` (settings.json, same file and fields as 3.x), `themes.rs` (the 10 themes), `fonts.rs`, `smoke.rs` (self-test).
- `resources/integration/` (the shell hook scripts, unchanged from 3.x), `assets/fonts/`.

## Build and test

```bash
cargo test                                   # unit tests + real zsh/bash/tcsh/fish
cargo fmt --check && cargo clippy --all-targets -- -D warnings
```
```bash
cargo run                                    # the app (Linux needs libxkbcommon-x11-0)
```

- **Self-test:** `JAMALTERM_SMOKE=<dir> JAMALTERM_SETTINGS=<dir>/settings.json cargo run` types into its own shell, checks what it sees and writes `<dir>/report.json`. Without a Mac: run it under `xvfb-run -a`.
- **Shell setups test:** set `JT_SHELL_SETUPS` to a folder with clones of ohmyzsh, powerlevel10k and prezto (CI's `shells` job does) and run `cargo test real_shell`.
- **Mac app:** `scripts/build-mac.sh [version]` builds `build/bin/jamalterm.app` (Apple silicon + Intel, bundle id `com.jamalxcode.jamalterm`, as in 3.x).

## Rules

- **Never log or send** commands, terminal output, AI questions or keys.
- **API keys go in an encrypted file, never the Keychain** (3.x's design, DESIGN.md §8). Nothing may trigger a password prompt unless the user clicks for it.
- **Follow Apple's Human Interface Guidelines and Terminal.app:** native alerts with Cancel as the default for destructive actions; Settings changes apply instantly; menu items say what they'll do; **don't rely on color alone** (the owner is color-blind; status is ✓/✕/… in shape too).
- **Keep a test with each change**, next to the code (`#[cfg(test)]`). Real-shell tests live in `session.rs`.
- **settings.json stays compatible** with 3.x: never rename fields; keep fields not used yet.

## Commits and releases

- **Commit as the user,** with short, specific messages, and push to `main`:
  ```
  git -c user.name="jamal xcode" -c user.email="j.mutawa@gmail.com" commit -m "..."
  ```
- **CI** (`.github/workflows/build.yml`): fmt + clippy, tests and the self-test on Linux, the shell-setups job (Linux and macOS), and on a Mac the tests, a universal build and the self-test.
- **Releases are off** until the Rust app matches 3.5.0 (DESIGN.md, "Rust port", lists what's left). They come back with the updater, its Sigstore check and the publish job; then follow the **release** skill. Never push a `Release X.Y.Z` commit before that.
