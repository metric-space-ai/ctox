//! Native transport adapters; domain decisions stay in app handlers.
use super::*;
use crate::mission::channels;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
pub static ROUTES: &[NativeRoute] = &[
    route(
        "mail.send",
        "send",
        "cap.mail.send",
        "EmailAdapter::send_cli",
        "src/core/communication/adapters.rs:412",
    ),
    route(
        "mail.channel",
        "test",
        "ctox.channel.test",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "sync",
        "ctox.channel.sync",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "settings.save",
        "ctox.channel.settings.save",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "pair.start",
        "ctox.channel.pair.start",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "disconnect",
        "ctox.channel.disconnect",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "jami.create",
        "ctox.channel.jami.create",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "jami.export",
        "ctox.channel.jami.export",
        "run_channel_command",
        "src/core/business_os/store_provider_subscription_auth.rs:486",
    ),
    route(
        "mail.channel",
        "mailbox.link",
        "outbound.campaign.mailbox.link",
        "handle_outbound_active_command",
        "src/core/business_os/store_outbound_commands.rs:30",
    ),
    route(
        "mail.server",
        "config.get",
        "ctox.mailserver.get_config",
        "handle_mailserver_command",
        "src/core/business_os/store.rs:16410",
    ),
    route(
        "mail.server",
        "domain.save",
        "ctox.mailserver.save_domain",
        "handle_mailserver_command",
        "src/core/business_os/store.rs:16410",
    ),
    route(
        "mail.server",
        "runtime.save",
        "ctox.mailserver.save_runtime",
        "handle_mailserver_command",
        "src/core/business_os/store.rs:16410",
    ),
    route(
        "mail.server",
        "domain.delete",
        "ctox.mailserver.delete_domain",
        "handle_mailserver_command",
        "src/core/business_os/store.rs:16410",
    ),
    route(
        "mail.server",
        "user.save",
        "ctox.mailserver.save_user",
        "handle_mailserver_command",
        "src/core/business_os/store.rs:16410",
    ),
    route(
        "mail.server",
        "user.delete",
        "ctox.mailserver.delete_user",
        "handle_mailserver_command",
        "src/core/business_os/store.rs:16410",
    ),
    route(
        "mail.read",
        "read",
        "cap.mail.read",
        "channels::load_thread_communication_feed",
        "src/core/mission/channels/mod.rs:2184",
    ),
    route(
        "mail.delivery_log",
        "query",
        "cap.mail.delivery_log",
        "SqliteStore delivery log (read-only projection)",
        "src/core/mailserver/src/store/sqlite.rs:343",
    ),
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendRequest {
    pub account_id: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub subject: String,
    #[serde(default)]
    pub text: String,
    pub html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<blob::BlobRef>,
    pub thread_ref: Option<String>,
    pub idempotency_key: String,
}
impl SendRequest {
    fn validate(&self) -> Result<(), CtoxError> {
        crate::app_capabilities::protocol::identifier(&self.idempotency_key, "$.idempotency_key")?;
        crate::app_capabilities::protocol::identifier(&self.account_id, "$.account_id")?;
        if self.to.is_empty()
            || self.to.len() + self.cc.len() > 100
            || self.attachments.len() > 16
            || self.subject.len() > 4096
            || self.subject.contains(['\r', '\n'])
            || (self.text.is_empty() && self.html.as_deref().unwrap_or("").is_empty())
        {
            return Err(invalid("$.mail"));
        }
        if self
            .to
            .iter()
            .chain(&self.cc)
            .any(|v| !v.contains('@') || v.len() > 320 || v.chars().any(char::is_control))
        {
            return Err(invalid("$.to"));
        }
        if let Some(thread) = &self.thread_ref {
            crate::app_capabilities::protocol::identifier(thread, "$.thread_ref")?;
        }
        for reference in &self.attachments {
            reference.validate()?;
        }
        Ok(())
    }
}
/// Existing durable control claim; uncertain claims NEVER resend. The registry
/// checks current authorization before returning even an existing receipt.
pub fn send_once(
    root: &Path,
    ctx: &CapCallContext,
    request: &SendRequest,
    effect: impl FnOnce() -> Result<Value, CtoxError>,
) -> Result<Value, CtoxError> {
    request.validate()?;
    let key = serde_json::to_vec(&(&ctx.app_id, &ctx.actor.id, &request.idempotency_key))
        .map_err(native_error)?;
    let command_id = format!("cap-mail:{:x}", Sha256::digest(key));
    let intent = json!({"app_id":ctx.app_id,"actor_id":ctx.actor.id,"request":request});
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&intent).map_err(native_error)?)
    );
    let claim = channels::BusinessCommandClaimRequest {
        command_id: command_id.clone(),
        idempotency_key: request.idempotency_key.clone(),
        payload_hash: hash,
        module: ctx.app_id.clone(),
        command_type: "cap.mail.send".into(),
        record_id: request.account_id.clone(),
        intent,
        created_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(native_error)?
            .as_millis()
            .min(i64::MAX as u128) as i64,
    };
    let claimed = channels::claim_business_control_command(root, claim).map_err(native_error)?;
    match claimed.disposition {
        "terminal" => return claimed.result.ok_or_else(CtoxError::uncertain),
        "new" => {}
        _ => return Err(CtoxError::uncertain()),
    }
    let result = effect().map_err(|_| CtoxError::uncertain())?;
    let message_id = result
        .get("message_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(CtoxError::uncertain)?;
    let status = result
        .get("status")
        .and_then(Value::as_str)
        .filter(|s| matches!(*s, "accepted" | "confirmed" | "queued" | "sent"))
        .ok_or_else(CtoxError::uncertain)?;
    let receipt = json!({"message_id":message_id,"status":status});
    channels::complete_business_control_command(root, &command_id, "completed", &receipt, None)
        .map_err(|_| CtoxError::uncertain())?;
    Ok(receipt)
}
pub(super) struct Sender {
    pub address: String,
    pub provider: Option<String>,
    pub display_name: Option<String>,
}
/// Core policy authorizes the requested account before entry. Personal accounts
/// additionally require ownership/sharing; the instance identity must match its
/// configured address exactly, never the native arbitrary-sender fallback.
pub(super) fn resolve_sender(
    root: &Path,
    ctx: &CapCallContext,
    requested: &str,
) -> Result<Sender, CtoxError> {
    if let Some(account) = crate::communication::email_accounts::load_accounts(root)
        .map_err(native_error)?
        .into_iter()
        .find(|a| a.address.eq_ignore_ascii_case(requested))
    {
        if account.owner_user_id != ctx.actor.id
            && !account
                .shared_user_ids
                .as_ref()
                .is_some_and(|ids| ids.contains(&ctx.actor.id))
        {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "mail account is not shared with actor",
            ));
        }
        return Ok(Sender {
            address: account.address,
            provider: Some(account.provider),
            display_name: Some(account.display_name),
        });
    }
    let runtime = crate::communication::gateway::runtime_settings_from_root(
        root,
        crate::communication::gateway::CommunicationAdapterKind::Email,
    );
    let instance = runtime
        .get("CTO_EMAIL_ADDRESS")
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    if instance.is_empty() || !instance.eq_ignore_ascii_case(requested) {
        return Err(CtoxError::new(
            ErrorCode::NotFound,
            "configured mail account not found",
        ));
    }
    Ok(Sender {
        address: instance.to_owned(),
        provider: runtime.get("CTO_EMAIL_PROVIDER").cloned(),
        display_name: None,
    })
}
pub(super) fn send(
    root: &Path,
    blobs: &dyn blob::BlobAccess,
    ctx: &CapCallContext,
    args: &Value,
) -> Result<Value, CtoxError> {
    let request: SendRequest = decode(args)?;
    request.validate()?;
    let account = resolve_sender(root, ctx, &request.account_id)?;
    let directory = root.join("runtime/capability-scratch");
    let mut files = Vec::new();
    if !request.attachments.is_empty() {
        std::fs::create_dir_all(&directory).map_err(native_error)?;
    }
    for reference in &request.attachments {
        use std::io::Write;
        let bytes = blob::read(blobs, ctx, reference)?;
        let mut file = tempfile::Builder::new()
            .prefix("attachment-")
            .tempfile_in(&directory)
            .map_err(native_error)?;
        file.write_all(&bytes).map_err(native_error)?;
        file.flush().map_err(native_error)?;
        files.push(file);
    }
    let paths = files
        .iter()
        .map(|f| f.path().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let thread = request
        .thread_ref
        .clone()
        .unwrap_or_else(|| format!("cap-mail-{}", request.idempotency_key));
    send_once(root, ctx, &request, || {
        let db = crate::paths::core_db(root);
        let native = crate::communication::adapters::EmailSendCommandRequest {
            db_path: &db,
            sender_email: &account.address,
            provider: account.provider.as_deref(),
            profile_json: None,
            thread_key: &thread,
            to: &request.to,
            cc: &request.cc,
            sender_display: account.display_name.as_deref(),
            subject: &request.subject,
            body: &request.text,
            body_html: request.html.as_deref(),
            attachments: &paths,
        };
        let value = crate::communication::adapters::email()
            .send_cli(root, &native)
            .map_err(native_error)?;
        Ok(json!({"message_id":value["messageId"],"status":value["status"]}))
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    thread_key: String,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    100
}
pub(super) fn read(root: &Path, args: &Value) -> Result<Value, CtoxError> {
    let request: ReadRequest = decode(args)?;
    crate::app_capabilities::protocol::identifier(&request.thread_key, "$.thread_key")?;
    if request.limit == 0 || request.limit > 1000 {
        return Err(invalid("$.limit"));
    }
    let feed = channels::load_thread_communication_feed(root, &request.thread_key, request.limit)
        .map_err(native_error)?;
    let mut messages = Vec::new();
    for item in feed {
        let Some(record) = channels::pull_communication_record_for_business_os(
            root,
            "communication_messages",
            &item.message_key,
        )
        .map_err(native_error)?
        else {
            continue;
        };
        // A message moved concurrently must not cross the authorized thread scope.
        if record["thread_key"] != request.thread_key {
            continue;
        }
        let fields = [
            "message_key",
            "channel",
            "account_key",
            "thread_key",
            "remote_id",
            "direction",
            "sender_display",
            "sender_address",
            "recipient_addresses_json",
            "cc_addresses_json",
            "subject",
            "body_text",
            "body_html",
            "status",
            "external_created_at",
        ];
        messages.push(Value::Object(
            fields
                .into_iter()
                .filter_map(|key| record.get(key).map(|value| (key.to_owned(), value.clone())))
                .collect(),
        ));
    }
    Ok(json!({"messages":messages}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryQuery {
    #[serde(default)]
    message_ids: Vec<String>,
    since: Option<i64>,
    #[serde(default = "default_limit")]
    limit: usize,
}
pub(super) fn delivery_log(root: &Path, args: &Value) -> Result<Value, CtoxError> {
    let request: DeliveryQuery = decode(args)?;
    if request.limit == 0 || request.limit > 1000 || request.message_ids.len() > 100 {
        return Err(invalid("$.limit"));
    }
    for id in &request.message_ids {
        crate::app_capabilities::protocol::identifier(id, "$.message_ids")?;
    }
    // Read the existing native SMTP log, never run the app-mutating reconcile.
    let connection = rusqlite::Connection::open_with_flags(
        crate::paths::core_db(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(native_error)?;
    let mut statement=connection.prepare("SELECT id,outcome,completed_at FROM stalwart_smtp_delivery_log WHERE completed_at>=?1 AND (?2='[]' OR id IN (SELECT value FROM json_each(?2))) ORDER BY completed_at DESC,id LIMIT ?3").map_err(native_error)?;
    let ids = serde_json::to_string(&request.message_ids).map_err(native_error)?;
    let rows=statement.query_map(rusqlite::params![request.since.unwrap_or(0),ids,request.limit as i64],|row|{
    Ok(json!({"message_id":row.get::<_,String>(0)?,"status":row.get::<_,String>(1)?,"at":row.get::<_,i64>(2)?}))
 }).map_err(native_error)?.collect::<Result<Vec<_>,_>>().map_err(native_error)?;
    Ok(json!({"deliveries":rows}))
}
/// No raw credentials or legacy file paths may reach durable command intake.
pub(super) fn validate_legacy(value: &Value) -> Result<(), CtoxError> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_ascii_lowercase();
                if key == "path"
                    || key.ends_with("_path")
                    || key.ends_with("path")
                    || key == "attachments"
                    || ((!key.ends_with("_ref"))
                        && (key.contains("password")
                            || key == "token"
                            || key == "secret"
                            || key == "dkim_private_key"))
                {
                    return Err(invalid("$.args"));
                }
                validate_legacy(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_legacy(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn redact_paths(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| {
                    let k = key.to_ascii_lowercase();
                    k != "path" && !k.ends_with("_path") && !k.ends_with("path")
                })
                .map(|(k, v)| (k, redact_paths(v)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(redact_paths).collect()),
        other => other,
    }
}

// ---------------------------------------------------------------- CAP-3
// Remaining mail.server infrastructure routes (§16.28): passwords arrive only
// through the command's secret intake, a DKIM private key is generated by the
// Core (never supplied by an app), and no secret field leaves in a result.

/// Builds the native payload for a mail.server route. Pure, no effect.
pub fn server_payload(
    intake: &dyn crate::app_capabilities::cap3::secrets::SecretIntake,
    ctx: &CapCallContext,
    function: &str,
    payload: &Value,
) -> Result<Value, CtoxError> {
    match function {
        "user.save" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct UserSave {
                command_id: String,
                password: Value,
                username: String,
                owner_user_id: Option<String>,
                communication_account: Option<Value>,
            }
            let request: UserSave = decode(payload)?;
            if request.command_id != ctx.command_id {
                return Err(CtoxError::new(
                    ErrorCode::Forbidden,
                    "secret intake belongs to another command",
                ));
            }
            if request.password != json!({"secret_intake": true}) {
                return Err(invalid("$.password"));
            }
            crate::app_capabilities::protocol::identifier(&request.username, "$.username")?;
            if let Some(owner) = &request.owner_user_id {
                crate::app_capabilities::protocol::identifier(owner, "$.owner_user_id")?;
            }
            if let Some(account) = &request.communication_account {
                validate_legacy(account)?;
            }
            let password = intake
                .take(ctx, "password")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| invalid("$.password"))?;
            // Native intake detaches `password` before any persistence
            // (store::detach_secret_intake_value for ctox.mailserver.save_user).
            Ok(
                json!({"username":request.username,"owner_user_id":request.owner_user_id,
                      "password":password}),
            )
        }
        "domain.save" => {
            let mut map = payload
                .as_object()
                .cloned()
                .ok_or_else(|| invalid("$.args"))?;
            match map.remove("dkim_private_key") {
                None | Some(Value::Null) => {}
                Some(_) => return Err(invalid("$.dkim_private_key")),
            }
            let value = Value::Object(map);
            validate_legacy(&value)?;
            Ok(value)
        }
        _ => {
            validate_legacy(payload)?;
            Ok(payload.clone())
        }
    }
}
/// No password, secret, token or private key field leaves a mail.server
/// result (`*_set` / `*_ref` flags stay).
pub fn redact_secrets(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| {
                    let k = key.to_ascii_lowercase();
                    k.ends_with("_set")
                        || k.ends_with("_ref")
                        || !(k.contains("password")
                            || k.contains("secret")
                            || k.contains("private_key")
                            || k.contains("token"))
                })
                .map(|(k, v)| (k, redact_secrets(v)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(redact_secrets).collect()),
        other => other,
    }
}
pub fn server(
    root: &Path,
    intake: &dyn crate::app_capabilities::cap3::secrets::SecretIntake,
    ctx: &CapCallContext,
    call: &NativeCall,
) -> Result<Value, CtoxError> {
    let native = server_payload(intake, ctx, call.route().function, call.payload())?;
    let call = call.route().call(&json!({ "payload": native }))?;
    crate::app_capabilities::native::NativeCommandBackend::new(root)
        .execute(ctx, &call)
        .map(|value| redact_secrets(redact_paths(value)))
}
