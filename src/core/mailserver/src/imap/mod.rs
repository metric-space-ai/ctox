// ref: stalwart/src/imap/mod.rs:1-300
// ref: ctox-mailserver SQLite-backed native IMAP server

use crate::config::ImapConfig;
use crate::store::SqliteStore;
use crate::util::errors::StalwartResult;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

pub struct ImapServer {
    store: SqliteStore,
    config: ImapConfig,
}

#[derive(Debug, Clone, PartialEq)]
enum ImapState {
    NotAuthenticated,
    Authenticated {
        username: String,
    },
    Selected {
        username: String,
        mailbox_id: String,
        mailbox_name: String,
    },
}

impl ImapServer {
    pub fn new(store: SqliteStore, config: ImapConfig) -> Self {
        Self { store, config }
    }

    pub async fn start(self: Arc<Self>) -> StalwartResult<()> {
        let listener = TcpListener::bind(self.config.bind_address).await?;
        info!("IMAP Server running on {}", self.config.bind_address);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Inbound IMAP connection from {}", addr);
                    let self_clone = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = self_clone.handle_connection(stream).await {
                            error!("IMAP Connection Error: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("IMAP accept connection failed: {:?}", e);
                }
            }
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream) -> StalwartResult<()> {
        let mut buf = [0u8; 4096];
        stream
            .write_all(b"* OK [CAPABILITY IMAP4rev1] IMAP4rev1 Server Ready\r\n")
            .await?;

        let mut state = ImapState::NotAuthenticated;
        let mut line_buffer = String::new();

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

                let tokens = parse_imap_line(&line);
                if tokens.is_empty() {
                    continue;
                }

                let tag = tokens[0].clone();
                if tokens.len() < 2 {
                    stream
                        .write_all(format!("{} BAD Missing command\r\n", tag).as_bytes())
                        .await?;
                    continue;
                }

                let command = tokens[1].to_uppercase();
                let args = &tokens[2..];

                match command.as_str() {
                    "CAPABILITY" => {
                        stream.write_all(b"* CAPABILITY IMAP4rev1\r\n").await?;
                        stream
                            .write_all(format!("{} OK CAPABILITY completed\r\n", tag).as_bytes())
                            .await?;
                    }
                    "NOOP" => {
                        stream
                            .write_all(format!("{} OK NOOP completed\r\n", tag).as_bytes())
                            .await?;
                    }
                    "LOGOUT" => {
                        stream
                            .write_all(b"* BYE IMAP4rev1 Server logging out\r\n")
                            .await?;
                        stream
                            .write_all(format!("{} OK LOGOUT completed\r\n", tag).as_bytes())
                            .await?;
                        return Ok(());
                    }
                    "LOGIN" => {
                        if args.len() < 2 {
                            stream
                                .write_all(
                                    format!("{} BAD Missing username or password\r\n", tag)
                                        .as_bytes(),
                                )
                                .await?;
                            continue;
                        }
                        let username = &args[0];
                        let password = &args[1];
                        if self.store.authenticate_user(username, password)? {
                            state = ImapState::Authenticated {
                                username: username.clone(),
                            };
                            stream
                                .write_all(format!("{} OK LOGIN completed\r\n", tag).as_bytes())
                                .await?;
                        } else {
                            stream
                                .write_all(
                                    format!("{} NO LOGIN failed: bad credentials\r\n", tag)
                                        .as_bytes(),
                                )
                                .await?;
                        }
                    }
                    "LIST" => {
                        if let ImapState::Authenticated { ref username }
                        | ImapState::Selected { ref username, .. } = state
                        {
                            let mailboxes = self.store.get_mailboxes(username)?;
                            for (_, name) in mailboxes {
                                stream
                                    .write_all(
                                        format!("* LIST (\\HasNoChildren) \"/\" \"{}\"\r\n", name)
                                            .as_bytes(),
                                    )
                                    .await?;
                            }
                            stream
                                .write_all(format!("{} OK LIST completed\r\n", tag).as_bytes())
                                .await?;
                        } else {
                            stream
                                .write_all(format!("{} NO Authenticate first\r\n", tag).as_bytes())
                                .await?;
                        }
                    }
                    "SELECT" => {
                        if args.is_empty() {
                            stream
                                .write_all(
                                    format!("{} BAD Missing mailbox name\r\n", tag).as_bytes(),
                                )
                                .await?;
                            continue;
                        }
                        if let ImapState::Authenticated { ref username }
                        | ImapState::Selected { ref username, .. } = state
                        {
                            let mailbox_name = &args[0];
                            if let Some(mailbox_id) =
                                self.store.get_mailbox_id(username, mailbox_name)?
                            {
                                let messages = self.store.get_messages(&mailbox_id)?;
                                let count = messages.len();
                                state = ImapState::Selected {
                                    username: username.clone(),
                                    mailbox_id: mailbox_id,
                                    mailbox_name: mailbox_name.clone(),
                                };
                                stream
                                    .write_all(format!("* {} EXISTS\r\n", count).as_bytes())
                                    .await?;
                                stream.write_all(b"* 0 RECENT\r\n").await?;
                                stream
                                    .write_all(b"* OK [UIDVALIDITY 1] UIDs valid\r\n")
                                    .await?;
                                stream.write_all(b"* OK [FLAGS (\\Answered \\Flagged \\Deleted \\Draft \\Seen)] Flags permitted\r\n").await?;
                                stream.write_all(b"* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Draft \\Seen)] Permanent flags\r\n").await?;
                                stream
                                    .write_all(
                                        format!("{} OK [READ-WRITE] SELECT completed\r\n", tag)
                                            .as_bytes(),
                                    )
                                    .await?;
                            } else {
                                stream
                                    .write_all(
                                        format!("{} NO SELECT failed: no such mailbox\r\n", tag)
                                            .as_bytes(),
                                    )
                                    .await?;
                            }
                        } else {
                            stream
                                .write_all(format!("{} NO Authenticate first\r\n", tag).as_bytes())
                                .await?;
                        }
                    }
                    "FETCH" => {
                        if args.len() < 2 {
                            stream
                                .write_all(
                                    format!("{} BAD Missing sequence set or query\r\n", tag)
                                        .as_bytes(),
                                )
                                .await?;
                            continue;
                        }
                        if let ImapState::Selected { ref mailbox_id, .. } = state {
                            let sequence_set = &args[0];
                            let query = args[1..].join(" ");
                            let messages = self.store.get_messages(mailbox_id)?;
                            let mut chron_messages = messages;
                            chron_messages.reverse(); // sequence 1 is oldest

                            let indices = parse_sequence_set(sequence_set, chron_messages.len());
                            for idx in indices {
                                let msg = &chron_messages[idx];
                                let seq_num = idx + 1;
                                let uid = idx + 1;

                                let mut fetch_res = Vec::new();
                                let query_upper = query.to_uppercase();

                                if query_upper.contains("UID") {
                                    fetch_res.push(format!("UID {}", uid));
                                }
                                if query_upper.contains("FLAGS") {
                                    let flag_str = if msg.6 { "\\Seen" } else { "" };
                                    fetch_res.push(format!("FLAGS ({})", flag_str));
                                }
                                if query_upper.contains("INTERNALDATE") {
                                    fetch_res.push(format!(
                                        "INTERNALDATE \"{}\"",
                                        format_imap_date(msg.7)
                                    ));
                                }
                                if query_upper.contains("RFC822.SIZE")
                                    || query_upper.contains("BODY.SIZE")
                                {
                                    let headers_part = msg.5.as_deref().unwrap_or("");
                                    let full_raw = if headers_part.is_empty() {
                                        format!(
                                            "From: {}\r\nTo: {}\r\nSubject: {}\r\n\r\n{}",
                                            msg.1,
                                            msg.2,
                                            msg.3.as_deref().unwrap_or(""),
                                            msg.4
                                        )
                                    } else {
                                        format!("{}\r\n{}", headers_part.trim_end(), msg.4)
                                    };
                                    fetch_res.push(format!("RFC822.SIZE {}", full_raw.len()));
                                }

                                if query_upper.contains("BODY") || query_upper.contains("RFC822") {
                                    let headers_part = msg.5.as_deref().unwrap_or("");
                                    let full_raw = if headers_part.is_empty() {
                                        format!(
                                            "From: {}\r\nTo: {}\r\nSubject: {}\r\n\r\n{}",
                                            msg.1,
                                            msg.2,
                                            msg.3.as_deref().unwrap_or(""),
                                            msg.4
                                        )
                                    } else {
                                        format!("{}\r\n{}", headers_part.trim_end(), msg.4)
                                    };

                                    if query_upper.contains("HEADER") {
                                        let headers_only = if headers_part.is_empty() {
                                            format!(
                                                "From: {}\r\nTo: {}\r\nSubject: {}\r\n\r\n",
                                                msg.1,
                                                msg.2,
                                                msg.3.as_deref().unwrap_or("")
                                            )
                                        } else {
                                            format!("{}\r\n", headers_part.trim_end())
                                        };
                                        fetch_res.push(format!(
                                            "BODY[HEADER] {{{}}}\r\n{}",
                                            headers_only.len(),
                                            headers_only
                                        ));
                                    } else if query_upper.contains("TEXT") {
                                        fetch_res.push(format!(
                                            "BODY[TEXT] {{{}}}\r\n{}",
                                            msg.4.len(),
                                            msg.4
                                        ));
                                    } else {
                                        fetch_res.push(format!(
                                            "BODY[] {{{}}}\r\n{}",
                                            full_raw.len(),
                                            full_raw
                                        ));
                                    }
                                }

                                stream
                                    .write_all(
                                        format!(
                                            "* {} FETCH ({})\r\n",
                                            seq_num,
                                            fetch_res.join(" ")
                                        )
                                        .as_bytes(),
                                    )
                                    .await?;
                            }
                            stream
                                .write_all(format!("{} OK FETCH completed\r\n", tag).as_bytes())
                                .await?;
                        } else {
                            stream
                                .write_all(
                                    format!("{} NO Select a mailbox first\r\n", tag).as_bytes(),
                                )
                                .await?;
                        }
                    }
                    "STORE" => {
                        if args.len() < 3 {
                            stream
                                .write_all(
                                    format!("{} BAD Missing parameters for STORE\r\n", tag)
                                        .as_bytes(),
                                )
                                .await?;
                            continue;
                        }
                        if let ImapState::Selected { ref mailbox_id, .. } = state {
                            let sequence_set = &args[0];
                            let _item = &args[1];
                            let value = args[2..].join(" ");
                            let messages = self.store.get_messages(mailbox_id)?;
                            let mut chron_messages = messages;
                            chron_messages.reverse();

                            let indices = parse_sequence_set(sequence_set, chron_messages.len());
                            let is_seen = value.to_uppercase().contains("\\SEEN");
                            let is_deleted = value.to_uppercase().contains("\\DELETED");

                            for idx in indices {
                                let msg = &chron_messages[idx];
                                if is_deleted {
                                    self.store.delete_message(&msg.0)?;
                                } else {
                                    self.store.update_message_flags(&msg.0, is_seen)?;
                                }

                                let flag_str = if is_deleted {
                                    "\\Deleted"
                                } else if is_seen {
                                    "\\Seen"
                                } else {
                                    ""
                                };
                                stream
                                    .write_all(
                                        format!(
                                            "* {} FETCH (FLAGS ({}) UID {})\r\n",
                                            idx + 1,
                                            flag_str,
                                            idx + 1
                                        )
                                        .as_bytes(),
                                    )
                                    .await?;
                            }
                            stream
                                .write_all(format!("{} OK STORE completed\r\n", tag).as_bytes())
                                .await?;
                        } else {
                            stream
                                .write_all(
                                    format!("{} NO Select a mailbox first\r\n", tag).as_bytes(),
                                )
                                .await?;
                        }
                    }
                    "EXPUNGE" => {
                        stream
                            .write_all(format!("{} OK EXPUNGE completed\r\n", tag).as_bytes())
                            .await?;
                    }
                    _ => {
                        stream
                            .write_all(
                                format!("{} BAD Command not implemented\r\n", tag).as_bytes(),
                            )
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn parse_imap_line(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ' ' && !in_quotes {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn parse_sequence_set(set: &str, len: usize) -> Vec<usize> {
    let mut res = Vec::new();
    if len == 0 {
        return res;
    }
    for part in set.split(',') {
        if part == "*" {
            for i in 0..len {
                res.push(i);
            }
        } else if part.contains(':') {
            let range_parts: Vec<&str> = part.split(':').collect();
            if range_parts.len() == 2 {
                let start = range_parts[0]
                    .parse::<usize>()
                    .unwrap_or(1)
                    .saturating_sub(1);
                let end_str = range_parts[1];
                let end = if end_str == "*" {
                    len
                } else {
                    end_str.parse::<usize>().unwrap_or(len).min(len)
                };
                for i in start..end {
                    res.push(i);
                }
            }
        } else {
            if let Ok(idx) = part.parse::<usize>() {
                if idx > 0 && idx <= len {
                    res.push(idx - 1);
                }
            }
        }
    }
    res
}

fn format_imap_date(timestamp: u64) -> String {
    let dt = chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    dt.format("%d-%b-%Y %H:%M:%S +0000").to_string()
}
