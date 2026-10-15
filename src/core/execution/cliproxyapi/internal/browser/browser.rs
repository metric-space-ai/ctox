// ref: internal/browser/browser.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Cross-platform browser launching for interactive authentication flows.
//!
//! CTOX deliberately makes availability checks side-effect free. The Go
//! implementation calls `open.Run("about:blank")` from `IsAvailable`, which
//! can open a browser merely because a caller requested platform metadata.

use serde::Serialize;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

const LINUX_BROWSER_COMMANDS: &[&str] = &[
    "xdg-open",
    "x-www-browser",
    "www-browser",
    "firefox",
    "chromium",
    "google-chrome",
];

/// Stable, serializable equivalent of upstream's `map[string]interface{}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_command: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub available_browsers: Vec<String>,
}

#[derive(Debug)]
pub enum BrowserError {
    UnsupportedOperatingSystem(String),
    NoSuitableBrowser(String),
    Start { command: String, source: io::Error },
}

impl fmt::Display for BrowserError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperatingSystem(os) => {
                write!(formatter, "unsupported operating system: {os}")
            }
            Self::NoSuitableBrowser(os) => {
                write!(formatter, "no suitable browser found on {os} system")
            }
            Self::Start { command, source } => {
                write!(
                    formatter,
                    "failed to start browser command {command}: {source}"
                )
            }
        }
    }
}

impl std::error::Error for BrowserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Start { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Host-owned browser capability. Implementations decide which launchers are
/// permitted and perform the process start; this module never reads `PATH` or
/// starts a process on its own.
pub trait BrowserAuthority: Send + Sync {
    fn operating_system(&self) -> &str;
    fn architecture(&self) -> &str;
    fn find_command(&self, command: &str) -> Option<PathBuf>;
    fn start(&self, command: &Path, arguments: &[OsString]) -> io::Result<()>;
}

/// Opens `url` with the platform's first available browser launcher.
///
/// The URL is always passed as one process argument and is never interpreted
/// by a shell.
pub fn open_url(system: &dyn BrowserAuthority, url: &str) -> Result<(), BrowserError> {
    let (command, prefix_arguments) = selected_command(system)?;
    let mut arguments = prefix_arguments;
    arguments.push(OsString::from(url));
    system
        .start(&command, &arguments)
        .map_err(|source| BrowserError::Start {
            command: command.display().to_string(),
            source,
        })
}

fn selected_command(
    system: &dyn BrowserAuthority,
) -> Result<(PathBuf, Vec<OsString>), BrowserError> {
    match system.operating_system() {
        "macos" => system
            .find_command("open")
            .map(|command| (command, Vec::new()))
            .ok_or_else(|| BrowserError::NoSuitableBrowser("macOS".to_owned())),
        "windows" => system
            .find_command("rundll32")
            .map(|command| (command, vec![OsString::from("url.dll,FileProtocolHandler")]))
            .ok_or_else(|| BrowserError::NoSuitableBrowser("Windows".to_owned())),
        "linux" => LINUX_BROWSER_COMMANDS
            .iter()
            .find_map(|candidate| {
                system
                    .find_command(candidate)
                    .map(|command| (command, Vec::new()))
            })
            .ok_or_else(|| BrowserError::NoSuitableBrowser("Linux".to_owned())),
        os => Err(BrowserError::UnsupportedOperatingSystem(os.to_owned())),
    }
}

/// Reports whether the injected host authority has a browser launcher.
/// This function never starts a browser.
pub fn is_available(system: &dyn BrowserAuthority) -> bool {
    selected_command(system).is_ok()
}

/// Returns upstream-compatible capability fields using only injected host
/// observations.
pub fn get_platform_info(system: &dyn BrowserAuthority) -> PlatformInfo {
    platform_info_with(system)
}

fn platform_info_with(system: &dyn BrowserAuthority) -> PlatformInfo {
    let available_browsers = if system.operating_system() == "linux" {
        LINUX_BROWSER_COMMANDS
            .iter()
            .filter(|candidate| system.find_command(candidate).is_some())
            .map(|candidate| (*candidate).to_owned())
            .collect()
    } else {
        Vec::new()
    };

    let default_command = match system.operating_system() {
        "macos" if system.find_command("open").is_some() => Some("open".to_owned()),
        "windows" if system.find_command("rundll32").is_some() => Some("rundll32".to_owned()),
        "linux" => available_browsers.first().cloned(),
        _ => None,
    };

    PlatformInfo {
        os: reported_operating_system(system.operating_system()).to_owned(),
        arch: system.architecture().to_owned(),
        available: default_command.is_some(),
        default_command,
        available_browsers,
    }
}

fn reported_operating_system(os: &str) -> &str {
    // Go reports macOS as `darwin`; retain that public compatibility value.
    if os == "macos" {
        "darwin"
    } else {
        os
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    struct FakeSystem {
        os: &'static str,
        arch: &'static str,
        commands: BTreeSet<&'static str>,
        starts: Mutex<Vec<(PathBuf, Vec<OsString>)>>,
        fail_start: bool,
    }

    impl FakeSystem {
        fn new(os: &'static str, commands: &[&'static str]) -> Self {
            Self {
                os,
                arch: "test-arch",
                commands: commands.iter().copied().collect(),
                starts: Mutex::new(Vec::new()),
                fail_start: false,
            }
        }
    }

    impl BrowserAuthority for FakeSystem {
        fn operating_system(&self) -> &str {
            self.os
        }

        fn architecture(&self) -> &str {
            self.arch
        }

        fn find_command(&self, command: &str) -> Option<PathBuf> {
            self.commands
                .contains(command)
                .then(|| PathBuf::from(format!("/commands/{command}")))
        }

        fn start(&self, command: &Path, arguments: &[OsString]) -> io::Result<()> {
            if self.fail_start {
                return Err(io::Error::other("injected failure"));
            }
            self.starts
                .lock()
                .unwrap()
                .push((command.to_owned(), arguments.to_vec()));
            Ok(())
        }
    }

#[test]
    fn linux_uses_first_available_launcher_and_keeps_url_one_argument() {
        let system = FakeSystem::new("linux", &["firefox", "google-chrome"]);
        let url = "https://example.test/oauth?code=a&state=b;touch /tmp/nope";

        open_url(&system, url).unwrap();

        assert_eq!(
            *system.starts.lock().unwrap(),
            vec![(
                PathBuf::from("/commands/firefox"),
                vec![OsString::from(url)]
            )]
        );
    }

#[test]
    fn windows_includes_file_protocol_handler_before_url() {
        let system = FakeSystem::new("windows", &["rundll32"]);

        open_url(&system, "https://example.test").unwrap();

        assert_eq!(
            system.starts.lock().unwrap()[0].1,
            vec![
                OsString::from("url.dll,FileProtocolHandler"),
                OsString::from("https://example.test")
            ]
        );
    }

#[test]
    fn platform_info_is_side_effect_free_and_preserves_linux_order() {
        let system = FakeSystem::new("linux", &["google-chrome", "xdg-open"]);

        let info = platform_info_with(&system);

        assert_eq!(info.os, "linux");
        assert_eq!(info.arch, "test-arch");
        assert!(info.available);
        assert_eq!(info.default_command.as_deref(), Some("xdg-open"));
        assert_eq!(info.available_browsers, ["xdg-open", "google-chrome"]);
        assert!(system.starts.lock().unwrap().is_empty());
    }

#[test]
    fn unsupported_and_missing_platforms_fail_explicitly() {
        let unsupported = FakeSystem::new("plan9", &[]);
        let linux = FakeSystem::new("linux", &[]);

        assert!(matches!(
            open_url(&unsupported, "https://example.test"),
            Err(BrowserError::UnsupportedOperatingSystem(os)) if os == "plan9"
        ));
        assert!(matches!(
            open_url(&linux, "https://example.test"),
            Err(BrowserError::NoSuitableBrowser(os)) if os == "Linux"
        ));
        assert!(!platform_info_with(&linux).available);
    }

#[test]
    fn availability_uses_injected_authority_without_starting_a_process() {
        let unavailable = FakeSystem::new("linux", &[]);
        let available = FakeSystem::new("linux", &["firefox"]);

        assert!(!is_available(&unavailable));
        assert!(is_available(&available));
        assert!(available.starts.lock().unwrap().is_empty());
        assert_eq!(
            get_platform_info(&available).default_command.as_deref(),
            Some("firefox")
        );
    }
}
