use super::{ConfigString, ServerDefinition};
use std::str::FromStr;

pub(crate) struct AdoNetConfig {
    dict: connection_string::AdoNetString,
}

impl FromStr for AdoNetConfig {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        let dict = s.parse()?;
        Ok(Self { dict })
    }
}

impl ConfigString for AdoNetConfig {
    fn dict(&self) -> &std::collections::HashMap<String, String> {
        &self.dict
    }

    fn server(&self) -> crate::Result<ServerDefinition> {
        fn parse_port(parts: &[&str]) -> crate::Result<Option<u16>> {
            Ok(match parts.first() {
                Some(s) => Some(s.parse()?),
                None => None,
            })
        }

        fn parse_server(parts: Vec<&str>) -> crate::Result<ServerDefinition> {
            if parts.is_empty() || parts.len() >= 3 {
                return Err(crate::Error::Conversion("Server value faulty.".into()));
            }

            let definition = if parts[0].contains('\\') {
                let port = parse_port(&parts[1..])?;
                let parts: Vec<&str> = parts[0].split('\\').collect();

                ServerDefinition {
                    host: Some(parts[0].replace("(local)", "localhost")),
                    port,
                    instance: Some(parts[1].into()),
                }
            } else {
                // Connect using a TCP target
                ServerDefinition {
                    host: Some(parts[0].replace("(local)", "localhost")),
                    port: parse_port(&parts[1..])?,
                    instance: None,
                }
            };

            Ok(definition)
        }

        match self
            .dict
            .get("server")
            .or_else(|| self.dict.get("data source"))
        {
            Some(value) if value.starts_with("tcp:") => {
                parse_server(value[4..].split(',').collect())
            }
            Some(value) => parse_server(value.split(',').collect()),
            None => Ok(ServerDefinition {
                host: None,
                port: None,
                instance: None,
            }),
        }
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
