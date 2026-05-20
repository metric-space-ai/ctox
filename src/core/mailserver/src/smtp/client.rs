// ref: stalwart/src/smtp/client/mod.rs:1-40
use crate::util::errors::{StalwartError, StalwartResult};
use crate::smtp::dkim::DkimSigner;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct SmtpOutboundClient {
    addr: SocketAddr,
    stream: Option<TcpStream>,
}

impl SmtpOutboundClient {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr, stream: None }
    }

    // ref: stalwart/src/smtp/client/mod.rs:45-80
    pub async fn connect(&mut self) -> StalwartResult<()> {
        let stream = TcpStream::connect(self.addr).await?;
        self.stream = Some(stream);
        self.read_response(220).await?;
        Ok(())
    }

    // ref: stalwart/src/smtp/client/mod.rs:85-120
    pub async fn send_ehlo(&mut self, domain: &str) -> StalwartResult<()> {
        self.send_command(&format!("EHLO {}\r\n", domain)).await?;
        self.read_response(250).await?;
        Ok(())
    }

    // ref: stalwart/src/smtp/client/mod.rs:125-180
    pub async fn send_mail(
        &mut self,
        from: &str,
        to: &[String],
        body: &str,
        dkim: Option<&DkimSigner>,
    ) -> StalwartResult<()> {
        self.send_command(&format!("MAIL FROM:<{}>\r\n", from)).await?;
        self.read_response(250).await?;

        for recipient in to {
            self.send_command(&format!("RCPT TO:<{}>\r\n", recipient)).await?;
            self.read_response(250).await?;
        }

        self.send_command("DATA\r\n").await?;
        self.read_response(354).await?;

        // Apply DKIM signing if present
        let signed_body = if let Some(signer) = dkim {
            signer.sign(from, body)?
        } else {
            body.to_string()
        };

        self.send_command(&format!("{}{}\r\n.\r\n", signed_body, if signed_body.ends_with("\r\n") { "" } else { "\r\n" })).await?;
        self.read_response(250).await?;

        Ok(())
    }

    // ref: stalwart/src/smtp/client/mod.rs:185-210
    pub async fn quit(&mut self) -> StalwartResult<()> {
        if self.stream.is_some() {
            self.send_command("QUIT\r\n").await?;
            let _ = self.read_response(221).await;
            self.stream = None;
        }
        Ok(())
    }

    pub async fn reset(&mut self) -> StalwartResult<()> {
        self.send_command("RSET\r\n").await?;
        self.read_response(250).await?;
        Ok(())
    }


    async fn send_command(&mut self, cmd: &str) -> StalwartResult<()> {
        if let Some(ref mut stream) = self.stream {
            stream.write_all(cmd.as_bytes()).await?;
            stream.flush().await?;
            Ok(())
        } else {
            Err(StalwartError::General("Not connected".to_string()))
        }
    }

    async fn read_response(&mut self, expected_code: u16) -> StalwartResult<String> {
        if let Some(ref mut stream) = self.stream {
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await?;
            let resp = String::from_utf8_lossy(&buf[..n]).to_string();
            let clean_resp = resp.trim();
            let code = if clean_resp.len() >= 3 {
                clean_resp[..3].parse::<u16>().unwrap_or(0)
            } else {
                0
            };
            if code == expected_code || (expected_code == 250 && code == 200) {
                Ok(resp)
            } else {
                Err(StalwartError::Smtp {
                    code,
                    message: resp,
                })
            }
        } else {
            Err(StalwartError::General("Not connected".to_string()))
        }
    }
}
