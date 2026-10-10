//! Image reference normalisation: `ubuntu` means `docker.io/library/ubuntu`.
//!
//! Every job pulls an image, and the name a workflow writes is almost never
//! the name the registry knows it by. `image: ubuntu` is Docker Hub's
//! `ubuntu`, which lives at `docker.io/library/ubuntu`; `image: cibuilds/hugo`
//! is `docker.io/cibuilds/hugo`. act normalises every reference through this
//! before pulling, and a wrong answer here means pulling the wrong image — or
//! nothing at all.
//!
//! Port of `github.com/distribution/reference` v0.6.0, which is what act's
//! `cleanImage` calls. The grammar, the domain rules and the error cases are
//! reproduced; the tests are act's own `TestCleanImage` table plus the cases
//! that table does not reach.
//!
//! # The domain rules, which are not one rule
//!
//! Deciding whether the first path component is a registry or part of the
//! image name is the whole job, and there are **five** ways to answer "domain":
//!
//! 1. there is no `/` at all → always a Docker Hub name, and a single
//!    component gains `library/`;
//! 2. the component is exactly `localhost` → a domain;
//! 3. the component is `index.docker.io` → the Docker Hub alias, canonicalised;
//! 4. the component contains `.` or `:` → a domain or an IP;
//! 5. the component is **not lowercase** → a domain, because image names
//!    must be lowercase and an uppercase namespace can only be a host.
//!
//! That last one surprises everybody. `UPPER/case` is *not* `docker.io/UPPER/case`
//! — it is the registry `UPPER` serving the image `case`. Getting it wrong
//! would send a pull to a host that does not exist.
//!
//! # Two domain rules, and they are not the same rule
//!
//! [`crate::container::docker_auth::registry_host`] applies a **different**
//! test to pick which credential to use: only `.`, `:` or `localhost`, with no
//! uppercase clause. So `UPPER/case` pulls from `UPPER` but looks up its
//! credentials under `index.docker.io`. That is upstream's behaviour, and the
//! two functions are ported side by side so the difference stays visible.

use std::sync::OnceLock;

use regex::Regex;

/// Docker Hub's canonical domain.
pub const DEFAULT_DOMAIN: &str = "docker.io";
/// The other spelling of Docker Hub, canonicalised to [`DEFAULT_DOMAIN`].
pub const LEGACY_DEFAULT_DOMAIN: &str = "index.docker.io";
/// The prefix a bare Docker Hub image name gains.
pub const OFFICIAL_REPO_PREFIX: &str = "library/";
/// The reserved host that is always a domain.
pub const LOCALHOST: &str = "localhost";

/// `alphanumeric` from distribution/reference.
const ALPHANUMERIC: &str = r"[a-z0-9]+";
/// `separator`: one period, one or two underscores, or one or more dashes.
const SEPARATOR: &str = r"(?:[._]|__|[-]+)";
/// `domainNameComponent`.
const DOMAIN_NAME_COMPONENT: &str = r"(?:[a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9])";
/// `optionalPort`.
const OPTIONAL_PORT: &str = r"(?::[0-9]+)?";
/// `ipv6address`.
const IPV6_ADDRESS: &str = r"\[(?:[a-fA-F0-9:]+)\]";
/// `tag`.
const TAG: &str = r"[\w][\w.-]{0,127}";
/// `digestPat`.
const DIGEST: &str = r"[A-Za-z][A-Za-z0-9]*(?:[-_+.][A-Za-z][A-Za-z0-9]*)*:[0-9a-fA-F]{32,}";

/// A parsed image reference.
///
/// Upstream has **two** concrete types here, and keeping them apart is not
/// bookkeeping: a bare digest prints as `sha256:…`, while a name prints as
/// `domain/path[:tag][@digest]`. A single struct with an empty `path` and a
/// digest set would render as `@sha256:…` — a leading `@` and no name, which
/// upstream can never produce. Modelling the two cases as two variants makes
/// that broken third state unrepresentable instead of special-casing it in
/// `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// A content identifier or digest with no name, e.g. `sha256:…`.
    Digest(String),
    /// A named reference, optionally tagged and/or digested.
    Named {
        /// The registry. Empty only for a name that reached the grammar
        /// without a domain, which normalisation never produces.
        domain: String,
        /// The path within the registry, without the domain.
        path: String,
        /// The tag, when one was given. Never defaulted: `ubuntu` stays
        /// untagged so the daemon applies its own default rather than this
        /// port guessing one.
        tag: Option<String>,
        /// The digest, when one was given.
        digest: Option<String>,
    },
}

impl Reference {
    /// The registry, or `""` for a digest-only reference.
    pub fn domain(&self) -> &str {
        match self {
            Self::Digest(_) => "",
            Self::Named { domain, .. } => domain,
        }
    }

    /// The name within the registry, without domain, tag or digest.
    pub fn path(&self) -> &str {
        match self {
            Self::Digest(_) => "",
            Self::Named { path, .. } => path,
        }
    }

    /// The tag, if the reference carries one.
    pub fn tag(&self) -> Option<&str> {
        match self {
            Self::Digest(_) => None,
            Self::Named { tag, .. } => tag.as_deref(),
        }
    }

    /// The digest, if the reference carries one.
    ///
    /// A digest-only reference *is* its digest, which is why this is total
    /// rather than an `Option` on the named side.
    pub fn digest(&self) -> Option<&str> {
        match self {
            Self::Digest(digest) => Some(digest),
            Self::Named { digest, .. } => digest.as_deref(),
        }
    }
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // `digestReference.String()`: the digest and nothing else.
            Self::Digest(digest) => f.write_str(digest),
            Self::Named {
                domain,
                path,
                tag,
                digest,
            } => {
                if domain.is_empty() {
                    f.write_str(path)?;
                } else {
                    write!(f, "{domain}/{path}")?;
                }
                if let Some(tag) = tag {
                    write!(f, ":{tag}")?;
                }
                if let Some(digest) = digest {
                    write!(f, "@{digest}")?;
                }
                Ok(())
            }
        }
    }
}

struct Patterns {
    reference: Regex,
    name: Regex,
    identifier: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let domain_name = format!(r"{DOMAIN_NAME_COMPONENT}(?:\.{DOMAIN_NAME_COMPONENT})*");
        let host = format!("(?:{domain_name}|{IPV6_ADDRESS})");
        let domain_and_port = format!("{host}{OPTIONAL_PORT}");
        let path_component = format!(r"{ALPHANUMERIC}(?:(?:{SEPARATOR}){ALPHANUMERIC})*");
        let remote_name = format!(r"{path_component}(?:/{path_component})*");
        let name_pattern = format!(r"(?:{domain_and_port}/)?{remote_name}");
        let reference = format!(
            r"^({name_pattern})(?::({TAG}))?(?:@({DIGEST}))?$"
        );
        let name = format!(r"^(?:({domain_and_port})/)?({remote_name})$");
        Patterns {
            reference: Regex::new(&reference).expect("the reference grammar"),
            name: Regex::new(&name).expect("the name grammar"),
            identifier: Regex::new(r"^([a-f0-9]{64})$").expect("the identifier grammar"),
        }
    })
}

/// `cleanImage`: the normalised form of `image`, or `""` when it is not a
/// valid reference.
///
/// The empty string is what act pulls with when normalisation fails, and the
/// daemon rejects it — which is the intended outcome for a malformed `image:`.
pub fn clean_image(image: &str) -> String {
    match parse_any_reference(image) {
        Some(reference) => reference.to_string(),
        None => String::new(),
    }
}

/// `ParseAnyReference`.
///
/// Two shortcuts come first, and neither carries a name: a bare 64-digit hex
/// string is a sha256 content identifier, and an `algorithm:hex` pair is a
/// digest. Only what is left goes through the name grammar.
pub fn parse_any_reference(image: &str) -> Option<Reference> {
    let patterns = patterns();
    if patterns.identifier.is_match(image) {
        return Some(Reference::Digest(format!("sha256:{image}")));
    }
    if let Some(digest) = parse_digest(image) {
        return Some(Reference::Digest(digest));
    }
    parse_normalized_named(image)
}

/// `go-digest`'s parse: an algorithm, a colon, and at least 32 hex digits.
///
/// `distribution/reference`'s own pattern is slightly stricter (it excludes
/// urlsafe base64), so the two disagree on exotic digests. The stricter one is
/// used, because that is what has to hold for the reference to be accepted
/// afterwards.
fn parse_digest(value: &str) -> Option<String> {
    let pattern = Regex::new(&format!(r"^{DIGEST}$")).ok()?;
    pattern
        .is_match(value)
        .then(|| value.to_string())
}

/// `ParseNormalizedNamed`.
pub fn parse_normalized_named(image: &str) -> Option<Reference> {
    let patterns = patterns();
    if patterns.identifier.is_match(image) {
        // A bare 64-hex string cannot also be a repository name; upstream
        // rejects it here as well.
        return None;
    }
    let (domain, remainder) = split_docker_domain(image);

    // The remote name has to be lowercase. The `:` is a tag separator, not a
    // path separator, which is why a port-looking first component was already
    // taken as a domain above.
    let remote = match remainder.find(':') {
        Some(index) => &remainder[..index],
        None => remainder.as_str(),
    };
    if remote.to_lowercase() != remote {
        return None;
    }

    parse(&format!("{domain}/{remainder}"))
}

/// `Parse`: the name grammar plus an optional tag and digest.
fn parse(value: &str) -> Option<Reference> {
    let patterns = patterns();
    if value.is_empty() {
        return None;
    }
    let captures = patterns.reference.captures(value)?;
    let full = captures.get(0)?;
    if full.as_str() != value {
        // The pattern is anchored, so this cannot happen; the check is here
        // because `Parse` reports a format error rather than a no-match and
        // the two need to stay distinguishable.
        return None;
    }

    let name = captures.get(1)?.as_str().to_string();
    let tag = captures.get(2).map(|tag| tag.as_str().to_string());
    let digest = captures.get(3).map(|digest| digest.as_str().to_string());

    // `anchoredNameRegexp` decides whether the name has a domain at all.
    let name_captures = patterns.name.captures(&name);
    let (domain, path) = match name_captures {
        Some(found) if found.get(1).is_some() => (
            found.get(1).map(|domain| domain.as_str().to_string()).unwrap_or_default(),
            found.get(2)?.as_str().to_string(),
        ),
        _ => (String::new(), name.clone()),
    };

    Some(Reference::Named {
        domain,
        path,
        tag,
        digest,
    })
}

/// `splitDockerDomain`: the registry and the name, after canonicalisation.
pub fn split_docker_domain(name: &str) -> (String, String) {
    let (domain, mut remote_name) = match name.split_once('/') {
        // A single element is always a Docker Hub name. It has to be handled
        // before any port parsing, or `ubuntu:18.04` would look like a host
        // with a port.
        None => (
            DEFAULT_DOMAIN.to_string(),
            format!("{OFFICIAL_REPO_PREFIX}{name}"),
        ),
        Some((maybe_domain, maybe_remote)) => match maybe_domain {
            // `localhost` is reserved and always a domain.
            LOCALHOST => (maybe_domain.to_string(), maybe_remote.to_string()),
            // Both Docker Hub spellings canonicalise to the same domain.
            LEGACY_DEFAULT_DOMAIN => {
                (DEFAULT_DOMAIN.to_string(), maybe_remote.to_string())
            }
            // A dot means a domain or an IP; a colon means a port.
            candidate if candidate.contains('.') || candidate.contains(':') => {
                (candidate.to_string(), maybe_remote.to_string())
            }
            // Uppercase namespaces are not legal image names, so an uppercase
            // first component can only be a host.
            candidate if candidate.to_lowercase() != candidate => {
                (candidate.to_string(), maybe_remote.to_string())
            }
            // Otherwise it is part of the name, and the whole input is the
            // remote name — including the first component.
            _ => (DEFAULT_DOMAIN.to_string(), name.to_string()),
        },
    };

    // `library/` is added only on Docker Hub, and only for a bare name.
    if domain == DEFAULT_DOMAIN && !remote_name.contains('/') {
        remote_name = format!("{OFFICIAL_REPO_PREFIX}{remote_name}");
    }
    (domain, remote_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    // docker_pull_test.go: TestCleanImage
    #[test]
    fn image_names_match_the_upstream_table() {
        let table: &[(&str, &str)] = &[
            ("myhost.com/foo/bar", "myhost.com/foo/bar"),
            ("localhost:8000/canonical/ubuntu", "localhost:8000/canonical/ubuntu"),
            ("localhost/canonical/ubuntu:latest", "localhost/canonical/ubuntu:latest"),
            (
                "localhost:8000/canonical/ubuntu:latest",
                "localhost:8000/canonical/ubuntu:latest",
            ),
            ("ubuntu", "docker.io/library/ubuntu"),
            ("ubuntu:18.04", "docker.io/library/ubuntu:18.04"),
            ("cibuilds/hugo:0.53", "docker.io/cibuilds/hugo:0.53"),
        ];
        for (input, want) in table {
            assert_eq!(clean_image(input), *want, "cleanImage({input:?})");
        }
    }

    /// The rest of the table, captured by running
    /// `reference.ParseAnyReference` under Go 1.26.
    #[test]
    fn the_remaining_reference_shapes_match_go() {
        let table: &[(&str, &str)] = &[
            ("catthehacker/ubuntu:latest", "docker.io/catthehacker/ubuntu:latest"),
            ("docker.io/library/ubuntu", "docker.io/library/ubuntu"),
            ("docker.io/library/ubuntu:18.04", "docker.io/library/ubuntu:18.04"),
            // The Docker Hub alias canonicalises to the short spelling.
            ("index.docker.io/library/ubuntu", "docker.io/library/ubuntu"),
            (
                "registry.hub.docker.com/library/ubuntu",
                "registry.hub.docker.com/library/ubuntu",
            ),
            (
                "ubuntu@sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "docker.io/library/ubuntu@sha256:0000000000000000000000000000000000000000000000000000000000000000",
            ),
            ("localhost:5000/foo", "localhost:5000/foo"),
            ("192.168.1.1:5000/foo/bar:tag", "192.168.1.1:5000/foo/bar:tag"),
            // A two-component name does not gain `library/`.
            ("a/b/c/d:tag", "docker.io/a/b/c/d:tag"),
            (
                "sub.domain.io/ns/img@sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "sub.domain.io/ns/img@sha256:1111111111111111111111111111111111111111111111111111111111111111",
            ),
            // A tag is preserved verbatim, including its case.
            ("ubuntu:LATEST", "docker.io/library/ubuntu:LATEST"),
            // An uppercase first component is a *registry*, not a namespace.
            ("UPPER/case", "UPPER/case"),
        ];
        for (input, want) in table {
            assert_eq!(clean_image(input), *want, "cleanImage({input:?})");
        }
    }

    /// A malformed `image:` normalises to the empty string, and the daemon
    /// rejects that. Preserved so a typo fails loudly rather than pulling
    /// something unexpected.
    #[test]
    fn an_invalid_reference_normalises_to_nothing() {
        for input in [
            "",
            "not a ref",
            "-leadingdash",
            "ubuntu:",
            "Foo/Bar",
            "  ",
        ] {
            assert_eq!(clean_image(input), "", "cleanImage({input:?})");
        }
    }

    /// The five ways the first component becomes a registry. This is the rule
    /// that decides where a pull is sent.
    #[test]
    fn the_first_component_decides_the_registry() {
        // 1. No slash at all: a Docker Hub name, and `library/` is added.
        assert_eq!(
            split_docker_domain("ubuntu:18.04"),
            (DEFAULT_DOMAIN.to_string(), "library/ubuntu:18.04".to_string()),
        );
        // 2. `localhost` is reserved.
        assert_eq!(
            split_docker_domain("localhost/ubuntu"),
            (LOCALHOST.to_string(), "ubuntu".to_string()),
        );
        // 3. The Docker Hub alias canonicalises.
        assert_eq!(
            split_docker_domain("index.docker.io/library/ubuntu"),
            (DEFAULT_DOMAIN.to_string(), "library/ubuntu".to_string()),
        );
        // 4. A dot or a colon means a domain or an IP.
        assert_eq!(
            split_docker_domain("registry.example.com:5000/team/app"),
            (
                "registry.example.com:5000".to_string(),
                "team/app".to_string()
            ),
        );
        // 5. Uppercase means a host, because image names are lowercase.
        assert_eq!(
            split_docker_domain("UPPER/case"),
            ("UPPER".to_string(), "case".to_string()),
        );
        // And the default: not a domain, so the whole input is the name.
        assert_eq!(
            split_docker_domain("cibuilds/hugo:0.53"),
            (DEFAULT_DOMAIN.to_string(), "cibuilds/hugo:0.53".to_string()),
        );
    }

    /// `library/` is added only on Docker Hub and only for a bare name — a
    /// namespace on another registry is left alone.
    #[test]
    fn library_is_added_only_to_bare_hub_names() {
        assert_eq!(clean_image("ubuntu"), "docker.io/library/ubuntu");
        assert_eq!(clean_image("myorg/ubuntu"), "docker.io/myorg/ubuntu");
        assert_eq!(
            clean_image("ghcr.io/ubuntu"),
            "ghcr.io/ubuntu",
            "a bare name on another registry does not gain library/",
        );
    }

    /// A tag is never invented: `ubuntu` stays untagged, because the daemon
    /// applies its own default and a pinned tag would change the pull.
    #[test]
    fn no_default_tag_is_added() {
        assert!(!clean_image("ubuntu").contains(":"));
        assert_eq!(clean_image("ubuntu:latest"), "docker.io/library/ubuntu:latest");
    }

    /// A digest survives normalisation, so a workflow pinned by digest pulls
    /// exactly that image.
    #[test]
    fn a_digest_is_preserved() {
        let full = format!("sha256:{}", "0".repeat(64));
        let cleaned = clean_image(&format!("alpine@{full}"));
        assert_eq!(cleaned, format!("docker.io/library/alpine@{full}"));
    }

    /// A bare sha256 content identifier is a digest, not a name.
    ///
    /// Captured from `reference.ParseAnyReference` under Go 1.26, which is what
    /// fixes the exact output: the digest and **nothing else**, with no leading
    /// `@`. A struct that carried an empty name alongside a digest would render
    /// `@sha256:…`, so upstream's two concrete types are two variants here.
    #[test]
    fn a_bare_sha256_identifier_is_a_digest() {
        let hex = "a".repeat(64);
        assert_eq!(clean_image(&hex), format!("sha256:{hex}"));
        // An explicit digest normalises to the same thing.
        assert_eq!(clean_image(&format!("sha256:{hex}")), format!("sha256:{hex}"));
        // And it is rejected as a *name*, which is the other half of upstream's
        // rule.
        assert!(parse_normalized_named(&hex).is_none());
    }

    /// The separators the name grammar allows inside one component.
    #[test]
    fn the_allowed_name_separators_are_accepted() {
        for image in [
            "my.image",
            "my_image",
            "my__image",
            "my--image",
            "my-image",
        ] {
            let cleaned = clean_image(&format!("owner/{image}"));
            assert!(
                cleaned.contains(image),
                "{image} should survive, got {cleaned}",
            );
        }
        // A leading or trailing separator is not.
        assert_eq!(clean_image("owner/-image"), "");
        assert_eq!(clean_image("owner/image-"), "");
    }
}
