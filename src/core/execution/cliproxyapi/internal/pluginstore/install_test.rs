// ref: internal/pluginstore/install_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};

use crate::sdk::pluginstore::{
    Artifact, Client, InstallOptions, InstallPlan, Plugin, INSTALL_TYPE_DIRECT,
};

use super::auth::UrlPolicy;
use super::github::{HttpRequest, HttpResponse, PluginStoreTransport, SafePluginStoreIo};
use super::install::install_archive;

struct LoopbackTransport;

impl PluginStoreTransport for LoopbackTransport {
    fn get(&self, request: &HttpRequest) -> std::result::Result<HttpResponse, String> {
        let host = request.url.host_str().ok_or("missing host")?;
        let port = request.url.port_or_known_default().ok_or("missing port")?;
        let mut stream = TcpStream::connect((host, port)).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .map_err(|error| error.to_string())?;
        write!(
            stream,
            "GET {} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n",
            request.url.path()
        )
        .map_err(|error| error.to_string())?;
        for (name, value) in &request.headers {
            write!(stream, "{name}: {value}\r\n").map_err(|error| error.to_string())?;
        }
        stream
            .write_all(b"\r\n")
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            if bytes.len() == 64 * 1024 {
                return Err("response head too large".to_owned());
            }
            let mut byte = [0_u8; 1];
            stream
                .read_exact(&mut byte)
                .map_err(|error| error.to_string())?;
            bytes.push(byte[0]);
        }
        let head =
            std::str::from_utf8(&bytes[..bytes.len() - 4]).map_err(|error| error.to_string())?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .ok_or("invalid status")?;
        let mut headers = BTreeMap::new();
        for line in head.lines().skip(1) {
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }
        Ok(HttpResponse {
            status,
            headers,
            body: Box::new(stream),
        })
    }
}

fn plugin_zip(name: &str, data: &[u8]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    writer
        .start_file(
            name,
            zip::write::FileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
    writer.write_all(data).unwrap();
    writer.finish().unwrap().into_inner()
}
