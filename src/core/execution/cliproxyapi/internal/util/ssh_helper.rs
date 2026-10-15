// ref: internal/util/ssh_helper.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use url::Url;

pub const DEFAULT_IP_SERVICES: [&str; 4] = [
    "https://api.ipify.org",
    "https://ifconfig.me/ip",
    "https://icanhazip.com",
    "https://ipinfo.io/ip",
];

pub trait IpProbe: Send + Sync {
    fn fetch(&self, endpoint: &Url, timeout: Duration) -> Result<Vec<u8>, IpProbeError>;
    fn outbound_ip(&self) -> Result<IpAddr, IpProbeError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpDiscoveryPolicy {
    endpoints: Vec<Url>,
    timeout: Duration,
}

impl IpDiscoveryPolicy {
    pub fn new(
        endpoints: impl IntoIterator<Item = String>,
        timeout: Duration,
    ) -> Result<Self, SshHelperError> {
        if timeout.is_zero() {
            return Err(SshHelperError::InvalidTimeout);
        }
        let endpoints = endpoints
            .into_iter()
            .map(|raw| validate_endpoint(&raw))
            .collect::<Result<Vec<_>, _>>()?;
        if endpoints.is_empty() {
            return Err(SshHelperError::NoEndpoints);
        }
        Ok(Self { endpoints, timeout })
    }

#[must_use]
    pub fn endpoints(&self) -> &[Url] {
        &self.endpoints
    }
}

impl Default for IpDiscoveryPolicy {
    fn default() -> Self {
        Self::new(
            DEFAULT_IP_SERVICES.into_iter().map(str::to_owned),
            Duration::from_secs(2),
        )
        .expect("static IP service policy must be valid")
    }
}

fn validate_endpoint(raw: &str) -> Result<Url, SshHelperError> {
    let url = Url::parse(raw).map_err(|_| SshHelperError::InvalidEndpoint)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(SshHelperError::InvalidEndpoint);
    }
    Ok(url)
}

#[must_use]
pub fn get_ip_address(probe: &dyn IpProbe, policy: &IpDiscoveryPolicy) -> IpAddr {
    for endpoint in &policy.endpoints {
        if let Ok(body) = probe.fetch(endpoint, policy.timeout) {
            if let Ok(text) = std::str::from_utf8(&body) {
                if let Ok(ip) = text.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    probe
        .outbound_ip()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshCommandPlan {
    program: PathBuf,
    args: Vec<OsString>,
}

impl SshCommandPlan {
#[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

#[must_use]
    pub fn args(&self) -> &[OsString] {
        &self.args
    }

    /// Human-readable rendering only. Execution must pass `program` and
    /// `args` separately to an injected process runner.
#[must_use]
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Host-owned process authority. The helper only constructs an argv plan and
/// never locates or starts `ssh` itself.
pub trait SshProcessRunner: Send + Sync {
    type Output;
    fn run(&self, plan: &SshCommandPlan) -> Result<Self::Output, SshHelperError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshTunnelSpec {
    ssh_program: PathBuf,
    identity_file: Option<PathBuf>,
    user: String,
    host: IpAddr,
    ssh_port: u16,
    callback_port: u16,
}

impl SshTunnelSpec {
    pub fn new(
        ssh_program: impl Into<PathBuf>,
        identity_file: Option<PathBuf>,
        user: impl Into<String>,
        host: IpAddr,
        ssh_port: u16,
        callback_port: u16,
    ) -> Result<Self, SshHelperError> {
        let ssh_program = ssh_program.into();
        let user = user.into();
        if !ssh_program.is_absolute() || !safe_path(&ssh_program) {
            return Err(SshHelperError::InvalidProgramPath);
        }
        if identity_file
            .as_deref()
            .is_some_and(|path| !path.is_absolute() || !safe_path(path))
        {
            return Err(SshHelperError::InvalidIdentityPath);
        }
        if user.is_empty()
            || !user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(SshHelperError::InvalidUser);
        }
        if ssh_port == 0 || callback_port == 0 {
            return Err(SshHelperError::InvalidPort);
        }
        Ok(Self {
            ssh_program,
            identity_file,
            user,
            host,
            ssh_port,
            callback_port,
        })
    }

#[must_use]
    pub fn command_plan(&self) -> SshCommandPlan {
        let mut args = Vec::new();
        if let Some(identity) = &self.identity_file {
            args.push(OsString::from("-i"));
            args.push(identity.as_os_str().to_owned());
        }
        args.extend([
            OsString::from("-L"),
            OsString::from(format!(
                "{}:127.0.0.1:{}",
                self.callback_port, self.callback_port
            )),
            OsString::from(format!("{}@{}", self.user, self.host)),
            OsString::from("-p"),
            OsString::from(self.ssh_port.to_string()),
        ]);
        SshCommandPlan {
            program: self.ssh_program.clone(),
            args,
        }
    }
}

fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path
            .as_os_str()
            .to_string_lossy()
            .chars()
            .any(char::is_control)
}

fn shell_quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/_:.,@%+=-".contains(&byte))
    {
        return value.into_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpProbeError;

impl fmt::Display for IpProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IP probe failed")
    }
}

impl std::error::Error for IpProbeError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshHelperError {
    InvalidEndpoint,
    InvalidIdentityPath,
    InvalidPort,
    InvalidProgramPath,
    InvalidTimeout,
    InvalidUser,
    NoEndpoints,
    Process,
}

impl fmt::Display for SshHelperError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEndpoint => "invalid credential-free HTTPS IP endpoint",
            Self::InvalidIdentityPath => "invalid absolute SSH identity path",
            Self::InvalidPort => "invalid SSH tunnel port",
            Self::InvalidProgramPath => "invalid absolute SSH program path",
            Self::InvalidTimeout => "invalid IP probe timeout",
            Self::InvalidUser => "invalid SSH user",
            Self::NoEndpoints => "IP discovery has no endpoints",
            Self::Process => "SSH process failed",
        })
    }
}

impl std::error::Error for SshHelperError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Probe {
        responses: Mutex<Vec<Result<Vec<u8>, IpProbeError>>>,
        outbound: Result<IpAddr, IpProbeError>,
    }

    impl IpProbe for Probe {
        fn fetch(&self, _: &Url, _: Duration) -> Result<Vec<u8>, IpProbeError> {
            self.responses.lock().unwrap().remove(0)
        }

        fn outbound_ip(&self) -> Result<IpAddr, IpProbeError> {
            self.outbound
        }
    }

}
