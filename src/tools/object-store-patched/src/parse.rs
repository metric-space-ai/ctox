// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use crate::ObjectStore;
#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
use crate::local::LocalFileSystem;
use crate::memory::InMemory;
use crate::path::Path;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unable to recognise URL \"{}\"", url)]
    Unrecognised { url: Url },

    #[error(transparent)]
    Path {
        #[from]
        source: crate::path::Error,
    },
}

impl From<Error> for super::Error {
    fn from(e: Error) -> Self {
        Self::Generic {
            store: "URL",
            source: Box::new(e),
        }
    }
}

/// Recognizes various URL formats, identifying the relevant [`ObjectStore`]
///
/// See [`ObjectStoreScheme::parse`] for more details
///
/// # Supported formats:
/// - `file:///path/to/my/file` -> [`LocalFileSystem`]
/// - `memory:///` -> [`InMemory`]
/// - `s3://bucket/path` -> [`AmazonS3`](crate::aws::AmazonS3) (also supports `s3a`)
/// - `gs://bucket/path` -> [`GoogleCloudStorage`](crate::gcp::GoogleCloudStorage)
/// - `[az|abfs[s]]://container[@<account>.<host>]/path` -> [`MicrosoftAzure`](crate::azure::MicrosoftAzure)
/// - `http://mydomain/path` -> [`HttpStore`](crate::http::HttpStore)
/// - `https://mydomain/path` -> [`HttpStore`](crate::http::HttpStore)
///
/// There are also special cases for AWS and Azure for `https://{host?}/path` paths:
/// - `dfs.core.windows.net`, `blob.core.windows.net`, `dfs.fabric.microsoft.com`, `blob.fabric.microsoft.com` -> [`MicrosoftAzure`](crate::azure::MicrosoftAzure)
/// - `amazonaws.com` -> [`AmazonS3`](crate::aws::AmazonS3)
/// - `r2.cloudflarestorage.com` -> [`AmazonS3`](crate::aws::AmazonS3)
///
#[non_exhaustive] // permit new variants
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum ObjectStoreScheme {
    /// Url corresponding to [`LocalFileSystem`]
    Local,
    /// Url corresponding to [`InMemory`]
    Memory,
    /// Url corresponding to [`AmazonS3`](crate::aws::AmazonS3)
    AmazonS3,
    /// Url corresponding to [`GoogleCloudStorage`](crate::gcp::GoogleCloudStorage)
    GoogleCloudStorage,
    /// Url corresponding to [`MicrosoftAzure`](crate::azure::MicrosoftAzure)
    MicrosoftAzure,
    /// Url corresponding to [`HttpStore`](crate::http::HttpStore)
    Http,
}

impl ObjectStoreScheme {
    /// Create an [`ObjectStoreScheme`] from the provided [`Url`]
    ///
    /// Returns the [`ObjectStoreScheme`] and the remaining [`Path`]
    ///
    /// # Example
    /// ```
    /// # use url::Url;
    /// # use object_store::ObjectStoreScheme;
    /// let url: Url = "file:///path/to/my/file".parse().unwrap();
    /// let (scheme, path) = ObjectStoreScheme::parse(&url).unwrap();
    /// assert_eq!(scheme, ObjectStoreScheme::Local);
    /// assert_eq!(path.as_ref(), "path/to/my/file");
    ///
    /// let url: Url = "https://blob.core.windows.net/container/path/to/my/file".parse().unwrap();
    /// let (scheme, path) = ObjectStoreScheme::parse(&url).unwrap();
    /// assert_eq!(scheme, ObjectStoreScheme::MicrosoftAzure);
    /// assert_eq!(path.as_ref(), "path/to/my/file");
    ///
    /// let url: Url = "https://example.com/path/to/my/file".parse().unwrap();
    /// let (scheme, path) = ObjectStoreScheme::parse(&url).unwrap();
    /// assert_eq!(scheme, ObjectStoreScheme::Http);
    /// assert_eq!(path.as_ref(), "path/to/my/file");
    /// ```
    pub fn parse(url: &Url) -> Result<(Self, Path), Error> {
        let strip_bucket = || Some(url.path().strip_prefix('/')?.split_once('/')?.1);

        let (scheme, path) = match (url.scheme(), url.host_str()) {
            ("file", None) => (Self::Local, url.path()),
            ("memory", None) => (Self::Memory, url.path()),
            ("s3" | "s3a", Some(_)) => (Self::AmazonS3, url.path()),
            ("gs", Some(_)) => (Self::GoogleCloudStorage, url.path()),
            ("az" | "adl" | "azure" | "abfs" | "abfss", Some(_)) => {
                (Self::MicrosoftAzure, url.path())
            }
            ("http", Some(_)) => (Self::Http, url.path()),
            ("https", Some(host)) => {
                if host.ends_with("dfs.core.windows.net")
                    || host.ends_with("blob.core.windows.net")
                    || host.ends_with("dfs.fabric.microsoft.com")
                    || host.ends_with("blob.fabric.microsoft.com")
                {
                    (Self::MicrosoftAzure, strip_bucket().unwrap_or_default())
                } else if host.ends_with("amazonaws.com") {
                    match host.starts_with("s3") {
                        true => (Self::AmazonS3, strip_bucket().unwrap_or_default()),
                        false => (Self::AmazonS3, url.path()),
                    }
                } else if host.ends_with("r2.cloudflarestorage.com") {
                    (Self::AmazonS3, strip_bucket().unwrap_or_default())
                } else {
                    (Self::Http, url.path())
                }
            }
            _ => return Err(Error::Unrecognised { url: url.clone() }),
        };

        Ok((scheme, Path::from_url_path(path)?))
    }
}

#[cfg(feature = "cloud")]
macro_rules! builder_opts {
    ($builder:ty, $url:expr, $options:expr) => {{
        let builder = $options.into_iter().fold(
            <$builder>::new().with_url($url.to_string()),
            |builder, (key, value)| match key.as_ref().to_ascii_lowercase().parse() {
                Ok(k) => builder.with_config(k, value),
                Err(_) => builder,
            },
        );
        Box::new(builder.build()?) as _
    }};
}

/// Create an [`ObjectStore`] based on the provided `url`
///
/// Returns
/// - An [`ObjectStore`] of the corresponding type
/// - The [`Path`] into the [`ObjectStore`] of the addressed resource
pub fn parse_url(url: &Url) -> Result<(Box<dyn ObjectStore>, Path), super::Error> {
    parse_url_opts(url, std::iter::empty::<(&str, &str)>())
}

/// Create an [`ObjectStore`] based on the provided `url` and options
///
/// This method can be used to create an instance of one of the provided
/// `ObjectStore` implementations based on the URL scheme (see
/// [`ObjectStoreScheme`] for more details).
///
/// For example
/// * `file:///path/to/my/file` will return a [`LocalFileSystem`] instance
/// * `s3://bucket/path` will return an [`AmazonS3`] instance if the `aws` feature is enabled.
///
/// Arguments:
/// * `url`: The URL to parse
/// * `options`: A list of key-value pairs to pass to the [`ObjectStore`] builder.
///   Note different object stores accept different configuration options, so
///   the options that are read depends on the `url` value. One common pattern
///   is to pass configuration information via process variables using [`std::env::vars`].
///
/// Returns
/// - An [`ObjectStore`] of the corresponding type
/// - The [`Path`] into the [`ObjectStore`] of the addressed resource
///
/// [`AmazonS3`]: https://docs.rs/object_store/0.12.0/object_store/aws/struct.AmazonS3.html
pub fn parse_url_opts<I, K, V>(
    url: &Url,
    options: I,
) -> Result<(Box<dyn ObjectStore>, Path), super::Error>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: Into<String>,
{
    let _options = options;
    let (scheme, path) = ObjectStoreScheme::parse(url)?;
    let path = Path::parse(path)?;

    let store = match scheme {
        #[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
        ObjectStoreScheme::Local => Box::new(LocalFileSystem::new()) as _,
        ObjectStoreScheme::Memory => Box::new(InMemory::new()) as _,
        #[cfg(feature = "aws")]
        ObjectStoreScheme::AmazonS3 => {
            builder_opts!(crate::aws::AmazonS3Builder, url, _options)
        }
        #[cfg(feature = "gcp")]
        ObjectStoreScheme::GoogleCloudStorage => {
            builder_opts!(crate::gcp::GoogleCloudStorageBuilder, url, _options)
        }
        #[cfg(feature = "azure")]
        ObjectStoreScheme::MicrosoftAzure => {
            builder_opts!(crate::azure::MicrosoftAzureBuilder, url, _options)
        }
        #[cfg(feature = "http")]
        ObjectStoreScheme::Http => {
            let url = &url[..url::Position::BeforePath];
            builder_opts!(crate::http::HttpBuilder, url, _options)
        }
        #[cfg(not(all(
            feature = "fs",
            feature = "aws",
            feature = "azure",
            feature = "gcp",
            feature = "http",
            not(target_arch = "wasm32")
        )))]
        s => {
            return Err(super::Error::Generic {
                store: "parse_url",
                source: format!("feature for {s:?} not enabled").into(),
            });
        }
    };

    Ok((store, path))
}

#[cfg(test)]
mod tests {
    use super::*;

}
