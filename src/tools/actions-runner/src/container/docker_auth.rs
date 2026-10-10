//! Registry credentials: reading Docker's config file and encoding what the
//! daemon expects on a pull.
//!
//! A job's `image:` decides both *which* registry is contacted and *whose*
//! credentials are sent there. The second is this module, and it has three steps
//! worth naming because all three are observable and none is obvious.
//!
//! # The `auth` field is base64 of `user:password`, and the password keeps its
//! # newline
//!
//! Docker's `config.json` stores `"auth": "dXNlcm5hbWU6cGFzc3dvcmQK"` — which
//! decodes to `username:password\n`, newline included. Docker splits on the
//! **first** colon and takes everything after it verbatim, so the password
//! really does end in a newline. A credential helper that trims whitespace
//! would produce a different `RegistryAuth` header and be rejected by the
//! registry, so the split and the trailing byte are both preserved.
//!
//! # `serveraddress` is the config's **key**, not a field in the entry
//!
//! Docker reads every entry, decodes its `auth`, and then overwrites
//! `ServerAddress` with the map key — unconditionally, discarding any
//! `ServerAddress` the file itself contained. A config entry for
//! `https://index.docker.io/v1/` therefore reports that whole string back,
//! scheme and `/v1/` path included, and that exact string is what gets sent to
//! the registry.
//!
//! # A lookup is an exact match, then a *normalisation* match
//!
//! act asks for the credential under a bare hostname — `index.docker.io` —
//! while a real config is keyed by whatever the user typed, historically
//! `https://index.docker.io/v1/`. Docker resolves this in two steps: an exact
//! key hit first, then every key run through [`convert_to_hostname`] and
//! compared. The normalisation is a URL parse when the key has a `://`, and a
//! cut at the first `/` otherwise — so `https://index.docker.io/v1/`,
//! `index.docker.io/v1/` and `index.docker.io` all resolve to the same
//! credential, and `registry.internal:5000` keeps its port.
//!
//! # The host rule here is *not* the host rule in [`super::image_ref`]
//!
//! act picks the credential to look up with its own test: the first `/`
//! component is a registry iff it contains `.` or `:` or is `localhost`.
//! `image_ref::split_docker_domain` has a fifth clause for uppercase
//! namespaces. So `UPPER/case` **pulls** from the registry `UPPER` but
//! **looks up credentials** under `index.docker.io`.
//!
//! That is upstream's behaviour, not a port artifact. Both functions are here
//! side by side so the divergence stays visible, and
//! [`registry_host_versus_pull_domain`] pins it.
//!
//! # `RegistryAuth` is base64 **URL-safe**
//!
//! `base64.URLEncoding` — padded, URL-safe alphabet — of a JSON object with
//! exactly three fields in this order, each omitted when empty. The order and
//! the omission are load-bearing: the registry parses this, and upstream's test
//! compares the encoded string byte for byte.
//!
//! # Deliberate deviation: credential *helpers* are not ported
//!
//! Docker can keep credentials outside the config file and delegate to an
//! external binary — `credsStore` on macOS means `osxkeychain`, and
//! `credHelpers` names a per-registry program. Those are child-process
//! invocations of tools the CTOX host may or may not have, and this port reads
//! the **file** only. Two consequences, both faithful to the file-only shape:
//!
//! * a config with no `auths` and no `credsStore` finds nothing here, where
//!   act on macOS would consult the keychain;
//! * a `credsStore`/`credHelpers` entry is read and its `auths` are still used,
//!   where act would have shelled out.
//!
//! [`LoadDockerAuthConfig`] upstream also *downgrades to* the keychain exactly
//! in that first case (`DetectDefaultStore` when the file holds no auth), so
//! this is the one place where a macOS user with a keychain-only login will
//! differ. Serving a job that needs a private image then means writing the
//! credential into the config file, which is the same thing the helper would
//! have done for Docker itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::base64url;

/// Docker Hub, as the credential lookup names it.
pub const DEFAULT_REGISTRY: &str = "index.docker.io";

/// What the registry is sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryAuthConfig {
    /// The user.
    pub username: String,
    /// The password or token, whatever bytes it has.
    pub password: String,
    /// The config file's **key** for this entry, not a field inside it.
    pub server_address: String,
}

impl RegistryAuthConfig {
    /// Whether there is anything to send.
    pub fn is_empty(&self) -> bool {
        self.username.is_empty() && self.password.is_empty()
    }

    /// The `RegistryAuth` header value.
    ///
    /// Three fields, in this order, with empty ones omitted — the shape
    /// `registry.AuthConfig` marshals to and the registry expects back. Go's
    /// `omitempty` is why a credential built from a username and a password
    /// alone encodes as a two-field object.
    pub fn encoded(&self) -> String {
        let mut json = String::from("{");
        let mut first = true;
        for (name, value) in [
            ("username", &self.username),
            ("password", &self.password),
            ("serveraddress", &self.server_address),
        ] {
            if value.is_empty() {
                continue;
            }
            if !first {
                json.push(',');
            }
            first = false;
            json.push('"');
            json.push_str(name);
            json.push_str("\":\"");
            json.push_str(&json_string(value));
            json.push('"');
        }
        json.push('}');
        base64url::encode_padded(json.as_bytes())
    }
}

/// A JSON string literal, escaped the way `encoding/json` escapes.
///
/// A password arrives from a base64 field and can hold anything, so the
/// quoting is done by hand rather than by a `format!` template: a quote or a
/// backslash in a token would otherwise produce invalid JSON and a header the
/// registry cannot parse.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The registry whose credentials apply to `image`.
///
/// act's rule, which differs from the one used to decide where to pull: a dot,
/// a colon, or exactly `localhost`. No uppercase clause, and no `library/`
/// handling — this is only about which config entry to look up.
pub fn registry_host(image: &str) -> String {
    if let Some(index) = image.find('/') {
        let candidate = &image[..index];
        if candidate.contains('.') || candidate.contains(':') || candidate == "localhost" {
            return candidate.to_string();
        }
    }
    DEFAULT_REGISTRY.to_string()
}

/// The one credential for `image` from Docker's config file.
///
/// A config directory that does not exist, or holds no matching entry, yields
/// an empty config rather than an error: a pull from a public image must work
/// with no credentials at all. A *malformed* file is an error, because Docker
/// reports one and silently ignoring it would hide a typo in a path the user
/// does control.
/// `config.Dir()`: where Docker keeps its `config.json`.
///
/// `DOCKER_CONFIG` wins, then the user's home directory — and on Windows the
/// home variable is `USERPROFILE`, not `HOME`, which is the whole reason this is
/// a function rather than a constant. An empty `DOCKER_CONFIG` is treated as
/// unset, because an empty path is not a directory.
///
/// Upstream is `config.Dir()` from `github.com/docker/cli`, which applies the
/// same two rules. It lives here because this module reads the file, and both
/// the pull path and the build path need it — one definition, so the two cannot
/// drift apart on a platform where `HOME` is set but `USERPROFILE` is not.
pub fn docker_config_dir() -> Result<PathBuf, std::io::Error> {
    if let Ok(dir) = std::env::var("DOCKER_CONFIG") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    // A machine with neither variable set has no home to speak of, and the
    // error says which one was looked for rather than the `VarError` enum,
    // which is a different failure (`NotUnicode`) wearing the same shape.
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "neither $HOME nor $USERPROFILE is set, so the docker config directory is unknown",
            )
        })?;
    Ok(PathBuf::from(home).join(".docker"))
}

pub fn load_docker_auth_config(
    config_dir: &Path,
    image: &str,
) -> Result<RegistryAuthConfig, AuthConfigError> {
    let file = DockerConfigFile::load(config_dir)?;
    let host = registry_host(image);
    Ok(file.auth(&host).unwrap_or_default())
}

/// Every credential in the config file, keyed by the file's own server address.
///
/// Used when a workflow names several private images, each from a different
/// registry. `LoadDockerAuthConfigs` upstream, with the credential-helper path
/// dropped — see the module head.
pub fn load_docker_auth_configs(
    config_dir: &Path,
) -> Result<BTreeMap<String, RegistryAuthConfig>, AuthConfigError> {
    Ok(DockerConfigFile::load(config_dir)?.all())
}

/// A parsed `config.json`, reduced to what a pull needs.
#[derive(Debug, Default)]
pub struct DockerConfigFile {
    /// The `auths` object, in file order.
    auths: Vec<(String, RegistryAuthConfig)>,
}

impl DockerConfigFile {
    /// Reads `config_dir/config.json`.
    ///
    /// A missing file is an empty config, not a failure — that is Docker's own
    /// rule and the reason a public pull works on a machine that has never run
    /// `docker login`.
    pub fn load(config_dir: &Path) -> Result<Self, AuthConfigError> {
        let path = config_dir.join("config.json");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(err) => return Err(AuthConfigError::Unreadable(path, err.to_string())),
        };
        let document: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|err| AuthConfigError::Malformed(path, err.to_string()))?;

        let mut auths = Vec::new();
        if let Some(map) = document.get("auths").and_then(|auths| auths.as_object()) {
            for (key, entry) in map {
                auths.push((key.clone(), read_entry(key, entry)));
            }
        }
        Ok(Self { auths })
    }

    /// The credential for `host`, by Docker's two-step rule.
    pub fn auth(&self, host: &str) -> Option<RegistryAuthConfig> {
        // Step one: the key used verbatim.
        if let Some((_, config)) = self.auths.iter().find(|(key, _)| key == host) {
            return Some(config.clone());
        }
        // Step two: the key normalised, for the legacy spellings.
        self.auths
            .iter()
            .find(|(key, _)| convert_to_hostname(key) == host)
            .map(|(_, config)| config.clone())
    }

    /// Every credential, keyed by the file's own server address.
    pub fn all(&self) -> BTreeMap<String, RegistryAuthConfig> {
        self.auths.iter().cloned().collect()
    }

    /// Whether the file holds anything to authenticate with.
    ///
    /// Docker checks this to decide whether to fall back to a credential
    /// helper; this port has no helper, so the answer only reports what the
    /// file itself contributes.
    pub fn contains_auth(&self) -> bool {
        !self.auths.is_empty()
    }
}

/// One `auths` entry, decoded.
///
/// `serveraddress` is the **key**, always: Docker overwrites whatever the entry
/// said, so a hand-written `ServerAddress` field is discarded.
fn read_entry(key: &str, entry: &serde_json::Value) -> RegistryAuthConfig {
    let server_address = key.to_string();
    match entry.get("auth").and_then(|value| value.as_str()) {
        // The encoded form wins, and it is decoded at the first colon.
        Some(encoded) => {
            let (username, password) = decode_basic_auth(encoded);
            RegistryAuthConfig {
                username,
                password,
                server_address,
            }
        }
        // A config may name the fields directly instead of encoding them.
        None => RegistryAuthConfig {
            username: string_field(entry, "username"),
            password: string_field(entry, "password"),
            server_address,
        },
    }
}

fn string_field(entry: &serde_json::Value, name: &str) -> String {
    entry
        .get(name)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

/// `base64.StdEncoding.DecodeString`, then split at the **first** colon.
///
/// Everything after the colon is the password, newline and all. A value that is
/// not base64 decodes to nothing rather than failing: a single bad entry should
/// not make every other credential in the file unusable.
fn decode_basic_auth(encoded: &str) -> (String, String) {
    // `base64url::decode` accepts the standard alphabet as well, which is what
    // this needs: Docker writes `+` and `/`, not `-` and `_`.
    let decoded = base64url::decode(encoded);
    let text = String::from_utf8_lossy(&decoded).into_owned();
    match text.find(':') {
        Some(index) => (text[..index].to_string(), text[index + 1..].to_string()),
        None => (text, String::new()),
    }
}

/// `credentials.ConvertToHostname`: the bare host a config key belongs to.
///
/// A URL becomes its authority — host, plus the port when there is one — and
/// anything else is cut at the first `/`. The port survives because a private
/// registry without it is a different registry, and dropping it would send
/// credentials meant for `registry.internal:5000` to a bare `registry.internal`.
pub fn convert_to_hostname(maybe_url: &str) -> String {
    if maybe_url.contains("://") {
        let rest = match maybe_url.split_once("://") {
            Some((_, rest)) => rest,
            None => return maybe_url.to_string(),
        };
        // Drop the path, then the query or fragment, keeping the authority.
        let authority = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        // Credentials in the authority are not part of the hostname.
        let authority = authority.rsplit('@').next().unwrap_or(authority);
        if let Some(authority) = authority.strip_prefix('[') {
            // An IPv6 literal, optionally with a port: `[::1]:5000`.
            if let Some((host, tail)) = authority.split_once(']') {
                return match tail.strip_prefix(':') {
                    Some(port) if !port.is_empty() => format!("[{host}]:{port}"),
                    _ => format!("[{host}]"),
                };
            }
            return format!("[{authority}]");
        }
        if !authority.is_empty() {
            return authority.to_string();
        }
    }
    maybe_url.split('/').next().unwrap_or_default().to_string()
}

/// Why a config file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthConfigError {
    /// The file exists but could not be read.
    Unreadable(PathBuf, String),
    /// The file is not valid JSON.
    Malformed(PathBuf, String),
}

impl std::fmt::Display for AuthConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(path, message) => {
                write!(f, "could not read {}: {message}", path.display())
            }
            Self::Malformed(path, message) => {
                write!(f, "could not parse {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for AuthConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// docker_pull_test.go: TestGetImagePullOptions, the explicit-credentials
    /// half. The expected string is upstream's, byte for byte.
    ///
    /// `serveraddress` is absent because act builds the config from a username
    /// and a password alone and Go's `omitempty` drops the empty field.
    #[test]
    fn explicit_credentials_encode_as_upstream_expects() {
        let config = RegistryAuthConfig {
            username: "username".to_string(),
            password: "password".to_string(),
            server_address: String::new(),
        };
        assert_eq!(
            config.encoded(),
            "eyJ1c2VybmFtZSI6InVzZXJuYW1lIiwicGFzc3dvcmQiOiJwYXNzd29yZCJ9",
        );
    }

    /// docker_pull_test.go: TestGetImagePullOptions, the config-file half.
    ///
    /// The password keeps its newline, because the file's `auth` field is
    /// base64 of `username:password\n` and Docker splits at the first colon
    /// without trimming. Trimming would produce a different header and be
    /// rejected by the registry.
    ///
    /// The config is the upstream fixture, `testdata/docker-pull-options`,
    /// copied verbatim — including that it has no `ServerAddress` field, so the
    /// key is what ends up in the header.
    #[test]
    fn a_config_file_credential_keeps_its_trailing_newline() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{
  "auths": {
          "https://index.docker.io/v1/": {
                  "auth": "dXNlcm5hbWU6cGFzc3dvcmQK"
          }
  }
}
"#,
        )
        .expect("written");

        let config = load_docker_auth_config(dir.path(), "nektos/act").expect("loaded");
        assert_eq!(config.username, "username");
        assert_eq!(config.password, "password\n", "the newline is part of it");
        assert_eq!(config.server_address, "https://index.docker.io/v1/");
        assert_eq!(
            config.encoded(),
            "eyJ1c2VybmFtZSI6InVzZXJuYW1lIiwicGFzc3dvcmQiOiJwYXNzd29yZFxuIiwic2VydmVyYWRkcmVzcyI6Imh0dHBzOi8vaW5kZXguZG9ja2VyLmlvL3YxLyJ9",
        );
    }

    /// The first half of `TestGetImagePullOptions`: no username, no password,
    /// and a config directory that does not exist. Empty header, no error.
    #[test]
    fn no_credentials_means_no_header_and_no_error() {
        let config = load_docker_auth_config(Path::new("/non-existent/docker"), "")
            .expect("a missing config is not an error");
        assert!(config.is_empty());
        assert_eq!(config.encoded(), "e30=", "the empty object, encoded");
    }

    /// `serveraddress` is the key even when the entry names one itself, because
    /// Docker overwrites it during load. Getting this wrong would send a header
    /// the registry does not recognise.
    #[test]
    fn the_key_wins_over_a_serveraddress_field() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"auths": {"ghcr.io": {"auth": "dXNlcjpwYXNz", "ServerAddress": "https://elsewhere.example/v2/"}}}"#,
        )
        .expect("written");
        let config = load_docker_auth_config(dir.path(), "ghcr.io/app").expect("loaded");
        assert_eq!(config.server_address, "ghcr.io");
    }

    /// A config without a matching entry contributes nothing, and the one that
    /// does match is found.
    #[test]
    fn a_config_without_a_matching_entry_contributes_nothing() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"auths": {"ghcr.io": {"auth": "dXNlcjpwYXNz"}}}"#,
        )
        .expect("written");
        let config = load_docker_auth_config(dir.path(), "alpine").expect("loaded");
        assert!(config.is_empty());

        let config = load_docker_auth_config(dir.path(), "ghcr.io/owner/app").expect("loaded");
        assert_eq!(config.username, "user");
        assert_eq!(config.password, "pass");
    }

    /// A private registry is keyed by its host, and the credential must not
    /// leak to a different image.
    #[test]
    fn a_private_registry_is_keyed_by_its_host() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"auths": {"registry.internal:5000": {"auth": "dXNlcjpwYXNz"}}}"#,
        )
        .expect("written");

        assert_eq!(
            load_docker_auth_config(dir.path(), "registry.internal:5000/app")
                .expect("loaded")
                .username,
            "user",
        );
        assert!(
            load_docker_auth_config(dir.path(), "other.registry/app")
                .expect("loaded")
                .is_empty(),
            "another registry gets no credential",
        );
    }

    /// A config may name the fields directly rather than encoding them.
    #[test]
    fn an_unencoded_credential_is_read_too() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"auths": {"ghcr.io": {"username": "u", "password": "p"}}}"#,
        )
        .expect("written");
        let config = load_docker_auth_config(dir.path(), "ghcr.io/app").expect("loaded");
        assert_eq!(config.username, "u");
        assert_eq!(config.password, "p");
    }

    #[test]
    fn a_malformed_config_file_is_reported() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(dir.path().join("config.json"), b"{not json").expect("written");
        assert!(matches!(
            load_docker_auth_config(dir.path(), "alpine"),
            Err(AuthConfigError::Malformed(_, _)),
        ));
    }

    /// A password containing a quote or a backslash has to survive the JSON
    /// encoding, and a newline in one has to come back as a newline.
    #[test]
    fn passwords_with_awkward_bytes_round_trip() {
        for password in ["p\"q", "p\\q", "p\nq", "p\tq", "pa ss", "ümlaut"] {
            let config = RegistryAuthConfig {
                username: "u".to_string(),
                password: password.to_string(),
                server_address: String::new(),
            };
            let encoded = config.encoded();
            let decoded = String::from_utf8(base64url::decode(&encoded)).expect("utf-8");
            let value: serde_json::Value = serde_json::from_str(&decoded).expect("valid JSON");
            assert_eq!(value["password"], password, "round trip for {password:?}");
        }
    }

    /// `convert_to_hostname`, which is what makes the legacy Hub key findable.
    ///
    /// The port case is the one that matters: dropping `:5000` would resolve a
    /// private registry's credential to a host that has none of its own.
    #[test]
    fn a_config_key_is_reduced_to_its_host() {
        let table: &[(&str, &str)] = &[
            ("index.docker.io", "index.docker.io"),
            ("index.docker.io/v1/", "index.docker.io"),
            ("https://index.docker.io/v1/", "index.docker.io"),
            ("https://ghcr.io", "ghcr.io"),
            ("ghcr.io/owner/app", "ghcr.io"),
            ("registry.internal:5000", "registry.internal:5000"),
            ("https://registry.internal:5000/v2/", "registry.internal:5000"),
            ("http://registry.internal:5000", "registry.internal:5000"),
            // Userinfo is not part of the hostname.
            ("https://user:pass@ghcr.io/v2/", "ghcr.io"),
            // An IPv6 literal keeps its brackets, and its port.
            ("[::1]:5000", "[::1]:5000"),
            ("https://[2001:db8::1]:5000/v2/", "[2001:db8::1]:5000"),
        ];
        for (input, want) in table {
            assert_eq!(convert_to_hostname(input), *want, "convert({input:?})");
        }
    }

    /// The two-step lookup: exact key first, normalised key second. Both steps
    /// are exercised, and a key that matches neither is a miss.
    #[test]
    fn a_credential_is_found_by_exact_then_by_normalised_key() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"auths": {
                 "exact.io":           {"auth": "ZXhhY3R1OnAx"},
                 "https://legacy.io/v2/": {"auth": "bGVnYWN5OnAy"}
               }}"#,
        )
        .expect("written");
        let file = DockerConfigFile::load(dir.path()).expect("loaded");

        // Step one, the verbatim key.
        assert_eq!(file.auth("exact.io").expect("found").password, "p1");
        // Step two, the normalised key.
        assert_eq!(file.auth("legacy.io").expect("found").password, "p2");
        // And a miss is a miss, not a wrong credential.
        assert!(file.auth("other.io").is_none());
    }

    /// A config with an empty `auths` reports no auth. Upstream would switch to
    /// the platform keychain here; this port reads the file only, and says so
    /// rather than pretending otherwise.
    #[test]
    fn an_empty_auths_object_contributes_nothing() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(dir.path().join("config.json"), r#"{"auths": {}}"#).expect("written");
        let file = DockerConfigFile::load(dir.path()).expect("loaded");
        assert!(!file.contains_auth());
        assert!(file.all().is_empty());
    }

    /// **The two host rules really are different**, in exactly two ways, and
    /// both are load-bearing.
    ///
    /// 1. Docker Hub: the pull goes to `docker.io`, the credential is looked up
    ///    under the historical `index.docker.io`. A workflow on a public image
    ///    therefore pulls from one host while sending a credential meant for
    ///    another — which is correct, because that is the key `docker login`
    ///    wrote.
    /// 2. Uppercase: `UPPER/case` pulls from the registry `UPPER` but looks its
    ///    credential up under Docker Hub, because this rule has no uppercase
    ///    clause.
    ///
    /// Everywhere else the two agree, which is what makes the two differences
    /// easy to miss. Spelled out as a table rather than asserted in a loop,
    /// because "they differ" without a list of where is not a test.
    #[test]
    fn registry_host_versus_pull_domain() {
        use crate::container::image_ref::split_docker_domain;
        let table: &[(&str, &str, &str)] = &[
            // image, credential host, pull domain
            ("ubuntu", DEFAULT_REGISTRY, "docker.io"),
            ("ubuntu:18.04", DEFAULT_REGISTRY, "docker.io"),
            ("cibuilds/hugo:0.53", DEFAULT_REGISTRY, "docker.io"),
            ("UPPER/case", DEFAULT_REGISTRY, "UPPER"),
            // These agree.
            ("localhost/ubuntu", "localhost", "localhost"),
            ("localhost:8000/canonical/ubuntu", "localhost:8000", "localhost:8000"),
            ("myhost.com/foo/bar", "myhost.com", "myhost.com"),
            ("ghcr.io/owner/app", "ghcr.io", "ghcr.io"),
        ];
        for (image, credential, pull) in table {
            assert_eq!(&registry_host(image), credential, "credential host for {image}");
            assert_eq!(
                split_docker_domain(image).0,
                *pull,
                "pull domain for {image}"
            );
        }
    }

    #[test]
    fn every_credential_can_be_loaded_at_once() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"auths": {
                 "ghcr.io": {"auth": "dXNlcjpwYXNz"},
                 "registry.internal:5000": {"auth": "YTpi"}
               }}"#,
        )
        .expect("written");
        let all = load_docker_auth_configs(dir.path()).expect("loaded");
        assert_eq!(all.len(), 2);
        assert_eq!(all["ghcr.io"].username, "user");
        assert_eq!(all["registry.internal:5000"].password, "b");
    }
}
