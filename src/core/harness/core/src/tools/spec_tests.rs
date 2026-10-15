use crate::client_common::tools::FreeformTool;
use crate::config::test_config;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::model_info::with_config_overrides;
use crate::tools::ToolRouter;
use crate::tools::registry::ConfiguredToolSpec;
use crate::tools::router::ToolRouterParams;
use ctox_app_server_protocol::AppInfo;
use ctox_protocol::openai_models::ModelInfo;
use ctox_protocol::openai_models::ModelsResponse;
use ctox_protocol::protocol::ReadOnlyAccess;
use ctox_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;

use super::*;

fn mcp_tool(name: &str, description: &str, input_schema: serde_json::Value) -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        name.to_string(),
        description.to_string(),
        std::sync::Arc::new(rmcp::model::object(input_schema)),
    )
}



#[test]
fn channel_tools_are_registered_in_default_tool_router() {
    let model_info = model_info_from_models_json("gpt-5-codex");
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let router = ToolRouter::from_config(
        &config,
        ToolRouterParams {
            mcp_tools: None,
            app_tools: None,
            discoverable_tools: None,
            dynamic_tools: &[],
        },
    );
    let specs = router.specs();
    assert_contains_tool_names(
        &specs
            .iter()
            .map(|spec| ConfiguredToolSpec {
                spec: spec.clone(),
                supports_parallel_tool_calls: false,
            })
            .collect::<Vec<_>>(),
        &[
            "channel_sync",
            "channel_take",
            "channel_ack",
            "channel_send",
        ],
    );
}

#[test]
fn explicit_no_tools_surface_restores_neither_builtin_nor_dynamic_tools() {
    let model_info = model_info_from_models_json("gpt-5-codex");
    let mut features = Features::with_defaults();
    features.enable(Feature::ApplyPatchFreeform);
    features.enable(Feature::Collab);
    let available_models = Vec::new();
    let config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Live),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::WorkspaceWrite {
            writable_roots: Vec::new(),
            read_only_access: ReadOnlyAccess::FullAccess,
            network_access: true,
            exclude_tmpdir_env_var: true,
            exclude_slash_tmp: true,
        },
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    })
    .with_no_tools_surface(true);
    let dynamic = DynamicToolSpec {
        name: "persisted_dynamic_mutator".to_string(),
        description: "must not be restored".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
        defer_loading: false,
    };
    let (tools, _) = build_specs(&config, None, None, &[dynamic]).build();
    assert!(
        tools.is_empty(),
        "explicit no-tools contract leaked {} tool(s)",
        tools.len()
    );
}

#[test]
fn lean_tool_surface_exposes_only_authoring_tools() {
    let model_info = model_info_from_models_json("gpt-5-codex");
    let mut features = Features::with_defaults();
    features.enable(Feature::ApplyPatchFreeform);
    features.enable(Feature::Collab);
    features.enable(Feature::Artifact);
    features.enable(Feature::JsRepl);
    features.enable(Feature::ToolSuggest);
    let available_models = Vec::new();
    let config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Live),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    })
    .with_lean_tool_surface(true);
    let mut mcp_tools = std::collections::HashMap::new();
    mcp_tools.insert(
        "demo__oversized_tool".to_string(),
        mcp_tool(
            "oversized_tool",
            "A tool that should not be model-visible in lean authoring sessions.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "payload": { "type": "string" }
                }
            }),
        ),
    );

    let (tools, _) = build_specs(&config, Some(mcp_tools), None, &[]).build();
    let names = tools
        .iter()
        .map(|tool| tool_name(&tool.spec).to_string())
        .collect::<Vec<_>>();

    assert!(
        names.iter().any(|name| matches!(
            name.as_str(),
            "exec_command" | "shell_command" | "shell" | "local_shell"
        )),
        "expected a shell authoring tool; had: {names:?}"
    );
    assert!(names.iter().any(|name| name == "apply_patch"));
    assert!(names.iter().any(|name| name == "update_plan"));
    for absent in [
        "view_image",
        "request_user_input",
        "tool_search",
        "tool_suggest",
        "artifacts",
        "channel_send",
        "context_retrieve",
        "meeting_status",
        "list_mcp_resources",
        "demo__oversized_tool",
    ] {
        assert!(
            !names.iter().any(|name| name == absent),
            "expected lean tool surface to omit {absent}; had: {names:?}"
        );
    }
    assert!(
        names.len() <= 4,
        "lean authoring surface should stay small; had: {names:?}"
    );
}

#[test]
fn mcp_tool_to_openai_tool_inserts_empty_properties() {
    let mut schema = rmcp::model::JsonObject::new();
    schema.insert("type".to_string(), serde_json::json!("object"));

    let tool = rmcp::model::Tool::new("no_props", "No properties", schema);

    let openai_tool =
        mcp_tool_to_openai_tool("server/no_props".to_string(), tool).expect("convert tool");
    let parameters = serde_json::to_value(openai_tool.parameters).expect("serialize schema");

    assert_eq!(parameters.get("properties"), Some(&serde_json::json!({})));
}

#[test]
fn mcp_tool_to_openai_tool_preserves_top_level_output_schema() {
    let mut input_schema = rmcp::model::JsonObject::new();
    input_schema.insert("type".to_string(), serde_json::json!("object"));

    let mut output_schema = rmcp::model::JsonObject::new();
    output_schema.insert(
        "properties".to_string(),
        serde_json::json!({
            "result": {
                "properties": {
                    "nested": {}
                }
            }
        }),
    );
    output_schema.insert("required".to_string(), serde_json::json!(["result"]));

    let tool = rmcp::model::Tool::new("with_output", "Has output schema", input_schema)
        .with_raw_output_schema(std::sync::Arc::new(output_schema));

    let openai_tool = mcp_tool_to_openai_tool("mcp__server__with_output".to_string(), tool)
        .expect("convert tool");

    assert_eq!(
        openai_tool.output_schema,
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "array",
                    "items": {}
                },
                "structuredContent": {
                    "properties": {
                        "result": {
                            "properties": {
                                "nested": {}
                            }
                        }
                    },
                    "required": ["result"]
                },
                "isError": {
                    "type": "boolean"
                },
                "_meta": {}
            },
            "required": ["content"],
            "additionalProperties": false
        }))
    );
}

#[test]
fn mcp_tool_to_openai_tool_preserves_output_schema_without_inferred_type() {
    let mut input_schema = rmcp::model::JsonObject::new();
    input_schema.insert("type".to_string(), serde_json::json!("object"));

    let mut output_schema = rmcp::model::JsonObject::new();
    output_schema.insert("enum".to_string(), serde_json::json!(["ok", "error"]));

    let tool = rmcp::model::Tool::new("with_enum_output", "Has enum output schema", input_schema)
        .with_raw_output_schema(std::sync::Arc::new(output_schema));

    let openai_tool = mcp_tool_to_openai_tool("mcp__server__with_enum_output".to_string(), tool)
        .expect("convert tool");

    assert_eq!(
        openai_tool.output_schema,
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "array",
                    "items": {}
                },
                "structuredContent": {
                    "enum": ["ok", "error"]
                },
                "isError": {
                    "type": "boolean"
                },
                "_meta": {}
            },
            "required": ["content"],
            "additionalProperties": false
        }))
    );
}



fn tool_name(tool: &ToolSpec) -> &str {
    match tool {
        ToolSpec::Function(ResponsesApiTool { name, .. }) => name,
        ToolSpec::ToolSearch { .. } => "tool_search",
        ToolSpec::LocalShell {} => "local_shell",
        ToolSpec::ImageGeneration { .. } => "image_generation",
        ToolSpec::WebSearch { .. } => "web_search",
        ToolSpec::Freeform(FreeformTool { name, .. }) => name,
    }
}

// Avoid order-based assertions; compare via set containment instead.
fn assert_contains_tool_names(tools: &[ConfiguredToolSpec], expected_subset: &[&str]) {
    use std::collections::HashSet;
    let mut names = HashSet::new();
    let mut duplicates = Vec::new();
    for name in tools.iter().map(|t| tool_name(&t.spec)) {
        if !names.insert(name) {
            duplicates.push(name);
        }
    }
    assert!(
        duplicates.is_empty(),
        "duplicate tool entries detected: {duplicates:?}"
    );
    for expected in expected_subset {
        assert!(
            names.contains(expected),
            "expected tool {expected} to be present; had: {names:?}"
        );
    }
}

fn assert_lacks_tool_name(tools: &[ConfiguredToolSpec], expected_absent: &str) {
    let names = tools
        .iter()
        .map(|tool| tool_name(&tool.spec))
        .collect::<Vec<_>>();
    assert!(
        !names.contains(&expected_absent),
        "expected tool {expected_absent} to be absent; had: {names:?}"
    );
}

fn shell_tool_name(config: &ToolsConfig) -> Option<&'static str> {
    match config.shell_type {
        ConfigShellToolType::Default => Some("shell"),
        ConfigShellToolType::Local => Some("local_shell"),
        ConfigShellToolType::UnifiedExec => None,
        ConfigShellToolType::Disabled => None,
        ConfigShellToolType::ShellCommand => Some("shell_command"),
    }
}

fn find_tool<'a>(tools: &'a [ConfiguredToolSpec], expected_name: &str) -> &'a ConfiguredToolSpec {
    tools
        .iter()
        .find(|tool| tool_name(&tool.spec) == expected_name)
        .unwrap_or_else(|| panic!("expected tool {expected_name}"))
}



fn model_info_from_models_json(slug: &str) -> ModelInfo {
    let config = test_config();
    let response: ModelsResponse =
        serde_json::from_str(include_str!("../../models.json")).expect("valid models.json");
    let model = response
        .models
        .into_iter()
        .find(|candidate| candidate.slug == slug)
        .unwrap_or_else(|| panic!("model slug {slug} is missing from models.json"));
    with_config_overrides(model, &config)
}




#[test]
fn removed_collab_flag_cannot_expose_free_subagent_tools() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::Collab);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();
    for tool_name in [
        "spawn_agent",
        "send_input",
        "list_agents",
        "resume_agent",
        "wait_agent",
        "close_agent",
        "spawn_agents_on_csv",
    ] {
        assert_lacks_tool_name(&tools, tool_name);
    }
}

#[test]
fn removed_fanout_flag_cannot_expose_free_subagent_tools() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::SpawnCsv);
    features.normalize_dependencies();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();
    for tool_name in [
        "spawn_agent",
        "send_input",
        "list_agents",
        "resume_agent",
        "wait_agent",
        "close_agent",
        "spawn_agents_on_csv",
    ] {
        assert_lacks_tool_name(&tools, tool_name);
    }
}

#[test]
fn legacy_thread_spawn_source_cannot_expose_free_subagent_tools() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::SpawnCsv);
    features.normalize_dependencies();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ctox_protocol::ThreadId::new(),
            depth: 1,
            agent_path: None,
            agent_nickname: Some("worker".to_string()),
            agent_role: None,
        }),
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();
    for tool_name in [
        "spawn_agent",
        "send_input",
        "list_agents",
        "resume_agent",
        "wait_agent",
        "close_agent",
        "spawn_agents_on_csv",
        "report_agent_job_result",
    ] {
        assert_lacks_tool_name(&tools, tool_name);
    }
    assert_lacks_tool_name(&tools, "request_user_input");
}




#[test]
fn removed_agent_job_worker_source_cannot_expose_result_tool() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::SpawnCsv);
    features.normalize_dependencies();
    features.enable(Feature::Sqlite);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::SubAgent(SubAgentSource::Other(
            "agent_job:test".to_string(),
        )),
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();
    assert_lacks_tool_name(&tools, "report_agent_job_result");
    for tool_name in [
        "spawn_agent",
        "send_input",
        "list_agents",
        "resume_agent",
        "wait_agent",
        "close_agent",
        "spawn_agents_on_csv",
    ] {
        assert_lacks_tool_name(&tools, tool_name);
    }
    assert_lacks_tool_name(&tools, "request_user_input");
}

#[test]
fn read_only_sessions_omit_mutating_builtin_tools() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::ApplyPatchFreeform);
    features.enable(Feature::Collab);
    features.enable(Feature::SpawnCsv);
    features.normalize_dependencies();
    let available_models = Vec::new();
    let policy = SandboxPolicy::new_read_only_policy();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &policy,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();
    assert_contains_tool_names(&tools, &["context_retrieve"]);
    for tool_name in [
        "apply_patch",
        "update_plan",
        "artifacts",
        "spawn_agent",
        "spawn_agents_on_csv",
        "channel_sync",
        "channel_take",
        "channel_ack",
        "channel_send",
        "meeting_send_chat",
        "meeting_join",
        "meeting_schedule",
    ] {
        assert_lacks_tool_name(&tools, tool_name);
    }
}











#[test]
fn ctox_web_tools_are_hidden_by_default() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: None,
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_web_search"),
        "ctox_web_search should stay hidden unless the typed CTOX web-stack policy enables it"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_browser_automation"),
        "browser automation should stay hidden until the CTOX web stack is active"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_deep_research"),
        "ctox_deep_research should stay hidden unless the typed CTOX web-stack policy enables it"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_web_auth_assist_request"),
        "ctox_web_auth_assist_request should stay hidden unless the typed CTOX web-stack policy enables it"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_browser_context_capture"),
        "ctox_browser_context_capture should stay hidden unless the typed CTOX web-stack policy enables it"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_browser_context_extract"),
        "ctox_browser_context_extract should stay hidden unless the typed CTOX web-stack policy enables it"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool.spec.name() == "ctox_doc_search"),
        "ctox_doc_search should stay hidden unless the typed CTOX doc-stack policy enables it"
    );
}

#[test]
fn ctox_web_stack_registers_only_ctox_web_tools() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::JsRepl);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Live),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    })
    .with_ctox_web_stack_enabled(true);
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();

    assert_contains_tool_names(
        &tools,
        &[
            "ctox_web_search",
            "ctox_web_read",
            "ctox_deep_research",
            "ctox_web_scrape",
            "ctox_web_auth_assist_request",
            "ctox_web_auth_assist_status",
            "ctox_browser_context_capture",
            "ctox_browser_context_extract",
            "ctox_browser_automation",
        ],
    );
    assert!(
        !tools.iter().any(|tool| tool.spec.name() == "web_search"),
        "native web_search must be hidden when the CTOX web stack is enabled"
    );
    assert!(
        !tools.iter().any(|tool| tool.spec.name() == "js_repl"),
        "js_repl must not be separately exposed when CTOX owns browser automation"
    );
    assert!(
        !tools.iter().any(|tool| tool.spec.name() == "js_repl_reset"),
        "js_repl_reset must not be separately exposed when CTOX owns browser automation"
    );
}

#[test]
fn ctox_web_stack_exposes_browser_automation_without_js_repl() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    })
    .with_ctox_web_stack_enabled(true);
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();

    assert_contains_tool_names(
        &tools,
        &[
            "ctox_web_search",
            "ctox_web_read",
            "ctox_deep_research",
            "ctox_web_scrape",
            "ctox_web_auth_assist_request",
            "ctox_web_auth_assist_status",
            "ctox_browser_context_capture",
            "ctox_browser_context_extract",
            "ctox_browser_automation",
        ],
    );
}

#[test]
fn ctox_doc_stack_registers_doc_tools() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    })
    .with_ctox_doc_stack_enabled(true);
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();

    assert_contains_tool_names(&tools, &["ctox_doc_search", "ctox_doc_read"]);
}





#[test]
fn mcp_resource_tools_are_hidden_without_mcp_servers() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();

    assert!(
        !tools.iter().any(|tool| matches!(
            tool.spec.name(),
            "list_mcp_resources" | "list_mcp_resource_templates" | "read_mcp_resource"
        )),
        "MCP resource tools should be omitted when no MCP servers are configured"
    );
}

#[test]
fn mcp_resource_tools_are_included_when_mcp_servers_are_present() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, Some(HashMap::new()), None, &[]).build();

    assert_contains_tool_names(
        &tools,
        &[
            "list_mcp_resources",
            "list_mcp_resource_templates",
            "read_mcp_resource",
        ],
    );
}










#[test]
fn test_build_specs_default_shell_present() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("o3", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Live),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, Some(HashMap::new()), None, &[]).build();

    // Only check the shell variant and a couple of core tools.
    let mut subset = vec!["exec_command", "write_stdin", "update_plan"];
    if let Some(shell_tool) = shell_tool_name(&tools_config) {
        subset.push(shell_tool);
    }
    assert_contains_tool_names(&tools, &subset);
}


#[test]
#[ignore]
fn test_parallel_support_flags() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(&tools_config, None, None, &[]).build();

    assert!(find_tool(&tools, "exec_command").supports_parallel_tool_calls);
    assert!(!find_tool(&tools, "write_stdin").supports_parallel_tool_calls);
    assert!(find_tool(&tools, "grep_files").supports_parallel_tool_calls);
    assert!(find_tool(&tools, "list_dir").supports_parallel_tool_calls);
    assert!(find_tool(&tools, "read_file").supports_parallel_tool_calls);
}


#[test]
fn test_build_specs_mcp_tools_converted() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("o3", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Live),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([(
            "test_server/do_something_cool".to_string(),
            mcp_tool(
                "do_something_cool",
                "Do something cool",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "string_argument": { "type": "string" },
                        "number_argument": { "type": "number" },
                        "object_argument": {
                            "type": "object",
                            "properties": {
                                "string_property": { "type": "string" },
                                "number_property": { "type": "number" },
                            },
                            "required": ["string_property", "number_property"],
                            "additionalProperties": false,
                        },
                    },
                }),
            ),
        )])),
        None,
        &[],
    )
    .build();

    let tool = find_tool(&tools, "test_server/do_something_cool");
    assert_eq!(
        &tool.spec,
        &ToolSpec::Function(ResponsesApiTool {
            name: "test_server/do_something_cool".to_string(),
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([
                    (
                        "string_argument".to_string(),
                        JsonSchema::String { description: None }
                    ),
                    (
                        "number_argument".to_string(),
                        JsonSchema::Number {
                            description: None,
                            minimum: None,
                            maximum: None,
                        }
                    ),
                    (
                        "object_argument".to_string(),
                        JsonSchema::Object {
                            properties: BTreeMap::from([
                                (
                                    "string_property".to_string(),
                                    JsonSchema::String { description: None }
                                ),
                                (
                                    "number_property".to_string(),
                                    JsonSchema::Number {
                                        description: None,
                                        minimum: None,
                                        maximum: None,
                                    }
                                ),
                            ]),
                            required: Some(vec![
                                "string_property".to_string(),
                                "number_property".to_string(),
                            ]),
                            additional_properties: Some(false.into()),
                        },
                    ),
                ]),
                required: None,
                additional_properties: None,
            },
            description: "Do something cool".to_string(),
            strict: false,
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({}))),
            defer_loading: None,
        })
    );
}

#[test]
fn test_build_specs_mcp_tools_sorted_by_name() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("o3", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    // Intentionally construct a map with keys that would sort alphabetically.
    let tools_map: HashMap<String, rmcp::model::Tool> = HashMap::from([
        (
            "test_server/do".to_string(),
            mcp_tool("a", "a", serde_json::json!({"type": "object"})),
        ),
        (
            "test_server/something".to_string(),
            mcp_tool("b", "b", serde_json::json!({"type": "object"})),
        ),
        (
            "test_server/cool".to_string(),
            mcp_tool("c", "c", serde_json::json!({"type": "object"})),
        ),
    ]);

    let (tools, _) = build_specs(&tools_config, Some(tools_map), None, &[]).build();

    // Only assert that the MCP tools themselves are sorted by fully-qualified name.
    let mcp_names: Vec<_> = tools
        .iter()
        .map(|t| tool_name(&t.spec).to_string())
        .filter(|n| n.starts_with("test_server/"))
        .collect();
    let expected = vec![
        "test_server/cool".to_string(),
        "test_server/do".to_string(),
        "test_server/something".to_string(),
    ];
    assert_eq!(mcp_names, expected);
}








#[test]
fn test_mcp_tool_property_missing_type_defaults_to_string() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([(
            "dash/search".to_string(),
            mcp_tool(
                "search",
                "Search docs",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"description": "search query"}
                    }
                }),
            ),
        )])),
        None,
        &[],
    )
    .build();

    let tool = find_tool(&tools, "dash/search");
    assert_eq!(
        tool.spec,
        ToolSpec::Function(ResponsesApiTool {
            name: "dash/search".to_string(),
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "query".to_string(),
                    JsonSchema::String {
                        description: Some("search query".to_string())
                    }
                )]),
                required: None,
                additional_properties: None,
            },
            description: "Search docs".to_string(),
            strict: false,
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({}))),
            defer_loading: None,
        })
    );
}

#[test]
fn test_mcp_tool_integer_normalized_to_number() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([(
            "dash/paginate".to_string(),
            mcp_tool(
                "paginate",
                "Pagination",
                serde_json::json!({
                    "type": "object",
                    "properties": {"page": {"type": "integer"}}
                }),
            ),
        )])),
        None,
        &[],
    )
    .build();

    let tool = find_tool(&tools, "dash/paginate");
    assert_eq!(
        tool.spec,
        ToolSpec::Function(ResponsesApiTool {
            name: "dash/paginate".to_string(),
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "page".to_string(),
                    JsonSchema::Number {
                        description: None,
                        minimum: None,
                        maximum: None,
                    }
                )]),
                required: None,
                additional_properties: None,
            },
            description: "Pagination".to_string(),
            strict: false,
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({}))),
            defer_loading: None,
        })
    );
}

#[test]
fn test_mcp_tool_array_without_items_gets_default_string_items() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    features.enable(Feature::ApplyPatchFreeform);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([(
            "dash/tags".to_string(),
            mcp_tool(
                "tags",
                "Tags",
                serde_json::json!({
                    "type": "object",
                    "properties": {"tags": {"type": "array"}}
                }),
            ),
        )])),
        None,
        &[],
    )
    .build();

    let tool = find_tool(&tools, "dash/tags");
    assert_eq!(
        tool.spec,
        ToolSpec::Function(ResponsesApiTool {
            name: "dash/tags".to_string(),
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "tags".to_string(),
                    JsonSchema::Array {
                        items: Box::new(JsonSchema::String { description: None }),
                        description: None
                    }
                )]),
                required: None,
                additional_properties: None,
            },
            description: "Tags".to_string(),
            strict: false,
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({}))),
            defer_loading: None,
        })
    );
}

#[test]
fn test_mcp_tool_anyof_defaults_to_string() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([(
            "dash/value".to_string(),
            mcp_tool(
                "value",
                "AnyOf Value",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": {"anyOf": [{"type": "string"}, {"type": "number"}]}
                    }
                }),
            ),
        )])),
        None,
        &[],
    )
    .build();

    let tool = find_tool(&tools, "dash/value");
    assert_eq!(
        tool.spec,
        ToolSpec::Function(ResponsesApiTool {
            name: "dash/value".to_string(),
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "value".to_string(),
                    JsonSchema::String { description: None }
                )]),
                required: None,
                additional_properties: None,
            },
            description: "AnyOf Value".to_string(),
            strict: false,
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({}))),
            defer_loading: None,
        })
    );
}





#[test]
fn test_get_openai_tools_mcp_tools_with_additional_properties_schema() {
    let config = test_config();
    let model_info = ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    let mut features = Features::with_defaults();
    features.enable(Feature::UnifiedExec);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([(
            "test_server/do_something_cool".to_string(),
            mcp_tool(
                "do_something_cool",
                "Do something cool",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "string_argument": {"type": "string"},
                        "number_argument": {"type": "number"},
                        "object_argument": {
                            "type": "object",
                            "properties": {
                                "string_property": {"type": "string"},
                                "number_property": {"type": "number"}
                            },
                            "required": ["string_property", "number_property"],
                            "additionalProperties": {
                                "type": "object",
                                "properties": {
                                    "addtl_prop": {"type": "string"}
                                },
                                "required": ["addtl_prop"],
                                "additionalProperties": false
                            }
                        }
                    }
                }),
            ),
        )])),
        None,
        &[],
    )
    .build();

    let tool = find_tool(&tools, "test_server/do_something_cool");
    assert_eq!(
        tool.spec,
        ToolSpec::Function(ResponsesApiTool {
            name: "test_server/do_something_cool".to_string(),
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([
                    (
                        "string_argument".to_string(),
                        JsonSchema::String { description: None }
                    ),
                    (
                        "number_argument".to_string(),
                        JsonSchema::Number {
                            description: None,
                            minimum: None,
                            maximum: None,
                        }
                    ),
                    (
                        "object_argument".to_string(),
                        JsonSchema::Object {
                            properties: BTreeMap::from([
                                (
                                    "string_property".to_string(),
                                    JsonSchema::String { description: None }
                                ),
                                (
                                    "number_property".to_string(),
                                    JsonSchema::Number {
                                        description: None,
                                        minimum: None,
                                        maximum: None,
                                    }
                                ),
                            ]),
                            required: Some(vec![
                                "string_property".to_string(),
                                "number_property".to_string(),
                            ]),
                            additional_properties: Some(
                                JsonSchema::Object {
                                    properties: BTreeMap::from([(
                                        "addtl_prop".to_string(),
                                        JsonSchema::String { description: None }
                                    ),]),
                                    required: Some(vec!["addtl_prop".to_string(),]),
                                    additional_properties: Some(false.into()),
                                }
                                .into()
                            ),
                        },
                    ),
                ]),
                required: None,
                additional_properties: None,
            },
            description: "Do something cool".to_string(),
            strict: false,
            output_schema: Some(mcp_call_tool_result_output_schema(serde_json::json!({}))),
            defer_loading: None,
        })
    );
}
