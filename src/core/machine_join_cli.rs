// Origin: CTOX
// License: AGPL-3.0-only
//! `ctox machine join --token <TOKEN|-> [--label <LABEL>]` (Sync v4 §16.41).
//!
//! The joining machine reuses the existing native first enrollment: the token
//! carries a one-time Business OS WebRTC invite (`mobile_invites`), pairing
//! (`transfers pair`) generates the device proof key in the CTOX secret store
//! (`native_data_device`) and binds it at the instance on first use. Steps are
//! reported over that admitted channel; the token never reaches argv (`-`
//! reads stdin), logs, disk or a command document.
use crate::business_os::machine_enrollment::{join_proof, JoinError, JoinToken, HEARTBEAT, STEP};
use anyhow::Result;
use ctox_sync::business_data_contract::{
    NativeBusinessDataCommand as Command, NativeBusinessDataCommandStatus as Status,
    NativeBusinessDataOperation as Operation, NativeBusinessDataRequest as Request,
    NativeBusinessDataResult as WireResult, NativeBusinessDataSessionRef as SessionRef,
    CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::path::Path;

const USAGE: &str = "usage: ctox machine join --token <TOKEN|-> [--label <LABEL>]";

pub fn handle(root: &Path, args: &[String]) -> Result<()> {
    anyhow::ensure!(args.first().map(String::as_str) == Some("join"), USAGE);
    let mut token = None;
    let mut label = None;
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--token" => token = rest.next().cloned(),
            "--label" => label = rest.next().cloned(),
            _ => anyhow::bail!(USAGE),
        }
    }
    let token = match token.as_deref() {
        Some("-") => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            line.trim().to_owned()
        }
        Some(value) => value.to_owned(),
        None => anyhow::bail!(USAGE),
    };
    match join(root, &token, label.as_deref()) {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Err(code) => {
            // Deterministic code only: never echo token, invite or peer text.
            println!("{}", json!({"ok": false, "error_code": code.code()}));
            Err(anyhow::Error::new(code))
        }
    }
}

fn join(root: &Path, token: &str, label: Option<&str>) -> Result<Value, JoinError> {
    let token = JoinToken::decode(token)?;
    let pin = token.source_pin.clone().ok_or(JoinError::BindRejected)?;
    let target_id = format!("machine-{}", token.enrollment_id);
    // Typed role config (no environment toggle): read by the machine service.
    write_role(root, &token, &target_id, label).map_err(|_| JoinError::ServiceStartFailed)?;
    let service = crate::service::start_background(root)
        .map(|_| ())
        .map_err(|_| JoinError::ServiceStartFailed);
    let started_at = chrono::Utc::now().timestamp_millis();
    let enrollment_id = token.enrollment_id.clone();
    let proof = join_proof(&token.secret);
    let service_failed = service.is_err();
    let report: crate::transfers_native::AfterReady = Box::new(move |pool, peer| {
        Box::pin(async move {
            let service_step = if service_failed {
                json!({"key":"service_started","state":"failed",
                    "error_code":"service_start_failed","detail":format!("at_ms={started_at}")})
            } else {
                json!({"key":"service_started","state":"done","detail":format!("at_ms={started_at}")})
            };
            let mut first = service_step;
            first["join_proof"] = json!(proof);
            submit(&pool, &peer, &enrollment_id, STEP, first).await?;
            if service_failed {
                anyhow::bail!(JoinError::ServiceStartFailed);
            }
            for key in ["device_bound", "peer_connected"] {
                submit(
                    &pool,
                    &peer,
                    &enrollment_id,
                    STEP,
                    json!({"key":key,"state":"done"}),
                )
                .await?;
            }
            // First life sign; the core alone then sets `green`.
            submit(&pool, &peer, &enrollment_id, HEARTBEAT, json!({})).await
        })
    });
    let paired = crate::transfers_native::pair_invite(
        root,
        &target_id,
        &pin,
        token.invite.clone(),
        Some(report),
    )
    .map_err(|error| classify(&error))?;
    service?;
    Ok(json!({
        "ok": true,
        "enrollment_id": token.enrollment_id,
        "target_id": target_id,
        "instance_id": token.instance_id,
        "device_id": paired.get("deviceId"),
    }))
}

async fn submit(
    pool: &ctox_sync::native::NativePool,
    peer: &rxdb::plugins::replication_webrtc::WebRTCRsConnection,
    enrollment_id: &str,
    command_type: &str,
    mut payload: Value,
) -> Result<()> {
    payload["enrollment_id"] = json!(enrollment_id);
    let id = format!(
        "machine-join-{}-{}",
        enrollment_id,
        uuid_like(command_type, &payload)
    );
    let request = Request {
        version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
        request_id: id.clone(),
        operation: Operation::SubmitCommand {
            session: SessionRef {
                handle: "machine-join".into(),
                generation: 1,
            },
            command: Command {
                command_id: id,
                command_type: command_type.into(),
                payload,
            },
        },
    };
    let response =
        ctox_sync::business_data_remote::remote_request(pool.clone(), peer.clone(), &request)
            .await
            .map_err(|_| JoinError::InstanceUnreachable)?;
    let WireResult::Command { state, .. } = response.result else {
        anyhow::bail!(JoinError::BindRejected);
    };
    let ok = state.status == Status::Completed
        && state
            .result
            .as_ref()
            .and_then(|r| r.get("ok"))
            .and_then(Value::as_bool)
            == Some(true);
    if ok {
        return Ok(());
    }
    let code = state
        .result
        .as_ref()
        .and_then(|r| r.get("error_code"))
        .and_then(Value::as_str)
        .unwrap_or("bind_rejected");
    anyhow::bail!(match code {
        "token_expired" => JoinError::TokenExpired,
        "token_used" => JoinError::TokenUsed,
        "token_invalid" => JoinError::TokenInvalid,
        "service_start_failed" => JoinError::ServiceStartFailed,
        "enrollment_closed" => JoinError::EnrollmentClosed,
        _ => JoinError::BindRejected,
    })
}

/// Stable per-step command id: a retried join replays, never duplicates.
fn uuid_like(command_type: &str, payload: &Value) -> String {
    let key = payload.get("key").and_then(Value::as_str).unwrap_or("hb");
    let digest = ring::digest::digest(
        &ring::digest::SHA256,
        format!("{command_type}:{key}").as_bytes(),
    );
    digest.as_ref()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Map pairing failures to the fixed codes; never surface peer text.
fn classify(error: &anyhow::Error) -> JoinError {
    if let Some(code) = error.chain().find_map(|e| e.downcast_ref::<JoinError>()) {
        return *code;
    }
    let text = format!("{error:#}");
    if text.contains("invite expired") {
        JoinError::TokenExpired
    } else if text.contains("peer_authentication_failed")
        || text.contains("already exists")
        || text.contains("one-time pairing")
    {
        JoinError::BindRejected
    } else {
        JoinError::InstanceUnreachable
    }
}

fn write_role(root: &Path, token: &JoinToken, target_id: &str, label: Option<&str>) -> Result<()> {
    let dir = crate::paths::runtime_dir(root).join("machine");
    std::fs::create_dir_all(&dir)?;
    let role = json!({
        "version": 1,
        "role": "machine",
        "enrollment_id": token.enrollment_id,
        "instance_id": token.instance_id,
        "target_id": target_id,
        "label": label,
    });
    let temporary = dir.join("role.json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(&role)?)?;
    std::fs::rename(temporary, dir.join("role.json"))?;
    Ok(())
}
