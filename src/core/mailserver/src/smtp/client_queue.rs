// ref: stalwart/src/smtp/client/queue.rs:1-150
// ref: ctox-mailserver queue runner that periodically pulls pending messages from SQLite and delivers them

use crate::store::SqliteStore;
use crate::config::SmtpConfig;
use crate::smtp::client::SmtpOutboundClient;
use crate::smtp::dkim::DkimSigner;
use crate::util::errors::{StalwartError, StalwartResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{info, error, warn};

pub struct SmtpOutboundQueue {
    store: SqliteStore,
    config: SmtpConfig,
}

impl SmtpOutboundQueue {
    pub fn new(store: SqliteStore, config: SmtpConfig) -> Self {
        Self { store, config }
    }

    pub async fn start(self: Arc<Self>) {
        info!("Starting SMTP outbound queue runner");
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = self.process_queue().await {
                error!("Error processing SMTP outbound queue: {:?}", e);
            }
        }
    }

    pub async fn process_queue(&self) -> StalwartResult<()> {
        let pending = self.store.get_pending_emails()?;
        if pending.is_empty() {
            return Ok(());
        }

        info!("Found {} pending outbound emails in queue", pending.len());

        // Group pending emails by recipient domain
        use std::collections::HashMap;
        let mut grouped: HashMap<String, Vec<(String, String, String, String, usize)>> = HashMap::new();

        for item in pending {
            let to = &item.2;
            let parts: Vec<&str> = to.split('@').collect();
            let domain = if parts.len() >= 2 {
                parts[1].to_string()
            } else {
                "unknown".to_string()
            };
            grouped.entry(domain).or_default().push(item);
        }

        for (domain, emails) in grouped {
            if domain == "unknown" {
                for (id, _from, to, _body, retry_count) in emails {
                    let err = StalwartError::General(format!("Invalid recipient address: {}", to));
                    let _ = self.handle_failure(&id, retry_count, err);
                }
                continue;
            }

            // Determine if domain is local or remote
            let is_local_domain = domain == "localhost"
                || domain == "ctox.local"
                || self.store.get_domain_dkim(&domain).map(|opt| opt.is_some()).unwrap_or(false)
                || emails.iter().any(|(_, _, to, _, _)| self.store.user_exists(to).unwrap_or(false));

            let mut target_addr = self.config.bind_address;
            if !is_local_domain {
                // Perform MX lookup
                let mx_hosts = resolve_mx_records(&domain).await;
                let mut resolved_addr = None;
                for host in mx_hosts {
                    let clean_host = host.trim_end_matches('.').to_string();
                    let lookup_res = tokio::net::lookup_host((clean_host.as_str(), 25)).await;
                    if let Ok(mut addrs) = lookup_res {
                        if let Some(addr) = addrs.next() {
                            resolved_addr = Some(addr);
                            break;
                        }
                    }
                }
                
                // Fallback to A record lookup if no MX records resolved
                if resolved_addr.is_none() {
                    let lookup_res = tokio::net::lookup_host((domain.as_str(), 25)).await;
                    if let Ok(mut addrs) = lookup_res {
                        if let Some(addr) = addrs.next() {
                            resolved_addr = Some(addr);
                        }
                    }
                }
                
                if let Some(addr) = resolved_addr {
                    target_addr = addr;
                } else {
                    for (id, _from, _to, _body, retry_count) in emails {
                        let err = StalwartError::General(format!("Failed to resolve mail server for domain {}", domain));
                        let _ = self.handle_failure(&id, retry_count, err);
                    }
                    continue;
                }
            }

            let mut client = SmtpOutboundClient::new(target_addr);
            let mut connected = false;

            for (id, from, to, body, retry_count) in emails {
                if !connected {
                    match client.connect().await {
                        Ok(_) => {
                            if let Err(e) = client.send_ehlo(&domain).await {
                                error!("EHLO failed for domain {}: {:?}", domain, e);
                                let _ = client.quit().await;
                                let _ = self.handle_failure(&id, retry_count, e);
                                continue;
                            }
                            connected = true;
                        }
                        Err(e) => {
                            error!("Failed to connect to SMTP server for domain {} at {}: {:?}", domain, target_addr, e);
                            let _ = self.handle_failure(&id, retry_count, e);
                            continue;
                        }
                    }
                }

                match self.deliver_on_connection(&mut client, &from, &to, &body).await {
                    Ok(_) => {
                        info!("Successfully delivered email {} to {}", id, to);
                        let _ = self.store.delete_email(&id);

                        // Reset session via RSET to reuse the connection
                        if let Err(e) = client.reset().await {
                            warn!("Failed to reset SMTP session: {:?}", e);
                            connected = false;
                            let _ = client.quit().await;
                        }
                    }
                    Err(e) => {
                        error!("Failed to deliver email {} over pooled connection: {:?}", id, e);
                        let _ = self.handle_failure(&id, retry_count, e);
                        connected = false;
                        let _ = client.quit().await;
                    }
                }
            }

            if connected {
                let _ = client.quit().await;
            }
        }

        Ok(())
    }

    async fn deliver_on_connection(&self, client: &mut SmtpOutboundClient, from: &str, to: &str, body: &str) -> StalwartResult<()> {
        let sender_parts: Vec<&str> = from.split('@').collect();
        let dkim_signer = if sender_parts.len() >= 2 {
            let sender_domain = sender_parts[1];
            if let Ok(Some((selector, priv_key))) = self.store.get_domain_dkim(sender_domain) {
                DkimSigner::new(&selector, sender_domain, &priv_key).ok()
            } else {
                None
            }
        } else {
            None
        };

        client.send_mail(from, &[to.to_string()], body, dkim_signer.as_ref()).await?;
        Ok(())
    }

    fn handle_failure(&self, id: &str, retry_count: usize, e: StalwartError) -> StalwartResult<()> {
        let next_retry = retry_count + 1;
        if next_retry >= 5 {
            warn!("Email {} failed permanently after {} retries: {:?}", id, next_retry, e);
            self.store.update_email_status(id, "failed_permanent", crate::util::now_utc_secs() + 86400, next_retry)?;
        } else {
            let backoff = 60 * (1 << next_retry);
            warn!("Email {} failed to deliver, will retry in {} seconds: {:?}", id, backoff, e);
            self.store.update_email_status(id, "pending", crate::util::now_utc_secs() + backoff, next_retry)?;
        }
        Ok(())
    }
}

async fn resolve_mx_records(domain: &str) -> Vec<String> {
    let mut query = Vec::new();
    query.extend_from_slice(&[0x12, 0x34]); // Transaction ID
    query.extend_from_slice(&[0x01, 0x00]); // Flags: Standard query, recursion desired
    query.extend_from_slice(&[0x00, 0x01]); // Questions: 1
    query.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // Answers/Authority/Additional: 0

    for part in domain.split('.') {
        if part.is_empty() { continue; }
        query.push(part.len() as u8);
        query.extend_from_slice(part.as_bytes());
    }
    query.push(0); // Terminating 0 length label

    query.extend_from_slice(&[0x00, 0x0f]); // Type: MX (15)
    query.extend_from_slice(&[0x00, 0x01]); // Class: IN (1)

    let mut mx_servers = Vec::new();
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return mx_servers,
    };

    if socket.connect("8.8.8.8:53").await.is_err() {
        return mx_servers;
    }

    if socket.send(&query).await.is_err() {
        return mx_servers;
    }

    let mut buf = vec![0u8; 1024];
    let n = match timeout(Duration::from_secs(3), socket.recv(&mut buf)).await {
        Ok(Ok(n)) => n,
        _ => return mx_servers,
    };

    let response = &buf[..n];
    if response.len() < 12 {
        return mx_servers;
    }

    let ancount = ((response[6] as u16) << 8) | (response[7] as u16);
    if ancount == 0 {
        return mx_servers;
    }

    let mut pos = 12;
    // Skip Question Section
    while pos < response.len() {
        let len = response[pos] as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            pos += 2;
            break;
        }
        pos += 1 + len;
    }
    pos += 4; // Skip type & class of question

    // Parse Answers
    for _ in 0..ancount {
        if pos >= response.len() { break; }
        // Skip Name field
        while pos < response.len() {
            let len = response[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            if len & 0xC0 == 0xC0 {
                pos += 2;
                break;
            }
            pos += 1 + len;
        }

        if pos + 10 > response.len() { break; }
        let rtype = ((response[pos] as u16) << 8) | (response[pos+1] as u16);
        let rdlength = (((response[pos+8] as u16) << 8) | (response[pos+9] as u16)) as usize;
        pos += 10;

        if rtype == 15 { // MX record
            if pos + rdlength > response.len() { break; }
            let name_pos = pos + 2; // skip preference (2 bytes)
            if let Some((parsed_name, _)) = parse_dns_name(response, name_pos, 0) {
                mx_servers.push(parsed_name);
            }
        }
        pos += rdlength;
    }

    mx_servers
}

fn parse_dns_name(response: &[u8], mut p: usize, depth: usize) -> Option<(String, usize)> {
    if depth > 10 { return None; }
    let mut name = String::new();
    let mut read_bytes = 0;
    let mut jumped = false;
    
    loop {
        if p >= response.len() { return None; }
        let len = response[p] as usize;
        if len == 0 {
            if !jumped { read_bytes += 1; }
            break;
        }
        
        if len & 0xC0 == 0xC0 {
            if p + 1 >= response.len() { return None; }
            let offset = (((len & 0x3F) as usize) << 8) | (response[p+1] as usize);
            if !jumped { read_bytes += 2; }
            jumped = true;
            let (referred_name, _) = parse_dns_name(response, offset, depth + 1)?;
            if !name.is_empty() { name.push('.'); }
            name.push_str(&referred_name);
            break;
        } else {
            p += 1;
            if !jumped { read_bytes += 1; }
            if p + len > response.len() { return None; }
            let label = String::from_utf8_lossy(&response[p..p+len]);
            if !name.is_empty() { name.push('.'); }
            name.push_str(&label);
            p += len;
            if !jumped { read_bytes += len; }
        }
    }
    Some((name, read_bytes))
}
