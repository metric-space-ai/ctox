//! An authenticated legacy session cannot mint a retained P9 invocation fence.
use super::*;

#[test]
fn e49_legacy_intake_cannot_invoke_native_effects_without_p9_authority() -> anyhow::Result<()> {
    crate::business_os::mcp_channel::native_tasks::tests::with_actor_fixture("admin", |root, session| {
        let command = BusinessCommand {
            id: Some("legacy-native-attempt".into()), module: "ctox".into(),
            command_type: "ctox.task.create".into(), record_id: None,
            payload: serde_json::json!({"title":"Must not run","instruction":"Do not create a task","idempotency_key":"legacy-attempt","target":{"kind":"native"}}),
            client_context: serde_json::json!({}), origin: CommandOrigin::TrustedLocal,
        };
        let prepared = PreparedBusinessCommand::from_command(&command)?;
        let error = match dispatch_business_command(root, "legacy-native-attempt", &command, prepared, Some(session)) {
            Err(error) => error,
            Ok(_) => panic!("a session without a retained P9 fence admitted a native effect"),
        };
        assert!(error.to_string().contains("native execution authority required"));
        Ok(())
    })
}
