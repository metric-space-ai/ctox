// ref: stalwart/src/smtp/client/queue.rs:1-150
// ref: ctox-mailserver queue runner that periodically pulls pending messages from SQLite and delivers them

use crate::store::SqliteStore;
use crate::config::SmtpConfig;
use crate::smtp::client::SmtpOutboundClient;
use crate::smtp::dkim::DkimSigner;
use crate::util::errors::{StalwartError, StalwartResult};
use std::sync::Arc;
use std::time::Duration;
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

            let mut client = SmtpOutboundClient::new(self.config.bind_address);
            let mut connected = false;

            for (id, from, to, body, retry_count) in emails {
                if !connected {
                    match client.connect().await {
                        Ok(_) => {
                            if let Err(e) = client.send_ehlo("localhost").await {
                                error!("EHLO failed for domain {}: {:?}", domain, e);
                                let _ = client.quit().await;
                                let _ = self.handle_failure(&id, retry_count, e);
                                continue;
                            }
                            connected = true;
                        }
                        Err(e) => {
                            error!("Failed to connect to SMTP server for domain {}: {:?}", domain, e);
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
