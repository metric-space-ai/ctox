// ref: stalwart/src/smtp/server/mod.rs:1-300
// ref: ctox-mailserver SQLite-backed native SMTP inbound listener for bounce processing

use crate::store::SqliteStore;
use crate::config::SmtpConfig;
use crate::smtp::dsn::parse_dsn_report;
use crate::util::errors::StalwartResult;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, error, warn};
use base64::Engine;

pub struct SmtpInboundServer {
    store: SqliteStore,
    config: SmtpConfig,
}

impl SmtpInboundServer {
    pub fn new(store: SqliteStore, config: SmtpConfig) -> Self {
        Self { store, config }
    }

    pub async fn start(self: Arc<Self>) -> StalwartResult<()> {
        let listener = TcpListener::bind(self.config.bind_address).await?;
        info!("SMTP Inbound Server running on {}", self.config.bind_address);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Inbound SMTP connection from {}", addr);
                    let self_clone = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = self_clone.handle_connection(stream).await {
                            error!("SMTP Connection Error: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("SMTP accept connection failed: {:?}", e);
                }
            }
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream) -> StalwartResult<()> {
        let client_ip = stream.peer_addr().map(|addr| addr.ip().to_string()).unwrap_or_else(|_| "unknown".to_string());
        let mut buf = [0u8; 4096];
        stream.write_all(b"220 localhost ESMTP Stalwart-Ctox Inbound\r\n").await?;

        let mut mail_from = String::new();
        let mut rcpt_to: Vec<String> = Vec::new();
        let mut mail_body = String::new();
        let mut receiving_data = false;
        let mut authenticated_user: Option<String> = None;
        let mut line_buffer = String::new();

        #[derive(Debug, Clone, PartialEq)]
        enum AuthState {
            None,
            Plain,
            LoginUsername,
            LoginPassword { username: String },
        }
        let mut auth_state = AuthState::None;

        loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            line_buffer.push_str(&String::from_utf8_lossy(&buf[..n]));

            while let Some(pos) = line_buffer.find('\n') {
                let mut line = line_buffer[..pos].to_string();
                line_buffer = line_buffer[pos + 1..].to_string();

                if line.ends_with('\r') {
                    line.pop();
                }

                // If receiving mail body data
                if receiving_data {
                    if line == "." {
                        receiving_data = false;
                        
                        // Parse email body with mailparse
                        let parsed = mailparse::parse_mail(mail_body.as_bytes());
                        let subject = parsed.as_ref().ok().and_then(|m| {
                            m.headers.iter()
                                .find(|h| h.get_key().to_ascii_lowercase() == "subject")
                                .map(|h| h.get_value())
                        });
                        let headers_str = parsed.as_ref().ok().map(|m| {
                            let mut s = String::new();
                            for h in &m.headers {
                                s.push_str(&format!("{}: {}\r\n", h.get_key(), h.get_value()));
                            }
                            s
                        });
                        let mut extracted_body = parsed.as_ref().ok()
                            .map(|m| extract_text_body(m))
                            .unwrap_or_default();
                        if extracted_body.is_empty() {
                            extracted_body = mail_body.clone();
                        }

                        // Deliver mail
                        let mut delivered = false;
                        for recipient in &rcpt_to {
                            // Extract raw email address (e.g. from <user@domain.com> to user@domain.com)
                            let clean_recip = recipient.trim_matches(|c| c == '<' || c == '>').trim().to_string();
                            if self.store.user_exists(&clean_recip)? {
                                if let Some(inbox_id) = self.store.get_mailbox_id(&clean_recip, "INBOX")? {
                                    let clean_from = mail_from.trim_matches(|c| c == '<' || c == '>').trim();
                                    self.store.put_message(
                                        &inbox_id,
                                        clean_from,
                                        &clean_recip,
                                        subject.as_deref(),
                                        &extracted_body,
                                        headers_str.as_deref(),
                                    )?;
                                    delivered = true;
                                }
                            }
                        }

                        // Also check for bounce processing
                        if let Some(dsn) = parse_dsn_report(&mail_body) {
                            warn!("Parsed bounce report: Recipient = {}, Status = {}, Hard = {}", dsn.recipient, dsn.status_code, dsn.is_hard_bounce);
                            let _ = self.store.queue_email(
                                "bounce-handler@localhost",
                                "admin@localhost",
                                &format!("Subject: Bounce report for {}\r\n\r\nRecipient {} bounced with status {}. Hard bounce: {}", dsn.recipient, dsn.recipient, dsn.status_code, dsn.is_hard_bounce)
                            );
                        }

                        if delivered || !rcpt_to.is_empty() {
                            stream.write_all(b"250 2.0.0 Ok: Message accepted for delivery\r\n").await?;
                        } else {
                            // If no valid recipients and not a bounce
                            stream.write_all(b"550 5.1.1 User unknown\r\n").await?;
                        }

                        mail_body.clear();
                    } else {
                        mail_body.push_str(&line);
                        mail_body.push_str("\r\n");
                    }
                    continue;
                }

                // If in an authentication flow
                match auth_state {
                    AuthState::Plain => {
                        auth_state = AuthState::None;
                        if let Ok(decoded) = base64::prelude::BASE64_STANDARD.decode(line.trim().as_bytes()) {
                            let parts: Vec<&[u8]> = decoded.split(|&b| b == 0).collect();
                            if parts.len() >= 3 {
                                let username = String::from_utf8_lossy(parts[1]).into_owned();
                                let password = String::from_utf8_lossy(parts[2]).into_owned();
                                if self.store.authenticate_user(&username, &password)? {
                                    authenticated_user = Some(username);
                                    stream.write_all(b"235 2.7.0 Authentication successful\r\n").await?;
                                } else {
                                    stream.write_all(b"535 5.7.8 Authentication failed\r\n").await?;
                                }
                            } else {
                                stream.write_all(b"501 5.5.4 Invalid AUTH PLAIN parameters\r\n").await?;
                            }
                        } else {
                            stream.write_all(b"501 5.5.4 Invalid Base64 data\r\n").await?;
                        }
                        continue;
                    }
                    AuthState::LoginUsername => {
                        if let Ok(decoded) = base64::prelude::BASE64_STANDARD.decode(line.trim().as_bytes()) {
                            let username = String::from_utf8_lossy(&decoded).into_owned();
                            auth_state = AuthState::LoginPassword { username };
                            // Respond with "Password:" in base64
                            stream.write_all(b"334 UGFzc3dvcmQ6\r\n").await?;
                        } else {
                            auth_state = AuthState::None;
                            stream.write_all(b"501 5.5.4 Invalid Base64 data\r\n").await?;
                        }
                        continue;
                    }
                    AuthState::LoginPassword { ref username } => {
                        let user = username.clone();
                        auth_state = AuthState::None;
                        if let Ok(decoded) = base64::prelude::BASE64_STANDARD.decode(line.trim().as_bytes()) {
                            let password = String::from_utf8_lossy(&decoded).into_owned();
                            if self.store.authenticate_user(&user, &password)? {
                                authenticated_user = Some(user);
                                stream.write_all(b"235 2.7.0 Authentication successful\r\n").await?;
                            } else {
                                stream.write_all(b"535 5.7.8 Authentication failed\r\n").await?;
                            }
                        } else {
                            stream.write_all(b"501 5.5.4 Invalid Base64 data\r\n").await?;
                        }
                        continue;
                    }
                    AuthState::None => {}
                }

                // Process regular SMTP commands
                let line_upper = line.trim().to_uppercase();
                if line_upper.starts_with("EHLO") || line_upper.starts_with("HELO") {
                    stream.write_all(b"250-localhost Greeting\r\n250-8BITMIME\r\n250-AUTH PLAIN LOGIN\r\n250 HELP\r\n").await?;
                } else if line_upper.starts_with("AUTH PLAIN") {
                    let arg = line["AUTH PLAIN".len()..].trim();
                    if arg.is_empty() {
                        auth_state = AuthState::Plain;
                        stream.write_all(b"334 \r\n").await?;
                    } else {
                        if let Ok(decoded) = base64::prelude::BASE64_STANDARD.decode(arg.as_bytes()) {
                            let parts: Vec<&[u8]> = decoded.split(|&b| b == 0).collect();
                            if parts.len() >= 3 {
                                let username = String::from_utf8_lossy(parts[1]).into_owned();
                                let password = String::from_utf8_lossy(parts[2]).into_owned();
                                if self.store.authenticate_user(&username, &password)? {
                                    authenticated_user = Some(username);
                                    stream.write_all(b"235 2.7.0 Authentication successful\r\n").await?;
                                } else {
                                    stream.write_all(b"535 5.7.8 Authentication failed\r\n").await?;
                                }
                            } else {
                                stream.write_all(b"501 5.5.4 Invalid AUTH PLAIN parameters\r\n").await?;
                            }
                        } else {
                            stream.write_all(b"501 5.5.4 Invalid Base64 data\r\n").await?;
                        }
                    }
                } else if line_upper.starts_with("AUTH LOGIN") {
                    let arg = line["AUTH LOGIN".len()..].trim();
                    if arg.is_empty() {
                        auth_state = AuthState::LoginUsername;
                        // Respond with "Username:" in base64
                        stream.write_all(b"334 VXNlcm5hbWU6\r\n").await?;
                    } else {
                        if let Ok(decoded) = base64::prelude::BASE64_STANDARD.decode(arg.as_bytes()) {
                            let username = String::from_utf8_lossy(&decoded).into_owned();
                            auth_state = AuthState::LoginPassword { username };
                            stream.write_all(b"334 UGFzc3dvcmQ6\r\n").await?;
                        } else {
                            stream.write_all(b"501 5.5.4 Invalid Base64 data\r\n").await?;
                        }
                    }
                } else if line_upper.starts_with("MAIL FROM:") {
                    mail_from = line.replace("MAIL FROM:", "").trim().to_string();
                    stream.write_all(b"250 2.1.0 Ok\r\n").await?;
                } else if line_upper.starts_with("RCPT TO:") {
                    let recipient = line.replace("RCPT TO:", "").trim().to_string();
                    let clean_recip = recipient.trim_matches(|c| c == '<' || c == '>').trim().to_string();
                    
                    // If authenticated, we allow sending to any recipient
                    // If not authenticated, we only accept local registered users
                    if authenticated_user.is_some() || self.store.user_exists(&clean_recip)? {
                        if authenticated_user.is_none() {
                            let clean_from = mail_from.trim_matches(|c| c == '<' || c == '>').trim();
                            if !self.store.check_greylist(&client_ip, clean_from, &clean_recip)? {
                                stream.write_all(b"450 4.2.0 Greylisting active, please try again later\r\n").await?;
                                continue;
                            }
                        }
                        rcpt_to.push(recipient);
                        stream.write_all(b"250 2.1.5 Ok\r\n").await?;
                    } else {
                        stream.write_all(b"550 5.1.1 User unknown\r\n").await?;
                    }
                } else if line_upper == "DATA" {
                    if mail_from.is_empty() || rcpt_to.is_empty() {
                        stream.write_all(b"503 5.5.1 Bad sequence of commands\r\n").await?;
                    } else {
                        receiving_data = true;
                        stream.write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n").await?;
                    }
                } else if line_upper == "RSET" {
                    mail_from.clear();
                    rcpt_to.clear();
                    mail_body.clear();
                    receiving_data = false;
                    auth_state = AuthState::None;
                    stream.write_all(b"250 2.0.0 Ok\r\n").await?;
                } else if line_upper == "QUIT" {
                    stream.write_all(b"221 2.0.0 Bye\r\n").await?;
                    return Ok(());
                } else if !line_upper.is_empty() {
                    stream.write_all(b"502 5.5.1 Command not implemented\r\n").await?;
                }
            }
        }

        Ok(())
    }
}

fn extract_text_body(parsed: &mailparse::ParsedMail<'_>) -> String {
    if parsed.subparts.is_empty() {
        let mimetype = parsed.ctype.mimetype.to_lowercase();
        if mimetype.starts_with("text/") || mimetype.is_empty() {
            parsed.get_body().unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        for part in &parsed.subparts {
            let body = extract_text_body(part);
            if !body.is_empty() {
                return body;
            }
        }
        String::new()
    }
}


