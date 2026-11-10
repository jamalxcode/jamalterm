//! Starting the user's shell: which shell, which arguments and environment,
//! and how jamalterm's hooks are loaded after the user's own startup files
//! (the user's files are never edited). Ported from 3.x's internal/shell.

use portable_pty::CommandBuilder;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The hook scripts, built into the app and written to disk on first use.
const SCRIPTS: &[(&str, &str)] = &[
    (
        "zsh/.zshenv",
        include_str!("../resources/integration/zsh/.zshenv"),
    ),
    (
        "zsh/integration.zsh",
        include_str!("../resources/integration/zsh/integration.zsh"),
    ),
    (
        "bash/rc.bash",
        include_str!("../resources/integration/bash/rc.bash"),
    ),
    (
        "fish/integration.fish",
        include_str!("../resources/integration/fish/integration.fish"),
    ),
    (
        "tcsh/.tcshrc",
        include_str!("../resources/integration/tcsh/.tcshrc"),
    ),
    (
        "tcsh/integration.tcsh",
        include_str!("../resources/integration/tcsh/integration.tcsh"),
    ),
    (
        "remote.sh",
        include_str!("../resources/integration/remote.sh"),
    ),
];

/// Where the scripts are installed (~/Library/Caches/jamalterm/shell-integration).
pub fn integration_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("jamalterm")
        .join("shell-integration")
}

/// Writes the scripts to dir, replacing older versions.
pub fn install_integration(dir: &Path) -> std::io::Result<()> {
    for (rel, body) in SCRIPTS {
        let target = dir.join(rel);
        if std::fs::read_to_string(&target).ok().as_deref() == Some(*body) {
            continue;
        }
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&target, body)?;
    }
    Ok(())
}

/// The user's login shell: $SHELL, then the account database, then /bin/zsh.
pub fn default_shell() -> String {
    if let Ok(s) = std::env::var("SHELL")
        && !s.is_empty()
    {
        return s;
    }
    if let Some(s) = shell_from_account() {
        return s;
    }
    for s in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
        if Path::new(s).exists() {
            return s.into();
        }
    }
    "/bin/sh".into()
}

fn shell_from_account() -> Option<String> {
    let user = std::env::var("USER").ok()?;
    if cfg!(target_os = "macos") {
        // macOS keeps accounts in Directory Services, not /etc/passwd.
        let out = std::process::Command::new("/usr/bin/dscl")
            .args([".", "-read", &format!("/Users/{user}"), "UserShell"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let f: Vec<&str> = text.split_whitespace().collect();
        return (f.len() == 2 && f[1].starts_with('/')).then(|| f[1].to_string());
    }
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.len() >= 7 && f[0] == user && f[6].starts_with('/')).then(|| f[6].to_string())
    })
}

fn base_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// How to start a shell: its arguments, and environment variables to add.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Arguments and environment that load jamalterm's hooks after the user's own
/// startup files. `args == None` means a login shell (what Terminal starts).
/// Unknown shells start unchanged.
pub fn with_integration(
    shell: &str,
    args: Option<Vec<String>>,
    dir: &Path,
    home: &str,
    zdotdir: Option<String>,
) -> Launch {
    let login = args.is_none();
    let mut args = args.unwrap_or_else(|| vec!["-l".into()]);
    let mut env = vec![(
        "JAMALTERM_INTEGRATION".to_string(),
        dir.display().to_string(),
    )];
    match base_name(shell) {
        "zsh" => {
            // zsh reads $ZDOTDIR/.zshenv first; ours restores ZDOTDIR, then zsh
            // carries on with the user's .zprofile, .zshrc, .zlogin.
            env.push(("ZDOTDIR".into(), dir.join("zsh").display().to_string()));
            if let Some(z) = zdotdir {
                env.push(("JAMALTERM_ZDOTDIR".into(), z));
            }
        }
        "bash" => {
            // --rcfile only applies to non-login shells, so rc.bash reads the
            // login files itself when a login shell was asked for.
            if login {
                env.push(("JAMALTERM_BASH_LOGIN".into(), "1".into()));
                args.clear();
            } else if !args.is_empty() {
                return Launch {
                    program: shell.into(),
                    args,
                    env,
                }; // custom arguments: leave alone
            }
            let mut a = vec![
                "--rcfile".to_string(),
                dir.join("bash/rc.bash").display().to_string(),
                "-i".into(),
            ];
            a.extend(args);
            args = a;
        }
        "fish" => {
            // fish runs -C commands after its own config.
            args.push("-C".into());
            args.push(format!(
                "source {}",
                shell_quote(&dir.join("fish/integration.fish").display().to_string())
            ));
        }
        "tcsh" | "csh" => {
            // tcsh has no --rcfile; point HOME at our .tcshrc for a moment.
            env.push(("HOME".into(), dir.join("tcsh").display().to_string()));
            env.push(("JAMALTERM_HOME".into(), home.into()));
        }
        _ => {}
    }
    Launch {
        program: shell.into(),
        args,
        env,
    }
}

/// The environment the shell gets: ours on top of the inherited one.
pub fn build_env(
    base: impl IntoIterator<Item = (String, String)>,
    shell: &str,
    extra: &[(String, String)],
) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = base.into_iter().collect();
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    env.insert("TERM_PROGRAM".into(), "jamalterm".into());
    env.insert("SHELL".into(), shell.into());
    // Apps launched from Finder get no locale; without one, zsh mangles UTF-8.
    let none = |k: &str| env.get(k).is_none_or(|v| v.is_empty());
    if none("LANG") && none("LC_ALL") && none("LC_CTYPE") {
        env.insert("LANG".into(), "en_US.UTF-8".into());
    }
    // Don't leak variables from whatever terminal launched us in dev mode.
    for k in [
        "TERM_PROGRAM_VERSION",
        "TERM_SESSION_ID",
        "ITERM_SESSION_ID",
        "ITERM_PROFILE",
    ] {
        env.remove(k);
    }
    for (k, v) in extra {
        env.insert(k.clone(), v.clone());
    }
    env
}

/// Everything needed to start a tab's shell.
pub struct ShellSpec {
    pub shell: String, // "" = the login shell
    pub args: Option<Vec<String>>,
    pub dir: Option<PathBuf>,
    pub integration: bool,
    pub extra_env: Vec<(String, String)>,
}

/// One line to type into a bash or zsh on another machine (over ssh) that
/// turns on browse mode there for that session only: nothing is written to
/// the server. It starts with a space so most shells keep it out of the
/// history. (3.x's, unchanged.)
pub fn remote_setup() -> String {
    use base64::Engine;
    let script = SCRIPTS
        .iter()
        .find(|(name, _)| *name == "remote.sh")
        .map(|(_, body)| *body)
        .unwrap_or("");
    // Comments don't need to travel.
    let keep: Vec<&str> = script
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect();
    let b = base64::engine::general_purpose::STANDARD.encode(keep.join("\n") + "\n");
    // GNU base64 takes -d, older macOS -D.
    format!(" eval \"$(printf '%s' '{b}' | {{ base64 -d 2>/dev/null || base64 -D; }})\"")
}

/// A path for typing at a prompt: quoted only when it needs it (as Terminal
/// does when you drop a file).
pub fn quote(path: &str) -> String {
    if !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+~@%:,=".contains(c))
    {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

/// The command to run in the PTY.
pub fn command(spec: &ShellSpec) -> CommandBuilder {
    let shell = if spec.shell.is_empty() {
        default_shell()
    } else {
        spec.shell.clone()
    };
    // The environment we start from: inherited, plus the caller's additions
    // (the hooks then go on top, so a HOME given here is the user's home).
    let mut base: BTreeMap<String, String> = std::env::vars().collect();
    for (k, v) in &spec.extra_env {
        base.insert(k.clone(), v.clone());
    }
    let home = base
        .get("HOME")
        .cloned()
        .or_else(|| dirs::home_dir().map(|h| h.display().to_string()))
        .unwrap_or_default();
    let mut launch = Launch {
        program: shell.clone(),
        args: spec.args.clone().unwrap_or_else(|| vec!["-l".into()]),
        env: vec![],
    };
    if spec.integration {
        let dir = integration_dir();
        if install_integration(&dir).is_ok() {
            launch = with_integration(
                &shell,
                spec.args.clone(),
                &dir,
                &home,
                base.get("ZDOTDIR").cloned(),
            );
        }
    }
    let env = build_env(base, &shell, &launch.env);
    let mut cmd = CommandBuilder::new(&launch.program);
    cmd.args(&launch.args);
    cmd.env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }
    let dir = spec
        .dir
        .clone()
        .filter(|d| d.is_dir())
        .or_else(|| Some(PathBuf::from(&home)));
    if let Some(d) = dir {
        cmd.cwd(d);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_line_and_quoting() {
        let line = remote_setup();
        assert!(
            line.starts_with(" eval "),
            "a space: kept out of the history"
        );
        let b64 = line.split('\'').nth(3).unwrap();
        use base64::Engine;
        let script = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap(),
        )
        .unwrap();
        assert!(script.contains("133"), "the marks");
        assert!(
            !script.lines().any(|l| l.trim().starts_with('#')),
            "no comments"
        );
        assert_eq!(quote("/Users/me/notes.txt"), "/Users/me/notes.txt");
        assert_eq!(quote("/tmp/My File.txt"), "'/tmp/My File.txt'");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn hooks_per_shell() {
        let dir = Path::new("/c/si");
        let z = with_integration("/bin/zsh", None, dir, "/home/u", Some("/zd".into()));
        assert_eq!(z.args, vec!["-l"]);
        assert!(z.env.contains(&("ZDOTDIR".into(), "/c/si/zsh".into())));
        assert!(z.env.contains(&("JAMALTERM_ZDOTDIR".into(), "/zd".into())));

        let b = with_integration("/bin/bash", None, dir, "/home/u", None);
        assert_eq!(b.args, vec!["--rcfile", "/c/si/bash/rc.bash", "-i"]);
        assert!(b.env.contains(&("JAMALTERM_BASH_LOGIN".into(), "1".into())));
        let b2 = with_integration(
            "/bin/bash",
            Some(vec!["-c".into(), "x".into()]),
            dir,
            "/home/u",
            None,
        );
        assert_eq!(b2.args, vec!["-c", "x"], "custom arguments are left alone");

        let f = with_integration("/opt/homebrew/bin/fish", None, dir, "/home/u", None);
        assert_eq!(
            f.args,
            vec!["-l", "-C", "source '/c/si/fish/integration.fish'"]
        );

        let t = with_integration("/bin/tcsh", None, dir, "/home/u", None);
        assert!(t.env.contains(&("HOME".into(), "/c/si/tcsh".into())));
        assert!(t.env.contains(&("JAMALTERM_HOME".into(), "/home/u".into())));

        let n = with_integration("/usr/bin/nu", None, dir, "/home/u", None);
        assert_eq!(n.args, vec!["-l"]);
    }

    #[test]
    fn environment() {
        let base = vec![
            ("PATH".to_string(), "/bin".to_string()),
            ("TERM_SESSION_ID".to_string(), "x".to_string()),
        ];
        let env = build_env(base, "/bin/zsh", &[("FOO".into(), "1".into())]);
        assert_eq!(env["TERM"], "xterm-256color");
        assert_eq!(env["TERM_PROGRAM"], "jamalterm");
        assert_eq!(
            env["LANG"], "en_US.UTF-8",
            "a locale for apps started from Finder"
        );
        assert_eq!(env["FOO"], "1");
        assert!(!env.contains_key("TERM_SESSION_ID"));
        let env = build_env(
            vec![("LC_ALL".to_string(), "C".to_string())],
            "/bin/zsh",
            &[],
        );
        assert!(!env.contains_key("LANG"), "an existing locale is kept");
    }

    #[test]
    fn scripts_install_once() {
        let dir = tempfile::tempdir().unwrap();
        install_integration(dir.path()).unwrap();
        assert!(
            dir.path().join("zsh/.zshenv").exists() && dir.path().join("bash/rc.bash").exists()
        );
        install_integration(dir.path()).unwrap(); // again: no error
    }
}
