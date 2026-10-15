use super::{ConfigString, ServerDefinition};
use crate::error::Error;
use connection_string::JdbcString;
use std::str::FromStr;

pub(crate) struct JdbcConfig {
    config: JdbcString,
}

impl FromStr for JdbcConfig {
    type Err = Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        let config = s.parse()?;
        Ok(Self { config })
    }
}

impl ConfigString for JdbcConfig {
    fn dict(&self) -> &std::collections::HashMap<String, String> {
        self.config.properties()
    }

    fn server(&self) -> crate::Result<ServerDefinition> {
        let def = ServerDefinition {
            host: self.config.server_name().map(|s| s.to_string()),
            port: self.config.port(),
            instance: self.config.instance_name().map(|s| s.to_string()),
        };

        Ok(def)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::AuthMethod;

    #[cfg(any(
        feature = "rustls",
        feature = "native-tls",
        feature = "vendored-openssl"
    ))]
    use crate::EncryptionLevel;
}
