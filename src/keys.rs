//! API keys: one encrypted file next to the settings, never the Keychain
//! (nothing here can ever show a password prompt).
//!
//! The same files as 3.x, so keys saved there keep working:
//! - `keys.enc` is AES-256-GCM, JSON `{"version":1,"nonce","data"}` (base64),
//!   associated data "jamalterm keys v1", holding a JSON map of provider →
//!   key;
//! - its key is HKDF-SHA256 of a random 32-byte `keys.secret`, with info
//!   "jamalterm api keys|<this computer's hardware id>|<user id>", so a copy
//!   of the folder can't be read on another computer or by another account;
//! - both files are 0600 in a 0700 folder and left out of Time Machine.
//!
//! Like SSH keys or the GitHub and AWS command-line tools, this keeps keys
//! from other accounts, copies and backups; it can't stop a program already
//! running as you. (3.x could move keys an older version left in the
//! Keychain; 4.x doesn't touch the Keychain at all.)

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const AAD: &[u8] = b"jamalterm keys v1";

/// The providers with keys, and the environment variable each falls back on.
pub const ENV: &[(&str, &str)] = &[
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OPENAI_API_KEY"),
    ("deepseek", "DEEPSEEK_API_KEY"),
    ("xai", "XAI_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
];

pub struct Keys {
    dir: PathBuf,
    machine: String,
    uid: u32,
    keys: BTreeMap<String, String>,
    /// A keys file is there but can't be opened here (copied from another
    /// computer, or damaged). Saving a key starts a new one.
    pub unreadable: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Sealed {
    version: u32,
    nonce: String,
    data: String,
}

impl Keys {
    /// The keys in the settings folder (the folder of `settings_path`).
    pub fn open(settings_path: &Path) -> Keys {
        let dir = settings_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        Keys::open_as(dir, machine_id(), unsafe { libc::getuid() })
    }

    fn open_as(dir: PathBuf, machine: String, uid: u32) -> Keys {
        let mut k = Keys {
            dir,
            machine,
            uid,
            keys: BTreeMap::new(),
            unreadable: false,
        };
        match std::fs::read(k.dir.join("keys.enc")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => k.unreadable = true,
            Ok(data) => match k.decrypt(&data) {
                Some(map) => k.keys = map,
                None => k.unreadable = true,
            },
        }
        k
    }

    /// A provider's saved key, else its environment variable.
    pub fn get(&self, provider: &str) -> Option<String> {
        if let Some(k) = self.keys.get(provider).filter(|k| !k.is_empty()) {
            return Some(k.clone());
        }
        let var = ENV.iter().find(|(p, _)| *p == provider)?.1;
        std::env::var(var)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    /// Whether a key was saved here (not just in the environment).
    pub fn saved(&self, provider: &str) -> bool {
        self.keys.get(provider).is_some_and(|k| !k.is_empty())
    }

    /// Saves a provider's key; an empty key removes it.
    pub fn set(&mut self, provider: &str, key: &str) -> Result<(), String> {
        if !ENV.iter().any(|(p, _)| *p == provider) {
            return Err(format!("unknown AI provider {provider}"));
        }
        let key = key.trim();
        if key.contains(|c: char| c.is_whitespace() || "\"'\\".contains(c)) {
            return Err("that doesn't look like an API key (it has spaces or quotes)".into());
        }
        if key.is_empty() {
            self.keys.remove(provider);
        } else {
            self.keys.insert(provider.into(), key.into());
        }
        self.store()
            .map_err(|e| format!("the key wasn't saved: {e}"))?;
        self.unreadable = false;
        Ok(())
    }

    fn cipher(&self, create: bool) -> std::io::Result<Aes256Gcm> {
        let path = self.dir.join("keys.secret");
        let secret = match std::fs::read(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                let mut s = vec![0u8; 32];
                getrandom::fill(&mut s).map_err(|e| std::io::Error::other(e.to_string()))?;
                write_private(&path, &s)?;
                exclude_from_backups(&path);
                s
            }
            Err(e) => return Err(e),
        };
        if secret.len() != 32 {
            return Err(std::io::Error::other("damaged keys secret"));
        }
        let info = format!("jamalterm api keys|{}|{}", self.machine, self.uid);
        let mut key = [0u8; 32];
        hkdf::Hkdf::<sha2::Sha256>::new(None, &secret)
            .expand(info.as_bytes(), &mut key)
            .map_err(|_| std::io::Error::other("key derivation failed"))?;
        Ok(Aes256Gcm::new(&key.into()))
    }

    fn decrypt(&self, data: &[u8]) -> Option<BTreeMap<String, String>> {
        let f: Sealed = serde_json::from_slice(data).ok()?;
        if f.version != 1 {
            return None;
        }
        let nonce = B64.decode(f.nonce).ok()?;
        let boxed = B64.decode(f.data).ok()?;
        if nonce.len() != 12 {
            return None;
        }
        let plain = self
            .cipher(false)
            .ok()?
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &boxed,
                    aad: AAD,
                },
            )
            .ok()?;
        serde_json::from_slice(&plain).ok()
    }

    fn store(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        set_mode(&self.dir, 0o700);
        let path = self.dir.join("keys.enc");
        if self.keys.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let cipher = self.cipher(true)?;
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|e| std::io::Error::other(e.to_string()))?;
        let plain = serde_json::to_vec(&self.keys)?;
        let boxed = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plain,
                    aad: AAD,
                },
            )
            .map_err(|_| std::io::Error::other("encryption failed"))?;
        let out = serde_json::to_vec(&Sealed {
            version: 1,
            nonce: B64.encode(nonce),
            data: B64.encode(boxed),
        })?;
        let tmp = path.with_extension("enc.tmp");
        write_private(&tmp, &out)?;
        std::fs::rename(&tmp, &path)?;
        exclude_from_backups(&path);
        Ok(())
    }
}

fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, data)?;
    set_mode(path, 0o600);
    Ok(())
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

/// Keeps a file out of Time Machine (macOS).
fn exclude_from_backups(path: &Path) {
    if cfg!(target_os = "macos") && !cfg!(test) {
        let _ = std::process::Command::new("/usr/bin/tmutil")
            .arg("addexclusion")
            .arg(path)
            .output();
    }
}

/// This computer's hardware id: the Mac's IOPlatformUUID, else systemd's
/// machine-id (as 3.x).
pub fn machine_id() -> String {
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("/usr/sbin/ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        return out
            .lines()
            .find(|l| l.contains("\"IOPlatformUUID\""))
            .and_then(|l| l.split('"').nth(3))
            .unwrap_or("")
            .to_string();
    }
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made by 3.5.0's own Go code (internal/settings, from git history) with
    /// this secret, machine id "TEST-MACHINE-UUID" and user id 0.
    const SECRET_HEX: &str = "42cb39a58011012be9838c210284fc29c64a5c69b6a0addcf2e74ab658685623";
    const KEYS_ENC: &str = r#"{"version":1,"nonce":"7pDof7UP94hvmDqh","data":"EajdeO/bgU7oAPzloNyQvilnIExoAljagkXl7carpoQG+NIOQFbx6CBMyZt6ejyG1o5EZJ3SPrgx60pDdehHEeWfj0+rlQYlUMULmU+paJaS5kf8jI1Dye4I"}"#;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let secret: Vec<u8> = (0..32)
            .map(|i| u8::from_str_radix(&SECRET_HEX[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        std::fs::write(dir.path().join("keys.secret"), secret).unwrap();
        std::fs::write(dir.path().join("keys.enc"), KEYS_ENC).unwrap();
        dir
    }

    #[test]
    fn reads_keys_saved_by_3x() {
        let dir = fixture();
        let k = Keys::open_as(dir.path().into(), "TEST-MACHINE-UUID".into(), 0);
        assert!(!k.unreadable);
        assert_eq!(
            k.get("anthropic").as_deref(),
            Some("sk-ant-api03-fixture-key")
        );
        assert_eq!(k.get("openai").as_deref(), Some("sk-proj-fixture-openai"));
        assert!(k.saved("anthropic") && !k.saved("xai"));
    }

    #[test]
    fn another_computer_or_account_cant_read_them() {
        let dir = fixture();
        assert!(Keys::open_as(dir.path().into(), "OTHER-MAC".into(), 0).unreadable);
        let k = Keys::open_as(dir.path().into(), "TEST-MACHINE-UUID".into(), 501);
        assert!(k.unreadable);
        assert!(!k.saved("anthropic"));
    }

    #[test]
    fn save_change_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let p: PathBuf = dir.path().join("jt");
        let mut k = Keys::open_as(p.clone(), "M".into(), 7);
        k.set("xai", "xai-abc123").unwrap();
        k.set("anthropic", "sk-ant-1").unwrap();
        assert!(k.set("anthropic", "has space").is_err());
        assert!(k.set("gemini", "x").is_err(), "unknown provider");
        let again = Keys::open_as(p.clone(), "M".into(), 7);
        assert_eq!(again.get("xai").as_deref(), Some("xai-abc123"));
        use std::os::unix::fs::PermissionsExt;
        let mode = |f: &str| std::fs::metadata(p.join(f)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode("keys.enc"), 0o600);
        assert_eq!(mode("keys.secret"), 0o600);
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let text = std::fs::read_to_string(p.join("keys.enc")).unwrap();
        assert!(!text.contains("xai-abc123"), "encrypted");
        let mut k = again;
        k.set("xai", "").unwrap();
        k.set("anthropic", "").unwrap();
        assert!(!p.join("keys.enc").exists(), "no keys: no file");
    }

    #[test]
    fn a_saved_key_wins_over_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let mut k = Keys::open_as(dir.path().into(), "M".into(), 1);
        // DEEPSEEK_API_KEY isn't set in tests; a saved one is used.
        k.set("deepseek", "sk-saved").unwrap();
        assert_eq!(k.get("deepseek").as_deref(), Some("sk-saved"));
    }
}
