//! Checking that a release was built by GitHub from this repository.
//!
//! Each release carries a Sigstore bundle (jamalterm-X-macos.zip.sigstore.json)
//! made by GitHub's attest-build-provenance action. It proves, through
//! Sigstore's public certificate authority (Fulcio), its transparency log
//! (Rekor) and certificate transparency, that the zip with exactly these
//! bytes was built by this repository's build.yml workflow on GitHub. No
//! signing key is kept anywhere that could leak.
//!
//! 3.x used sigstore-go. There's no pure-Rust Sigstore library that builds
//! without C code (CI checks the Mac build from Linux), so the checks are
//! done here with RustCrypto's crates, one by one:
//!
//! 1. the signing certificate chains to a Fulcio CA in Sigstore's trusted
//!    root and was valid when the build was logged;
//! 2. it was issued for GitHub Actions (the issuer extension) and for this
//!    repository's build.yml on main or a version tag (the SAN);
//! 3. it is in a certificate transparency log (its SCT verifies);
//! 4. the signature over the build statement is the certificate's;
//! 5. the statement names a file with exactly the download's SHA-256;
//! 6. the signature is in Rekor: the log's signed promise (SET) and its
//!    inclusion proof up to a signed checkpoint both verify, and the logged
//!    entry is this signature over this statement.
//!
//! Sigstore's public keys come from `resources/sigstore/trusted_root.json`,
//! built into the app (3.x fetched it through TUF at run time). When Sigstore
//! rotates its keys, that file must be updated from
//! github.com/sigstore/root-signing (`targets/trusted_root.json`) and a new
//! version released, or new releases will stop verifying.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use p256::pkcs8::DecodePublicKey as _;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha384};
use x509_cert::Certificate;
use x509_cert::der::asn1::{OctetString, Utf8StringRef};
use x509_cert::der::oid::ObjectIdentifier;
use x509_cert::der::{Decode, Encode};
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{ExtendedKeyUsage, SubjectAltName};

/// Sigstore's public keys (CAs, transparency logs, CT logs). Rotating
/// Sigstore keys means updating this file; see the module comment.
const TRUSTED_ROOT: &str = include_str!("../../resources/sigstore/trusted_root.json");

/// GitHub Actions' OIDC issuer: the build ran on GitHub's runners.
pub const BUILDER_ISSUER: &str = "https://token.actions.githubusercontent.com";
/// The workflow that must have built a release.
pub const BUILDER_SAN: &str = r"^https://github\.com/jamalxcode/jamalterm/\.github/workflows/build\.yml@refs/(heads/main|tags/v[0-9.]+(-[A-Za-z0-9.]+)?)$";

const OID_ISSUER_V2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.8");
const OID_ISSUER_V1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.1");
const OID_SCT_LIST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.11129.2.4.2");
const OID_SAN: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.17");
const OID_EKU: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.37");
const OID_CODE_SIGNING: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.3");
const OID_ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const OID_ECDSA_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");

/// Who must have signed: the issuer and a pattern for the workflow.
pub struct Policy {
    pub issuer: String,
    pub san: Regex,
}

impl Policy {
    /// jamalterm's own build workflow on GitHub.
    pub fn jamalterm() -> Policy {
        Policy {
            issuer: BUILDER_ISSUER.to_string(),
            san: Regex::new(BUILDER_SAN).expect("valid pattern"),
        }
    }
}

/// Checks that `bundle_json` proves this repository's GitHub workflow built
/// the file whose SHA-256 is `sha256_hex`. Errors are plain sentences.
pub fn verify(bundle_json: &[u8], sha256_hex: &str) -> Result<(), String> {
    let root = TrustedRoot::embedded()?;
    verify_with(bundle_json, sha256_hex, &Policy::jamalterm(), &root)
}

/// [`verify`] with a given policy and set of Sigstore keys (for tests).
pub fn verify_with(
    bundle_json: &[u8],
    sha256_hex: &str,
    policy: &Policy,
    root: &TrustedRoot,
) -> Result<(), String> {
    let digest = from_hex(sha256_hex)
        .filter(|d| d.len() == 32)
        .ok_or("bad checksum")?;
    let bundle: Bundle = serde_json::from_slice(bundle_json)
        .map_err(|e| format!("the build proof is damaged: {e}"))?;
    check(&bundle, &digest, policy, root).map_err(|e| {
        format!("the download isn't proven to be built by jamalterm's GitHub workflow: {e}")
    })
}

// ---- Sigstore's trusted root ----

/// Sigstore's public keys, from a trusted_root.json.
pub struct TrustedRoot {
    pub tlogs: Vec<Log>,
    pub ctlogs: Vec<Log>,
    pub cas: Vec<Authority>,
}

/// A transparency log (Rekor) or certificate transparency log.
pub struct Log {
    pub log_id: Vec<u8>,
    pub key: Key,
    pub valid: Window,
}

/// A Fulcio certificate authority: its chain (DER, issuing certificate
/// first, root last) and when it may issue.
pub struct Authority {
    pub chain: Vec<Vec<u8>>,
    pub valid: Window,
}

/// A time range in Unix seconds; `end` is open when None.
#[derive(Clone, Copy)]
pub struct Window {
    pub start: i64,
    pub end: Option<i64>,
}

impl Window {
    fn contains(&self, t: i64) -> bool {
        t >= self.start && self.end.is_none_or(|e| t <= e)
    }
}

/// A public key we can check signatures with.
pub enum Key {
    P256(p256::ecdsa::VerifyingKey),
    P384(p384::ecdsa::VerifyingKey),
    /// Another kind (Ed25519 for Rekor v2's logs): named, not usable here.
    Other(String),
}

impl Key {
    fn from_spki(der: &[u8]) -> Key {
        if let Ok(k) = p256::ecdsa::VerifyingKey::from_public_key_der(der) {
            return Key::P256(k);
        }
        if let Ok(k) = p384::ecdsa::VerifyingKey::from_public_key_der(der) {
            return Key::P384(k);
        }
        Key::Other("an unknown key type".into())
    }

    /// Checks a DER ECDSA signature over a digest (any of SHA-256/384).
    fn verify_prehash(&self, digest: &[u8], sig_der: &[u8]) -> bool {
        use p256::ecdsa::signature::hazmat::PrehashVerifier;
        match self {
            Key::P256(k) => p256::ecdsa::Signature::from_der(sig_der)
                .is_ok_and(|s| k.verify_prehash(digest, &s).is_ok()),
            Key::P384(k) => p384::ecdsa::Signature::from_der(sig_der)
                .is_ok_and(|s| k.verify_prehash(digest, &s).is_ok()),
            Key::Other(_) => false,
        }
    }

    /// Checks a DER ECDSA signature over `msg` with the key's usual hash
    /// (SHA-256 for P-256, SHA-384 for P-384).
    fn verify(&self, msg: &[u8], sig_der: &[u8]) -> bool {
        match self {
            Key::P384(_) => self.verify_prehash(&Sha384::digest(msg), sig_der),
            _ => self.verify_prehash(&Sha256::digest(msg), sig_der),
        }
    }
}

impl TrustedRoot {
    /// The copy built into the app.
    pub fn embedded() -> Result<TrustedRoot, String> {
        TrustedRoot::parse(TRUSTED_ROOT.as_bytes())
            .map_err(|e| format!("couldn't read Sigstore's public keys to check the download: {e}"))
    }

    pub fn parse(json: &[u8]) -> Result<TrustedRoot, String> {
        let v: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let logs = |name: &str| -> Result<Vec<Log>, String> {
            let mut out = Vec::new();
            for l in v[name].as_array().into_iter().flatten() {
                let pk = &l["publicKey"];
                let raw = b64(pk["rawBytes"].as_str().unwrap_or_default())?;
                let key = match pk["keyDetails"].as_str().unwrap_or_default() {
                    "PKIX_ECDSA_P256_SHA_256" | "PKIX_ECDSA_P384_SHA_384" => Key::from_spki(&raw),
                    other => Key::Other(other.to_string()),
                };
                out.push(Log {
                    log_id: b64(l["logId"]["keyId"].as_str().unwrap_or_default())?,
                    key,
                    valid: window(&pk["validFor"])?,
                });
            }
            Ok(out)
        };
        let mut cas = Vec::new();
        for ca in v["certificateAuthorities"].as_array().into_iter().flatten() {
            let chain = ca["certChain"]["certificates"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| b64(c["rawBytes"].as_str().unwrap_or_default()))
                .collect::<Result<Vec<_>, _>>()?;
            if !chain.is_empty() {
                cas.push(Authority {
                    chain,
                    valid: window(&ca["validFor"])?,
                });
            }
        }
        Ok(TrustedRoot {
            tlogs: logs("tlogs")?,
            ctlogs: logs("ctlogs")?,
            cas,
        })
    }
}

fn window(v: &Value) -> Result<Window, String> {
    let start = match v["start"].as_str() {
        Some(s) => rfc3339(s).ok_or_else(|| format!("bad time {s}"))?,
        None => 0,
    };
    let end = match v["end"].as_str() {
        Some(s) => Some(rfc3339(s).ok_or_else(|| format!("bad time {s}"))?),
        None => None,
    };
    Ok(Window { start, end })
}

/// "2022-12-31T23:59:59.999Z" in Unix seconds (fractions dropped).
fn rfc3339(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.split('.').next()?;
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (h, min, sec) = (t.next()??, t.next()??, t.next()??);
    Some(days_from_civil(y, m, day) * 86400 + h * 3600 + min * 60 + sec)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// ---- The bundle (protobuf JSON) ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bundle {
    media_type: String,
    verification_material: Material,
    dsse_envelope: Option<Envelope>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Material {
    certificate: Option<RawBytes>,
    x509_certificate_chain: Option<CertChain>,
    #[serde(default)]
    tlog_entries: Vec<TlogEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBytes {
    raw_bytes: String,
}

#[derive(Deserialize)]
struct CertChain {
    certificates: Vec<RawBytes>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TlogEntry {
    #[serde(deserialize_with = "int")]
    log_index: i64,
    log_id: KeyId,
    kind_version: KindVersion,
    #[serde(deserialize_with = "int")]
    integrated_time: i64,
    inclusion_promise: Option<Promise>,
    inclusion_proof: Option<Proof>,
    canonicalized_body: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeyId {
    key_id: String,
}

#[derive(Deserialize, Clone)]
struct KindVersion {
    kind: String,
    version: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Promise {
    signed_entry_timestamp: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Proof {
    #[serde(deserialize_with = "int")]
    log_index: i64,
    root_hash: String,
    #[serde(deserialize_with = "int")]
    tree_size: i64,
    #[serde(default)]
    hashes: Vec<String>,
    checkpoint: Option<Checkpoint>,
}

#[derive(Deserialize, Clone)]
struct Checkpoint {
    envelope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    payload: String,
    payload_type: String,
    signatures: Vec<EnvelopeSig>,
}

#[derive(Deserialize)]
struct EnvelopeSig {
    sig: String,
}

/// Protobuf JSON writes 64-bit numbers as strings; accept both.
fn int<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match Value::deserialize(d)? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
    .ok_or_else(|| serde::de::Error::custom("expected a number"))
}

// ---- The checks ----

fn check(b: &Bundle, digest: &[u8], policy: &Policy, root: &TrustedRoot) -> Result<(), String> {
    if !b
        .media_type
        .starts_with("application/vnd.dev.sigstore.bundle")
    {
        return Err(format!("it isn't a Sigstore bundle ({})", b.media_type));
    }
    let leaf_der = match (
        &b.verification_material.certificate,
        &b.verification_material.x509_certificate_chain,
    ) {
        (Some(c), _) => b64(&c.raw_bytes)?,
        (None, Some(ch)) if !ch.certificates.is_empty() => b64(&ch.certificates[0].raw_bytes)?,
        _ => return Err("it has no signing certificate".into()),
    };
    let env = b
        .dsse_envelope
        .as_ref()
        .ok_or("it isn't a signed build statement")?;
    let leaf = Certificate::from_der(&leaf_der)
        .map_err(|e| format!("its certificate is damaged ({e})"))?;
    let leaf_key = Key::from_spki(&spki_der(&leaf)?);
    let entries = &b.verification_material.tlog_entries;
    if entries.is_empty() {
        return Err("it isn't in Sigstore's transparency log".into());
    }

    // The certificate was valid when the signature was logged (Fulcio's
    // certificates live for 10 minutes; the log's time is what proves the
    // signature was made inside them).
    for e in entries {
        if !cert_valid_at(&leaf, e.integrated_time) {
            return Err("its certificate wasn't valid when the build was logged".into());
        }
    }
    let t = entries[0].integrated_time;

    // 1. Chains to a Fulcio CA.
    let issuer_der = chain_to_fulcio(&leaf, root, t)?;

    // 2. Issued for this repository's GitHub workflow.
    check_identity(&leaf, policy)?;

    // 3. In a certificate transparency log.
    check_sct(&leaf_der, &leaf, &issuer_der, root)?;

    // 4. The statement is signed by the certificate's key.
    if env.signatures.len() != 1 {
        return Err("the build statement should have exactly one signature".into());
    }
    let payload = b64(&env.payload).map_err(|_| "the build statement is damaged")?;
    let sig = b64(&env.signatures[0].sig).map_err(|_| "the signature is damaged")?;
    if !leaf_key.verify(&pae(&env.payload_type, &payload), &sig) {
        return Err("the signature doesn't match the build statement".into());
    }

    // 5. The statement is about this download.
    check_statement(&env.payload_type, &payload, digest)?;

    // 6. Logged in Rekor.
    for e in entries {
        check_tlog_entry(e, root, &payload, &sig, &leaf_der)?;
    }
    Ok(())
}

fn spki_der(c: &Certificate) -> Result<Vec<u8>, String> {
    c.tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|e| e.to_string())
}

fn cert_valid_at(c: &Certificate, t: i64) -> bool {
    let v = c.tbs_certificate().validity();
    let nb = v.not_before.to_unix_duration().as_secs() as i64;
    let na = v.not_after.to_unix_duration().as_secs() as i64;
    t >= nb && t <= na
}

/// Checks `cert`'s signature with `issuer`'s key.
fn signed_by(cert_der: &[u8], cert: &Certificate, issuer: &Certificate) -> bool {
    let Ok(key_der) = spki_der(issuer) else {
        return false;
    };
    let key = Key::from_spki(&key_der);
    let Some(tbs) = first_child(cert_der) else {
        return false;
    };
    let Some(sig) = cert.signature().as_bytes() else {
        return false;
    };
    let alg = cert.signature_algorithm().oid;
    let digest: Vec<u8> = if alg == OID_ECDSA_SHA256 {
        Sha256::digest(tbs).to_vec()
    } else if alg == OID_ECDSA_SHA384 {
        Sha384::digest(tbs).to_vec()
    } else {
        return false;
    };
    key.verify_prehash(&digest, sig)
}

/// Finds the Fulcio CA that issued `leaf` and checks its chain up to the
/// root. Returns the issuing certificate (for the SCT check).
fn chain_to_fulcio(leaf: &Certificate, root: &TrustedRoot, t: i64) -> Result<Vec<u8>, String> {
    let leaf_der = leaf.to_der().map_err(|e| e.to_string())?;
    for ca in &root.cas {
        if !ca.valid.contains(t) {
            continue;
        }
        let Ok(certs) = ca
            .chain
            .iter()
            .map(|d| Certificate::from_der(d))
            .collect::<Result<Vec<_>, _>>()
        else {
            continue;
        };
        if !signed_by(&leaf_der, leaf, &certs[0]) {
            continue;
        }
        // Each certificate is signed by the next; the last is the root,
        // trusted because it's in the trusted root. All valid at t.
        let chain_ok = certs.iter().all(|c| cert_valid_at(c, t))
            && (0..certs.len() - 1).all(|i| signed_by(&ca.chain[i], &certs[i], &certs[i + 1]));
        if chain_ok {
            return Ok(ca.chain[0].clone());
        }
    }
    Err("its certificate wasn't issued by Sigstore's certificate authority".into())
}

fn check_identity(leaf: &Certificate, policy: &Policy) -> Result<(), String> {
    let exts = leaf
        .tbs_certificate()
        .extensions()
        .map(|e| e.as_slice())
        .unwrap_or_default();
    let ext = |oid: ObjectIdentifier| exts.iter().find(|e| e.extn_id == oid);

    let code_signing = ext(OID_EKU)
        .and_then(|e| ExtendedKeyUsage::from_der(e.extn_value.as_bytes()).ok())
        .is_some_and(|eku| eku.0.contains(&OID_CODE_SIGNING));
    if !code_signing {
        return Err("its certificate isn't for code signing".into());
    }

    // The issuer: a DER UTF8String in .1.8, raw text in the older .1.1.
    let issuer = if let Some(e) = ext(OID_ISSUER_V2) {
        Utf8StringRef::from_der(e.extn_value.as_bytes())
            .map(|s| s.as_str().to_string())
            .map_err(|_| "its certificate's issuer is damaged")?
    } else if let Some(e) = ext(OID_ISSUER_V1) {
        String::from_utf8_lossy(e.extn_value.as_bytes()).into_owned()
    } else {
        return Err("its certificate doesn't say who issued the identity".into());
    };
    if issuer != policy.issuer {
        return Err(format!(
            "it was signed with an identity from {issuer}, not {}",
            policy.issuer
        ));
    }

    let sans: Vec<String> = ext(OID_SAN)
        .and_then(|e| SubjectAltName::from_der(e.extn_value.as_bytes()).ok())
        .map(|san| {
            san.0
                .iter()
                .filter_map(|n| match n {
                    GeneralName::UniformResourceIdentifier(u) => Some(u.to_string()),
                    GeneralName::Rfc822Name(m) => Some(m.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if !sans.iter().any(|s| policy.san.is_match(s)) {
        return Err(format!(
            "it was built by {}, not jamalterm's build workflow",
            sans.first()
                .map(String::as_str)
                .unwrap_or("an unnamed workflow")
        ));
    }
    Ok(())
}

/// Checks that at least one SCT in the certificate verifies with a CT log
/// in the trusted root (RFC 6962 §3.2, precertificate entry).
fn check_sct(
    leaf_der: &[u8],
    leaf: &Certificate,
    issuer_der: &[u8],
    root: &TrustedRoot,
) -> Result<(), String> {
    let none = || "its certificate isn't in a certificate transparency log".to_string();
    let ext = leaf
        .tbs_certificate()
        .extensions()
        .and_then(|e| e.iter().find(|e| e.extn_id == OID_SCT_LIST))
        .ok_or_else(none)?;
    let list = OctetString::from_der(ext.extn_value.as_bytes()).map_err(|_| none())?;
    let scts = parse_sct_list(list.as_bytes()).ok_or_else(none)?;

    let issuer = Certificate::from_der(issuer_der).map_err(|e| e.to_string())?;
    let issuer_key_hash = Sha256::digest(spki_der(&issuer)?);
    let tbs = first_child(leaf_der).ok_or_else(none)?;
    let precert_tbs = without_extension(tbs, &OID_SCT_LIST.to_der().map_err(|e| e.to_string())?)
        .ok_or_else(none)?;

    for s in scts {
        let Some(log) = root.ctlogs.iter().find(|l| l.log_id == s.log_id) else {
            continue;
        };
        if !log.valid.contains((s.timestamp / 1000) as i64) || s.hash_alg != 4 || s.sig_alg != 3 {
            continue;
        }
        let mut msg = vec![0u8, 0u8]; // v1, certificate_timestamp
        msg.extend_from_slice(&s.timestamp.to_be_bytes());
        msg.extend_from_slice(&[0, 1]); // precert_entry
        msg.extend_from_slice(&issuer_key_hash);
        let n = precert_tbs.len();
        msg.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
        msg.extend_from_slice(&precert_tbs);
        msg.extend_from_slice(&(s.extensions.len() as u16).to_be_bytes());
        msg.extend_from_slice(&s.extensions);
        if log.key.verify_prehash(&Sha256::digest(&msg), &s.signature) {
            return Ok(());
        }
    }
    Err("its certificate transparency proof (SCT) doesn't verify".into())
}

struct Sct {
    log_id: Vec<u8>,
    timestamp: u64,
    extensions: Vec<u8>,
    hash_alg: u8,
    sig_alg: u8,
    signature: Vec<u8>,
}

/// The TLS-encoded SignedCertificateTimestampList.
fn parse_sct_list(b: &[u8]) -> Option<Vec<Sct>> {
    let mut r = Reader(b);
    let mut list = Reader(r.take_u16_prefixed()?);
    let mut out = Vec::new();
    while !list.0.is_empty() {
        let mut s = Reader(list.take_u16_prefixed()?);
        if s.take(1)?[0] != 0 {
            continue; // not v1
        }
        let log_id = s.take(32)?.to_vec();
        let timestamp = u64::from_be_bytes(s.take(8)?.try_into().ok()?);
        let extensions = s.take_u16_prefixed()?.to_vec();
        let hash_alg = s.take(1)?[0];
        let sig_alg = s.take(1)?[0];
        let signature = s.take_u16_prefixed()?.to_vec();
        out.push(Sct {
            log_id,
            timestamp,
            extensions,
            hash_alg,
            sig_alg,
            signature,
        });
    }
    Some(out)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn take_u16_prefixed(&mut self) -> Option<&'a [u8]> {
        let n = u16::from_be_bytes(self.take(2)?.try_into().ok()?) as usize;
        self.take(n)
    }
}

/// DSSE's pre-authentication encoding: what is actually signed.
fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "DSSEv1 {} {} {} ",
        payload_type.len(),
        payload_type,
        payload.len()
    )
    .into_bytes();
    out.extend_from_slice(payload);
    out
}

fn check_statement(payload_type: &str, payload: &[u8], digest: &[u8]) -> Result<(), String> {
    if payload_type != "application/vnd.in-toto+json" {
        return Err(format!("it signs a {payload_type}, not a build statement"));
    }
    let st: Value =
        serde_json::from_slice(payload).map_err(|_| "the build statement is damaged")?;
    match st["_type"].as_str() {
        Some("https://in-toto.io/Statement/v1" | "https://in-toto.io/Statement/v0.1") => {}
        _ => return Err("the build statement is of an unknown kind".into()),
    }
    let want = to_hex(digest);
    let found = st["subject"].as_array().into_iter().flatten().any(|s| {
        s["digest"]["sha256"]
            .as_str()
            .is_some_and(|d| d.eq_ignore_ascii_case(&want))
    });
    if !found {
        return Err("it's for a different file (the checksum doesn't match)".into());
    }
    Ok(())
}

/// Checks one Rekor entry: it is genuine (signed promise and inclusion proof
/// up to a signed checkpoint) and it records this signature over this
/// statement, made with this certificate.
fn check_tlog_entry(
    e: &TlogEntry,
    root: &TrustedRoot,
    payload: &[u8],
    sig: &[u8],
    leaf_der: &[u8],
) -> Result<(), String> {
    let log_id = b64(&e.log_id.key_id)?;
    let log = root
        .tlogs
        .iter()
        .find(|l| l.log_id == log_id)
        .ok_or("it was logged in a transparency log Sigstore doesn't list")?;
    if let Key::Other(kind) = &log.key {
        return Err(format!(
            "it was logged in a transparency log this version can't check yet ({kind}); download it from the release page instead"
        ));
    }
    if !log.valid.contains(e.integrated_time) {
        return Err("the transparency log's key wasn't valid when the build was logged".into());
    }

    // The signed promise (SET): the log's signature over the entry and the
    // time it was added. This is the trusted time the certificate was
    // checked against.
    let set = e
        .inclusion_promise
        .as_ref()
        .ok_or("its transparency log entry has no signed timestamp")?;
    let canonical = format!(
        r#"{{"body":"{}","integratedTime":{},"logID":"{}","logIndex":{}}}"#,
        e.canonicalized_body,
        e.integrated_time,
        to_hex(&log_id),
        e.log_index
    );
    if e.canonicalized_body.contains(['"', '\\'])
        || !log
            .key
            .verify(canonical.as_bytes(), &b64(&set.signed_entry_timestamp)?)
    {
        return Err("its transparency log entry isn't signed by Sigstore's log".into());
    }

    // The inclusion proof up to a checkpoint the log signed.
    let body = b64(&e.canonicalized_body).map_err(|_| "its transparency log entry is damaged")?;
    let proof = e
        .inclusion_proof
        .as_ref()
        .ok_or("its transparency log entry has no inclusion proof")?;
    check_inclusion(proof, &body, log)?;

    // The entry is this signature over this statement.
    check_body(e, &body, payload, sig, leaf_der)
}

fn check_inclusion(p: &Proof, body: &[u8], log: &Log) -> Result<(), String> {
    let bad = || "its transparency log inclusion proof doesn't verify".to_string();
    let mut leaf = Sha256::new();
    leaf.update([0u8]);
    leaf.update(body);
    let leaf: [u8; 32] = leaf.finalize().into();
    let hashes = p
        .hashes
        .iter()
        .map(|h| b64(h).ok().and_then(|v| <[u8; 32]>::try_from(v).ok()))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)?;
    let root_hash = b64(&p.root_hash).map_err(|_| bad())?;
    if p.log_index < 0 || p.tree_size < 0 {
        return Err(bad());
    }
    let computed =
        root_from_proof(p.log_index as u64, p.tree_size as u64, leaf, &hashes).ok_or_else(bad)?;
    if computed[..] != root_hash[..] {
        return Err(bad());
    }
    let cp = p
        .checkpoint
        .as_ref()
        .ok_or("its transparency log inclusion proof has no checkpoint")?;
    check_checkpoint(&cp.envelope, p.tree_size as u64, &root_hash, log)
}

/// RFC 9162 §2.1.3.2: the tree root from a leaf and its audit path.
fn root_from_proof(index: u64, size: u64, leaf: [u8; 32], proof: &[[u8; 32]]) -> Option<[u8; 32]> {
    if index >= size {
        return None;
    }
    let inner = (64 - (index ^ (size - 1)).leading_zeros()) as usize;
    let border = (index >> inner).count_ones() as usize;
    if proof.len() != inner + border {
        return None;
    }
    let node = |l: &[u8; 32], r: &[u8; 32]| -> [u8; 32] {
        let mut h = Sha256::new();
        h.update([1u8]);
        h.update(l);
        h.update(r);
        h.finalize().into()
    };
    let mut res = leaf;
    for (i, p) in proof[..inner].iter().enumerate() {
        res = if (index >> i) & 1 == 0 {
            node(&res, p)
        } else {
            node(p, &res)
        };
    }
    for p in &proof[inner..] {
        res = node(p, &res);
    }
    Some(res)
}

/// A signed note: "origin\nsize\nroot\n[extra lines]\n\n— name base64(hint+sig)\n".
fn check_checkpoint(envelope: &str, size: u64, root_hash: &[u8], log: &Log) -> Result<(), String> {
    let bad = || "its transparency log checkpoint doesn't verify".to_string();
    let split = envelope.find("\n\n").ok_or_else(bad)?;
    let (text, sigs) = (&envelope[..split + 1], &envelope[split + 2..]);
    let mut lines = text.lines();
    let _origin = lines.next().ok_or_else(bad)?;
    let n: u64 = lines.next().and_then(|l| l.parse().ok()).ok_or_else(bad)?;
    let r = lines.next().and_then(|l| b64(l).ok()).ok_or_else(bad)?;
    if n != size || r != root_hash {
        return Err(bad());
    }
    let signed = sigs
        .lines()
        .filter_map(|l| l.strip_prefix("\u{2014} "))
        .any(|l| {
            let Some((_name, sig)) = l.rsplit_once(' ') else {
                return false;
            };
            let Ok(sig) = b64(sig) else { return false };
            sig.len() > 4
                && sig[..4] == log.log_id[..4]
                && log.key.verify(text.as_bytes(), &sig[4..])
        });
    if !signed {
        return Err(bad());
    }
    Ok(())
}

/// The logged entry (Rekor's canonical body) must be for this envelope.
fn check_body(
    e: &TlogEntry,
    body: &[u8],
    payload: &[u8],
    sig: &[u8],
    leaf_der: &[u8],
) -> Result<(), String> {
    let mismatch = || "the transparency log entry is for a different signature".to_string();
    let v: Value =
        serde_json::from_slice(body).map_err(|_| "its transparency log entry is damaged")?;
    if v["kind"] != e.kind_version.kind || v["apiVersion"] != e.kind_version.version {
        return Err(mismatch());
    }
    if e.kind_version.kind != "dsse" || e.kind_version.version != "0.0.1" {
        return Err(format!(
            "its transparency log entry is of a kind this version can't check ({} {})",
            e.kind_version.kind, e.kind_version.version
        ));
    }
    let spec = &v["spec"];
    if spec["payloadHash"]["algorithm"] != "sha256"
        || !spec["payloadHash"]["value"]
            .as_str()
            .is_some_and(|h| h.eq_ignore_ascii_case(&to_hex(&Sha256::digest(payload))))
    {
        return Err(mismatch());
    }
    let ours = spec["signatures"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|s| {
            let same_sig = s["signature"]
                .as_str()
                .and_then(|x| b64(x).ok())
                .is_some_and(|x| x == sig);
            let same_cert = s["verifier"]
                .as_str()
                .and_then(|x| b64(x).ok())
                .and_then(|pem| pem_to_der(&pem))
                .is_some_and(|d| d == leaf_der);
            same_sig && same_cert
        });
    if !ours {
        return Err(mismatch());
    }
    Ok(())
}

fn pem_to_der(pem: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(pem).ok()?;
    let body: String = text
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE-----"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END"))
        .collect();
    B64.decode(body.trim()).ok()
}

// ---- Small DER helpers (the parts x509-cert doesn't expose) ----

/// One DER element: (tag, header+content, content, rest).
type Element<'a> = (u8, &'a [u8], &'a [u8], &'a [u8]);

fn tlv(b: &[u8]) -> Option<Element<'_>> {
    let tag = *b.first()?;
    let first = *b.get(1)?;
    let (len, hdr) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *b.get(2 + i)? as usize;
        }
        (len, 2 + n)
    };
    let end = hdr.checked_add(len)?;
    if b.len() < end {
        return None;
    }
    Some((tag, &b[..end], &b[hdr..end], &b[end..]))
}

fn encode(tag: u8, content: &[u8]) -> Vec<u8> {
    let n = content.len();
    let mut out = vec![tag];
    if n < 0x80 {
        out.push(n as u8);
    } else {
        let bytes = n.to_be_bytes();
        let skip = bytes.iter().take_while(|&&b| b == 0).count();
        out.push(0x80 | (bytes.len() - skip) as u8);
        out.extend_from_slice(&bytes[skip..]);
    }
    out.extend_from_slice(content);
    out
}

/// The first element inside a SEQUENCE (a certificate's TBS part), as is.
fn first_child(seq: &[u8]) -> Option<&[u8]> {
    let (_, _, content, _) = tlv(seq)?;
    let (_, whole, _, _) = tlv(content)?;
    Some(whole)
}

/// A TBSCertificate re-encoded without the extension whose OID (DER) is
/// `oid_der`: the precertificate the CT log signed.
fn without_extension(tbs: &[u8], oid_der: &[u8]) -> Option<Vec<u8>> {
    let (tag, _, mut rest, _) = tlv(tbs)?;
    let mut fields = Vec::new();
    while !rest.is_empty() {
        let (t, whole, content, next) = tlv(rest)?;
        rest = next;
        if t != 0xa3 {
            fields.extend_from_slice(whole);
            continue;
        }
        let (seq_tag, _, mut exts, _) = tlv(content)?;
        let mut kept = Vec::new();
        while !exts.is_empty() {
            let (_, ext, ext_content, next) = tlv(exts)?;
            exts = next;
            let (_, oid, _, _) = tlv(ext_content)?;
            if oid != oid_der {
                kept.extend_from_slice(ext);
            }
        }
        fields.extend(encode(0xa3, &encode(seq_tag, &kept)));
    }
    Some(encode(tag, &fields))
}

// ---- Encodings ----

fn b64(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s.trim())
        .map_err(|_| "the build proof is damaged (bad base64)".to_string())
}

pub(super) fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub(super) fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE: &str =
        include_str!("../../resources/sigstore/test/jamalterm-3.5.0-macos.zip.sigstore.json");
    const SUMS: &str = include_str!("../../resources/sigstore/test/SHA256SUMS-3.5.0.txt");
    const ZIP_SHA: &str = "06ec8312305478372c2877b7b020e0495e94f4f34ef948a868cac839e994eb19";

    fn root() -> TrustedRoot {
        TrustedRoot::embedded().unwrap()
    }

    fn verify_value(b: &Value) -> Result<(), String> {
        verify(&serde_json::to_vec(b).unwrap(), ZIP_SHA)
    }

    fn bundle() -> Value {
        serde_json::from_str(BUNDLE).unwrap()
    }

    fn expect_err(r: Result<(), String>, part: &str) {
        let e = r.expect_err("should not verify");
        assert!(
            e.starts_with("the download isn't proven to be built by jamalterm's GitHub workflow: "),
            "{e}"
        );
        assert!(e.contains(part), "{e}");
    }

    #[test]
    fn the_real_3_5_0_release_verifies() {
        assert!(SUMS.contains(&format!("{ZIP_SHA}  jamalterm-3.5.0-macos.zip")));
        verify(BUNDLE.as_bytes(), ZIP_SHA).unwrap();
        // Upper-case hex is the same checksum.
        verify(BUNDLE.as_bytes(), &ZIP_SHA.to_uppercase()).unwrap();
    }

    #[test]
    fn a_different_file_is_refused() {
        // The dmg from the same release: real, but not what the proof covers.
        let dmg = "a0f9bd3ca3707c6fc4e739cbd31c0533b3f6cef65d18479b85b3f42a4e940ac3";
        assert!(SUMS.contains(dmg));
        expect_err(verify(BUNDLE.as_bytes(), dmg), "it's for a different file");
        assert_eq!(
            verify(BUNDLE.as_bytes(), "nope").unwrap_err(),
            "bad checksum"
        );
    }

    #[test]
    fn a_tampered_statement_is_refused() {
        let mut b = bundle();
        let payload = B64
            .decode(b["dsseEnvelope"]["payload"].as_str().unwrap())
            .unwrap();
        let changed = String::from_utf8(payload)
            .unwrap()
            .replace(ZIP_SHA, &"0".repeat(64));
        b["dsseEnvelope"]["payload"] = Value::String(B64.encode(changed));
        expect_err(
            verify_value(&b),
            "the signature doesn't match the build statement",
        );
    }

    #[test]
    fn a_tampered_signature_is_refused() {
        let mut b = bundle();
        let mut sig = B64
            .decode(b["dsseEnvelope"]["signatures"][0]["sig"].as_str().unwrap())
            .unwrap();
        let n = sig.len();
        sig[n - 3] ^= 1;
        b["dsseEnvelope"]["signatures"][0]["sig"] = Value::String(B64.encode(sig));
        expect_err(
            verify_value(&b),
            "the signature doesn't match the build statement",
        );
    }

    #[test]
    fn another_repo_or_workflow_is_refused() {
        // The same pattern for other names: the real certificate (main of
        // jamalxcode/jamalterm, build.yml) must not pass.
        for san in [
            r"^https://github\.com/someone/jamalterm/\.github/workflows/build\.yml@refs/heads/main$",
            r"^https://github\.com/jamalxcode/jamalterm/\.github/workflows/release\.yml@refs/heads/main$",
            r"^https://github\.com/jamalxcode/jamalterm/\.github/workflows/build\.yml@refs/tags/v[0-9.]+$",
        ] {
            let policy = Policy {
                issuer: BUILDER_ISSUER.into(),
                san: Regex::new(san).unwrap(),
            };
            expect_err(
                verify_with(BUNDLE.as_bytes(), ZIP_SHA, &policy, &root()),
                "it was built by https://github.com/jamalxcode/jamalterm/.github/workflows/build.yml@refs/heads/main, not jamalterm's build workflow",
            );
        }
        let policy = Policy {
            issuer: "https://accounts.google.com".into(),
            san: Regex::new(BUILDER_SAN).unwrap(),
        };
        expect_err(
            verify_with(BUNDLE.as_bytes(), ZIP_SHA, &policy, &root()),
            "identity from https://token.actions",
        );
    }

    #[test]
    fn the_jamalterm_pattern_allows_main_and_version_tags_only() {
        let p = Policy::jamalterm();
        let base = "https://github.com/jamalxcode/jamalterm/.github/workflows/build.yml@refs/";
        for ok in ["heads/main", "tags/v3.5.0", "tags/v3.6.0-beta.1"] {
            assert!(p.san.is_match(&format!("{base}{ok}")), "{ok}");
        }
        for bad in [
            "heads/feature",
            "pull/1/merge",
            "tags/latest",
            "heads/main2",
        ] {
            assert!(!p.san.is_match(&format!("{base}{bad}")), "{bad}");
        }
    }

    #[test]
    fn a_certificate_outside_its_validity_is_refused() {
        // A day after the 10-minute certificate expired.
        let mut b = bundle();
        let t: i64 = b["verificationMaterial"]["tlogEntries"][0]["integratedTime"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        b["verificationMaterial"]["tlogEntries"][0]["integratedTime"] =
            Value::String((t + 86400).to_string());
        expect_err(
            verify_value(&b),
            "its certificate wasn't valid when the build was logged",
        );
        // A time before it was issued.
        b["verificationMaterial"]["tlogEntries"][0]["integratedTime"] =
            Value::String((t - 3600).to_string());
        expect_err(
            verify_value(&b),
            "its certificate wasn't valid when the build was logged",
        );
    }

    #[test]
    fn a_changed_log_time_breaks_the_logs_signature() {
        // Still inside the certificate's 10 minutes, but not what Rekor signed.
        let mut b = bundle();
        let t: i64 = b["verificationMaterial"]["tlogEntries"][0]["integratedTime"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        b["verificationMaterial"]["tlogEntries"][0]["integratedTime"] =
            Value::String((t + 1).to_string());
        expect_err(verify_value(&b), "isn't signed by Sigstore's log");
    }

    #[test]
    fn a_tampered_log_entry_is_refused() {
        let mut b = bundle();
        let entry = &mut b["verificationMaterial"]["tlogEntries"][0];
        let mut set = B64
            .decode(
                entry["inclusionPromise"]["signedEntryTimestamp"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        let n = set.len();
        set[n - 2] ^= 1;
        entry["inclusionPromise"]["signedEntryTimestamp"] = Value::String(B64.encode(set));
        expect_err(verify_value(&b), "isn't signed by Sigstore's log");

        let mut b = bundle();
        let proof = &mut b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"];
        proof["hashes"][0] = Value::String(B64.encode([7u8; 32]));
        expect_err(verify_value(&b), "inclusion proof doesn't verify");

        let mut b = bundle();
        let cp = &mut b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]["envelope"];
        let text =
            cp.as_str()
                .unwrap()
                .replacen("rekor.sigstore.dev - ", "rekor.sigstore.dev - 9", 1);
        *cp = Value::String(text);
        expect_err(verify_value(&b), "checkpoint doesn't verify");

        let mut b = bundle();
        b["verificationMaterial"]["tlogEntries"] = Value::Array(vec![]);
        expect_err(verify_value(&b), "isn't in Sigstore's transparency log");
    }

    #[test]
    fn unknown_keys_are_refused() {
        // Sigstore's CT log key swapped for the Rekor key: the SCT fails.
        let mut r = root();
        let rekor = TrustedRoot::embedded().unwrap().tlogs.remove(0);
        for l in &mut r.ctlogs {
            l.key = Key::P256(match &rekor.key {
                Key::P256(k) => *k,
                _ => unreachable!(),
            });
        }
        expect_err(
            verify_with(BUNDLE.as_bytes(), ZIP_SHA, &Policy::jamalterm(), &r),
            "certificate transparency proof (SCT) doesn't verify",
        );
        // No Fulcio CA: the certificate isn't trusted.
        let mut r = root();
        r.cas.clear();
        expect_err(
            verify_with(BUNDLE.as_bytes(), ZIP_SHA, &Policy::jamalterm(), &r),
            "wasn't issued by Sigstore's certificate authority",
        );
        // The log listed under another key.
        let mut r = root();
        for l in &mut r.tlogs {
            l.key = Key::Other("PKIX_ED25519".into());
        }
        expect_err(
            verify_with(BUNDLE.as_bytes(), ZIP_SHA, &Policy::jamalterm(), &r),
            "can't check yet",
        );
    }

    #[test]
    fn damaged_proofs_say_so() {
        assert!(
            verify(b"not json", ZIP_SHA)
                .unwrap_err()
                .starts_with("the build proof is damaged")
        );
        let mut b = bundle();
        b.as_object_mut().unwrap().remove("dsseEnvelope");
        expect_err(verify_value(&b), "it isn't a signed build statement");
    }

    /// 3.x's cmd/verify-release, as a test CI can run on a new build:
    /// `JT_VERIFY_RELEASE="<zip> <bundle> [other-file]" cargo test verify_release`.
    /// With other-file it also checks the proof does NOT pass for another
    /// file, so a check that accepts anything can't slip through.
    #[test]
    fn verify_release() {
        let Ok(args) = std::env::var("JT_VERIFY_RELEASE") else {
            return;
        };
        let args: Vec<&str> = args.split_whitespace().collect();
        let sum = |p: &str| to_hex(&Sha256::digest(std::fs::read(p).unwrap()));
        let proof = std::fs::read(args[1]).unwrap();
        verify(&proof, &sum(args[0])).unwrap();
        if let Some(other) = args.get(2) {
            assert!(
                verify(&proof, &sum(other)).is_err(),
                "the proof also passed for {other}; the check is broken"
            );
        }
    }

    #[test]
    fn helpers() {
        assert_eq!(rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339("2022-12-31T23:59:59.999Z"), Some(1672531199));
        assert_eq!(
            rfc3339("2026-11-10T07:45:30Z"),
            Some(1794296730 - 1794296730 % 86400 + 7 * 3600 + 45 * 60 + 30)
        );
        assert_eq!(from_hex("0aFf"), Some(vec![0x0a, 0xff]));
        assert_eq!(to_hex(&[0x0a, 0xff]), "0aff");
        // A one-leaf tree's root is the leaf hash.
        assert_eq!(root_from_proof(0, 1, [3; 32], &[]), Some([3; 32]));
        assert_eq!(root_from_proof(1, 1, [3; 32], &[]), None);
        // DER lengths round-trip, short and long form.
        for n in [0usize, 5, 127, 128, 300, 70000] {
            let e = encode(0x04, &vec![1; n]);
            let (t, whole, content, rest) = tlv(&e).unwrap();
            assert_eq!(
                (t, whole.len(), content.len(), rest.len()),
                (4, e.len(), n, 0)
            );
        }
        assert_eq!(pae("t", b"ab"), b"DSSEv1 1 t 2 ab");
    }
}
