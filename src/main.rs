use anyhow::Context;
use std::path::{Path, PathBuf};

mod capabilities;
mod context;
mod doc_stack;
mod execution;
mod install;
mod mission;
mod secrets;
mod service;
mod ui;
mod web_stack;

pub use capabilities::browser;
pub use capabilities::doc;
pub use capabilities::scrape;
pub use capabilities::web;
pub use context::context_health;
pub use context::context_stress;
pub use context::lcm;
pub use context::live_context;
pub use install::version_info;
pub use mission::channels;
pub use mission::follow_up;
pub use mission::plan;
pub use mission::queue;
pub use mission::review;
pub use mission::schedule;
pub use mission::tickets;
pub use mission::verification;
pub use service::governance;
pub use service::mission_governor;
pub use service::state_invariants;
pub use ui::tui;

pub mod inference {
    pub use crate::execution::agent::turn_engine;
    pub use crate::execution::agent::turn_loop;
    pub use crate::execution::models::engine;
    pub use crate::execution::models::litert_bridge;
    pub use crate::execution::models::model_adapters;
    pub use crate::execution::models::model_manifest;
    pub use crate::execution::models::model_registry;
    pub use crate::execution::models::resource_state;
    pub use crate::execution::models::runtime_contract;
    pub use crate::execution::models::runtime_control;
    pub use crate::execution::models::runtime_engine_guard;
    pub use crate::execution::models::runtime_env;
    pub use crate::execution::models::runtime_gpu_manager;
    pub use crate::execution::models::runtime_kernel;
    pub use crate::execution::models::runtime_plan;
    pub use crate::execution::models::runtime_state;
    pub use crate::execution::models::supervisor;
    pub use crate::execution::models::turn_contract;
    pub use crate::execution::responses::gateway;
    pub use crate::execution::responses::web_search;
}

use crate::inference::engine;
use crate::inference::gateway;
use crate::inference::model_registry;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_plan;
use crate::inference::runtime_state;

#[cfg(test)]
#[path = "model_catalog_boundary_tests.rs"]
mod model_catalog_boundary_tests;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = resolve_workspace_root()?;

    match args.first().map(String::as_str) {
        None => tui::run_tui(&root),
        Some("source-status") => {
            let outcome = engine::source_layout_status(&root)?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Some("clean-room-baseline-plan") => {
            let family = args
                .get(1)
                .map(String::as_str)
                .unwrap_or(model_registry::default_local_chat_family_selector())
                .parse()?;
            let prompt = if args.len() > 2 {
                args[2..].join(" ")
            } else {
                "Reply with CTOX_BASELINE_OK and nothing else.".to_string()
            };
            let plan = engine::build_clean_room_baseline_plan(&root, family, prompt);
            println!("{}", serde_json::to_string_pretty(&plan)?);
            Ok(())
        }
        Some("clean-room-rewrite-responses") => {
            let input_path = args
                .get(1)
                .context("usage: ctox clean-room-rewrite-responses <json-path>")?;
            let raw = std::fs::read(input_path)
                .with_context(|| format!("failed to read responses payload from {}", input_path))?;
            let rewritten = engine::rewrite_engine_responses_request(&raw)?;
            println!("{}", String::from_utf8_lossy(&rewritten));
            Ok(())
        }
        Some("runtime") => match args.get(1).map(String::as_str) {
            Some("switch") => {
                let model = args
                    .get(2)
                    .context("usage: ctox runtime switch <model> <quality|performance>")?;
                let preset = args
                    .get(3)
                    .context("usage: ctox runtime switch <model> <quality|performance>")?;
                let outcome = runtime_control::execute_runtime_switch(&root, model, Some(preset))?;
                if let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(&root)? {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                } else {
                    let state = runtime_state::load_or_resolve_runtime_state(&root)?;
                    println!("{}", serde_json::to_string_pretty(&state)?);
                }
                eprintln!(
                    "ctox runtime switch requested model={} active_model={} phase={:?}",
                    model, outcome.active_model, outcome.phase
                );
                Ok(())
            }
            _ => anyhow::bail!("usage: ctox runtime switch <model> <quality|performance>"),
        },
        Some("serve-responses-proxy") => {
            let config = gateway::ProxyConfig::resolve_with_root(&root);
            eprintln!("{}", serde_json::to_string_pretty(&config)?);
            gateway::serve_proxy(config)
        }
        Some("serve-litert-bridge") => {
            let config_path = find_flag_value(&args[1..], "--config")
                .context("usage: ctox serve-litert-bridge --config <json-path>")?;
            inference::litert_bridge::serve_from_config_path(&root, Path::new(config_path))
        }
        Some("boost") => match args.get(1).map(String::as_str) {
            Some("status") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&gateway::boost_status(&root)?)?
                );
                Ok(())
            }
            Some("start") => {
                let minutes = find_flag_value(&args[2..], "--minutes")
                    .and_then(|value| value.parse::<u64>().ok());
                let model = find_flag_value(&args[2..], "--model");
                let reason = find_flag_value(&args[2..], "--reason");
                let result = gateway::start_boost_lease(
                    &root,
                    model,
                    minutes,
                    reason,
                )?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            Some("stop") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&gateway::stop_boost_lease(&root)?)?
                );
                Ok(())
            }
            _ => anyhow::bail!(
                "usage: ctox boost status | ctox boost start [--minutes <n>] [--model <id>] [--reason <text>] | ctox boost stop"
            ),
        },
        Some("service") => {
            let flags: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
            let foreground = flags.contains(&"--foreground");
            let auto_approve = flags.contains(&"--auto-approve-gates");
            if !foreground {
                anyhow::bail!(
                    "usage: ctox service --foreground [--auto-approve-gates]"
                );
            }
            if auto_approve {
                // Propagate to downstream processes (codex-exec, child turns).
                std::env::set_var("CTOX_AUTO_APPROVE_GATES", "1");
            }
            service::run_foreground(&root)
        }
        Some("version") => {
            let version = version_info(&root)?;
            println!("{}", serde_json::to_string_pretty(&version)?);
            Ok(())
        }
        Some("start") => {
            println!("{}", service::start_background(&root)?);
            Ok(())
        }
        Some("stop") => {
            println!("{}", service::stop_background(&root)?);
            Ok(())
        }
        Some("status") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&service::service_status_snapshot(&root)?)?
            );
            Ok(())
        }
        Some("tui") => tui::run_tui(&root),
        Some("tui-smoke") => {
            let page = args.get(1).map(String::as_str).unwrap_or("chat");
            let width: u16 = args
                .get(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(120);
            let height: u16 = args
                .get(3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(40);
            tui::run_tui_smoke(&root, page, width, height)
        }
        Some("browser") => browser::handle_browser_command(&root, &args[1..]),
        Some("channel") => channels::handle_channel_command(&root, &args[1..]),
        Some("doc") => doc::handle_doc_command(&root, &args[1..]),
        Some("follow-up") => follow_up::handle_follow_up_command(&args[1..]),
        Some("governance") => governance::handle_governance_command(&root, &args[1..]),
        Some("jami-daemon") => mission::communication_jami_native::handle_daemon_command(&root, &args[1..]),
        Some("meeting") => mission::communication_meeting_native::handle_meeting_command(&root, &args[1..]),
        Some("plan") => plan::handle_plan_command(&root, &args[1..]),
        Some("queue") => queue::handle_queue_command(&root, &args[1..]),
        Some("scrape") => scrape::handle_scrape_command(&root, &args[1..]),
        Some("secret") => secrets::handle_secret_command(&root, &args[1..]),
        Some("schedule") => schedule::handle_schedule_command(&root, &args[1..]),
        Some("ticket") => tickets::handle_ticket_command(&root, &args[1..]),
        Some("web") => web::handle_web_command(&root, &args[1..]),
        Some("verification") => verification::handle_verification_command(&root, &args[1..]),
        Some("state-invariants") => state_invariants::handle_state_invariants_command(&root, &args[1..]),
        Some("update") | Some("upgrade") => install::handle_update_command(&root, &args[1..]),
        Some("lcm-init") => {
            let db_path = args.get(1).context("usage: ctox lcm-init <db-path>")?;
            lcm::run_init(PathBuf::from(db_path).as_path())
        }
        Some("lcm-add-message") => {
            let db_path = args.get(1).context(
                "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context(
                    "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
                )?
                .parse()
                .context("failed to parse conversation id")?;
            let role = args.get(3).context(
                "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
            )?;
            let content = args
                .get(4..)
                .filter(|parts| !parts.is_empty())
                .map(|parts| parts.join(" "))
                .context(
                    "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
                )?;
            let message = lcm::run_add_message(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                role,
                &content,
            )?;
            println!("{}", serde_json::to_string_pretty(&message)?);
            Ok(())
        }
        Some("lcm-compact") => {
            let db_path = args.get(1).context(
                "usage: ctox lcm-compact <db-path> <conversation-id> [token-budget] [--force]",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context(
                    "usage: ctox lcm-compact <db-path> <conversation-id> [token-budget] [--force]",
                )?
                .parse()
                .context("failed to parse conversation id")?;
            let token_budget = args
                .get(3)
                .filter(|value| !value.starts_with("--"))
                .map(|value| value.parse())
                .transpose()
                .context("failed to parse token budget")?
                .unwrap_or(24_000_i64);
            let force = args.iter().any(|arg| arg == "--force");
            let result = lcm::run_compact(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                token_budget,
                force,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-grep") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let conversation_arg = args
                .get(2)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let conversation_id = if conversation_arg == "all" {
                None
            } else {
                Some(
                    conversation_arg
                        .parse()
                        .context("failed to parse conversation id")?,
                )
            };
            let scope = args
                .get(3)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let mode = args
                .get(4)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let tail = args
                .get(5..)
                .filter(|parts| !parts.is_empty())
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let (query, limit) = if let Some(last) = tail.last() {
                if let Ok(limit) = last.parse::<usize>() {
                    (tail[..tail.len().saturating_sub(1)].join(" "), limit)
                } else {
                    (tail.join(" "), 20_usize)
                }
            } else {
                anyhow::bail!("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]");
            };
            let result = lcm::run_grep(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                scope,
                mode,
                &query,
                limit,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-describe") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-describe <db-path> <summary-id>")?;
            let id = args
                .get(2)
                .context("usage: ctox lcm-describe <db-path> <summary-id>")?;
            let result = lcm::run_describe(PathBuf::from(db_path).as_path(), id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-expand") => {
            let db_path = args.get(1).context(
                "usage: ctox lcm-expand <db-path> <summary-id> [depth] [--messages] [token-cap]",
            )?;
            let summary_id = args.get(2).context(
                "usage: ctox lcm-expand <db-path> <summary-id> [depth] [--messages] [token-cap]",
            )?;
            let numeric_args = args
                .iter()
                .skip(3)
                .filter(|value| !value.starts_with("--"))
                .collect::<Vec<_>>();
            let depth = numeric_args
                .first()
                .map(|value| value.parse())
                .transpose()
                .context("failed to parse depth")?
                .unwrap_or(1_usize);
            let include_messages = args.iter().any(|arg| arg == "--messages");
            let token_cap = numeric_args
                .get(1)
                .map(|value| value.parse())
                .transpose()
                .context("failed to parse token cap")?
                .unwrap_or(8_000_i64);
            let result = lcm::run_expand(
                PathBuf::from(db_path).as_path(),
                summary_id,
                depth,
                include_messages,
                token_cap,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-dump") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-dump <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox lcm-dump <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result = lcm::run_dump(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-refresh-continuity") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-refresh-continuity <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox lcm-refresh-continuity <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result =
                lcm::run_refresh_continuity(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-show-continuity") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-show-continuity <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox lcm-show-continuity <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result =
                lcm::run_show_continuity(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-run-fixture") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-run-fixture <db-path> <fixture-path>")?;
            let fixture_path = args
                .get(2)
                .context("usage: ctox lcm-run-fixture <db-path> <fixture-path>")?;
            let result = lcm::run_fixture(
                PathBuf::from(db_path).as_path(),
                PathBuf::from(fixture_path).as_path(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-init") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-init <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-init <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result =
                lcm::run_continuity_init(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-show") => {
            let db_path = args.get(1).context(
                "usage: ctox continuity-show <db-path> <conversation-id> [narrative|anchors|focus]",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-show <db-path> <conversation-id> [narrative|anchors|focus]")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args.get(3).map(String::as_str);
            let result =
                lcm::run_continuity_show(PathBuf::from(db_path).as_path(), conversation_id, kind)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-apply") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args
                .get(3)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?;
            let diff_path = args
                .get(4)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?;
            let result = lcm::run_continuity_apply(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
                PathBuf::from(diff_path).as_path(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-log") => {
            let db_path = args.get(1).context(
                "usage: ctox continuity-log <db-path> <conversation-id> [narrative|anchors|focus]",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-log <db-path> <conversation-id> [narrative|anchors|focus]")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args.get(3).map(String::as_str);
            let result =
                lcm::run_continuity_log(PathBuf::from(db_path).as_path(), conversation_id, kind)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-rebuild") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args
                .get(3)
                .context("usage: ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let result = lcm::run_continuity_rebuild(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-forgotten") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-forgotten <db-path> <conversation-id> [narrative|anchors|focus] [query]")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-forgotten <db-path> <conversation-id> [narrative|anchors|focus] [query]")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args.get(3).map(String::as_str);
            let query = args
                .get(4..)
                .filter(|parts| !parts.is_empty())
                .map(|parts| parts.join(" "));
            let result = lcm::run_continuity_forgotten(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
                query.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-build-prompt") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args
                .get(3)
                .context("usage: ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let result = lcm::run_continuity_build_prompt(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("context-health") => {
            let db_path = args
                .get(1)
                .context("usage: ctox context-health <db-path> <conversation-id> [latest-user-prompt] [token-budget]")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox context-health <db-path> <conversation-id> [latest-user-prompt] [token-budget]")?
                .parse()
                .context("failed to parse conversation id")?;
            let tail = args.get(3..).unwrap_or(&[]);
            let (latest_prompt, token_budget) = if let Some(last) = tail.last() {
                if let Ok(token_budget) = last.parse::<i64>() {
                    (
                        (!tail[..tail.len().saturating_sub(1)].is_empty())
                            .then(|| tail[..tail.len().saturating_sub(1)].join(" ")),
                        token_budget,
                    )
                } else {
                    (Some(tail.join(" ")), 131_072_i64)
                }
            } else {
                (None, 131_072_i64)
            };
            let result = context_health::assess_for_conversation(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                token_budget,
                latest_prompt.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("chat-prompt-export") => {
            let conversation_id: i64 = find_flag_value(&args[1..], "--conversation-id")
                .unwrap_or("1")
                .parse()
                .context("failed to parse conversation id")?;
            let db_path = find_flag_value(&args[1..], "--db")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("runtime/ctox_lcm.db"));
            let output_path = find_flag_value(&args[1..], "--output").map(PathBuf::from);
            let settings = runtime_env::effective_runtime_env_map(&root).unwrap_or_default();
            let model = runtime_env::effective_chat_model_from_map(&settings)
                .unwrap_or_else(|| model_registry::default_local_chat_model().to_string());
            let artifact = live_context::render_live_prompt_artifact(
                &root,
                &settings,
                &model,
                db_path.as_path(),
                conversation_id,
            )?;
            let markdown = artifact.to_review_markdown();
            if let Some(path) = output_path {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create parent directory for {}", path.display())
                    })?;
                }
                std::fs::write(&path, markdown)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "conversation_id": artifact.conversation_id,
                        "model": artifact.model,
                        "output_path": path,
                        "breakdown": artifact.breakdown,
                    }))?
                );
            } else {
                println!("{markdown}");
            }
            Ok(())
        }
        Some("context-stress") => {
            let db_path = args
                .get(1)
                .context("usage: ctox context-stress <db-path> [conversation-id] [iterations] [token-budget]")?;
            let conversation_id = args
                .get(2)
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse conversation id")?;
            let iterations = args
                .get(3)
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse iterations")?;
            let token_budget = args
                .get(4)
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse token budget")?;
            let result = context_stress::run_context_stress(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                iterations,
                token_budget,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("context-retrieve") => {
            let conversation_id: i64 = find_flag_value(&args[1..], "--conversation-id")
                .unwrap_or("1")
                .parse()
                .context("failed to parse conversation id")?;
            let mode = find_flag_value(&args[1..], "--mode").unwrap_or("current");
            let db_path = find_flag_value(&args[1..], "--db")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("runtime/ctox_lcm.db"));
            let query = find_flag_value(&args[1..], "--query").map(ToOwned::to_owned);
            let continuity_kind = find_flag_value(&args[1..], "--kind").map(ToOwned::to_owned);
            let summary_id = find_flag_value(&args[1..], "--summary-id").map(ToOwned::to_owned);
            let limit = find_flag_value(&args[1..], "--limit")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse limit")?
                .unwrap_or(10);
            let depth = find_flag_value(&args[1..], "--depth")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse depth")?
                .unwrap_or(1);
            let token_cap = find_flag_value(&args[1..], "--token-cap")
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse token cap")?
                .unwrap_or(8_000);
            let include_messages = args.iter().any(|arg| arg == "--messages");
            let result = lcm::run_context_retrieve(
                db_path.as_path(),
                conversation_id,
                mode,
                query.as_deref(),
                continuity_kind.as_deref(),
                summary_id.as_deref(),
                limit,
                depth,
                include_messages,
                token_cap,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        _ => {
            let clean_room_families = model_registry::supported_clean_room_family_selectors().join("|");
            anyhow::bail!(
                "usage:\n  ctox\n  ctox version\n  ctox start\n  ctox stop\n  ctox status\n  ctox service --foreground\n  ctox governance <subcommand> ...\n  ctox state-invariants [--conversation-id <id>]\n  ctox update status\n  ctox update check\n  ctox update channel show\n  ctox update channel set-github --repo <owner/repo> [--api-base <url>] [--token-env <env-var>]\n  ctox update channel clear\n  ctox update adopt [--install-root <path>] [--state-root <path>] [--release <name>] [--skip-build] [--force]\n  ctox update apply --source <path> [--release <name>] [--force] [--keep-failed-release]\n  ctox update apply --latest [--force] [--keep-failed-release]\n  ctox update apply --version <tag> [--force] [--keep-failed-release]\n  ctox update rollback\n  ctox source-status\n  ctox clean-room-baseline-plan <{}> [prompt]\n  ctox clean-room-rewrite-responses <json-path>\n  ctox runtime switch <model> <quality|performance>\n  ctox serve-responses-proxy\n  ctox serve-litert-bridge --config <json-path>\n  ctox boost status\n  ctox boost start [--minutes <n>] [--model <id>] [--reason <text>]\n  ctox boost stop\n  ctox tui\n  ctox channel <subcommand> ...\n  ctox follow-up <subcommand> ...\n  ctox plan <subcommand> ...\n  ctox schedule <subcommand> ...\n  ctox secret <subcommand> ...\n  ctox ticket <subcommand> ...\n  ctox lcm-init <db-path>\n  ctox lcm-add-message <db-path> <conversation-id> <role> <content>\n  ctox lcm-compact <db-path> <conversation-id> [token-budget] [--force]\n  ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]\n  ctox lcm-describe <db-path> <summary-id>\n  ctox lcm-expand <db-path> <summary-id> [depth] [--messages] [token-cap]\n  ctox lcm-dump <db-path> <conversation-id>\n  ctox lcm-refresh-continuity <db-path> <conversation-id>\n  ctox lcm-show-continuity <db-path> <conversation-id>\n  ctox lcm-run-fixture <db-path> <fixture-path>\n  ctox continuity-init <db-path> <conversation-id>\n  ctox continuity-show <db-path> <conversation-id> [narrative|anchors|focus]\n  ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>\n  ctox continuity-log <db-path> <conversation-id> [narrative|anchors|focus]\n  ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>\n  ctox continuity-forgotten <db-path> <conversation-id> [narrative|anchors|focus] [query]\n  ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>\n  ctox context-health <db-path> <conversation-id> [latest-user-prompt] [token-budget]\n  ctox chat-prompt-export [--db <path>] [--conversation-id <id>] [--output <path>]\n  ctox context-stress <db-path> [conversation-id] [iterations] [token-budget]\n  ctox context-retrieve [--db <path>] [--conversation-id <id>] --mode <current|continuity|forgotten|search|describe|expand> [--kind <narrative|anchors|focus>] [--query <text>] [--summary-id <id>] [--limit <n>] [--depth <n>] [--messages] [--token-cap <n>]",
                clean_room_families
            )
        }
    }
}

fn resolve_workspace_root() -> anyhow::Result<PathBuf> {
    if let Some(root) = validated_workspace_root_override("CTOX_ROOT") {
        return Ok(root);
    }
    let current_dir = std::env::current_dir().context("failed to resolve CTOX workspace root")?;
    if let Some(root) = find_ctox_root_from_ancestors(&current_dir) {
        return Ok(root);
    }
    if let Some(root) = validated_workspace_root_override("CTOX_HOME") {
        return Ok(root);
    }
    let current_exe = std::env::current_exe().ok();
    if let Some(root) = current_exe
        .as_deref()
        .and_then(|exe| resolve_runtime_ctox_root(exe, home_dir().as_deref()))
    {
        return Ok(root);
    }
    Ok(current_dir)
}

fn validated_workspace_root_override(key: &str) -> Option<PathBuf> {
    let candidate = std::env::var_os(key).map(PathBuf::from)?;
    looks_like_ctox_root(&candidate).then_some(candidate)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn find_ctox_root_from_ancestors(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if looks_like_ctox_root(candidate) {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn resolve_runtime_ctox_root(current_exe: &Path, home_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(root) = find_ctox_root_from_ancestors(current_exe) {
        return Some(root);
    }
    let Some(home_dir) = home_dir else {
        return None;
    };
    let default_current_root = home_dir.join(".local/lib/ctox/current");
    if looks_like_ctox_root(&default_current_root) {
        return Some(default_current_root);
    }
    resolve_systemd_user_ctox_root(home_dir)
}

fn looks_like_ctox_root(candidate: &Path) -> bool {
    candidate.join("Cargo.toml").is_file()
        && candidate.join("src/main.rs").is_file()
        && candidate
            .join("contracts/history/creation-ledger.md")
            .is_file()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn resolve_systemd_user_ctox_root(home_dir: &Path) -> Option<PathBuf> {
    let service_file = home_dir.join(".config/systemd/user/ctox.service");
    let contents = std::fs::read_to_string(service_file).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Environment=CTOX_ROOT=") {
            let candidate = PathBuf::from(value.trim());
            if looks_like_ctox_root(&candidate) {
                return Some(candidate);
            }
        }
        if let Some(value) = trimmed.strip_prefix("WorkingDirectory=") {
            let candidate = PathBuf::from(value.trim());
            if looks_like_ctox_root(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        find_ctox_root_from_ancestors, looks_like_ctox_root, resolve_runtime_ctox_root,
        validated_workspace_root_override,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn finds_ctox_root_from_nested_workspace_directory() {
        let root = make_fake_ctox_root("root-ancestor");
        let nested = root.join("src/context");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_ctox_root_from_ancestors(&nested), Some(root.clone()));
        cleanup_test_dir(&root);
    }

    #[test]
    fn resolves_default_install_root_when_binary_runs_without_wrapper_env() {
        let home = unique_test_dir("installed-home");
        let install_root = home.join(".local/lib/ctox/current");
        create_fake_ctox_root(&install_root);
        let exe = home.join(".local/bin/ctox");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"binary").unwrap();

        assert_eq!(
            resolve_runtime_ctox_root(&exe, Some(home.as_path())),
            Some(install_root.clone())
        );

        cleanup_test_dir(&home);
    }

    #[test]
    fn resolves_systemd_service_root_when_wrapper_env_is_missing() {
        let home = unique_test_dir("systemd-home");
        let service_root = home.join("CTOX");
        create_fake_ctox_root(&service_root);
        let systemd_dir = home.join(".config/systemd/user");
        fs::create_dir_all(&systemd_dir).unwrap();
        fs::write(
            systemd_dir.join("ctox.service"),
            format!(
                "[Service]\nWorkingDirectory={}\nEnvironment=CTOX_ROOT={}\n",
                service_root.display(),
                service_root.display()
            ),
        )
        .unwrap();
        let exe = home.join(".local/bin/ctox");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"binary").unwrap();

        assert_eq!(
            resolve_runtime_ctox_root(&exe, Some(home.as_path())),
            Some(service_root.clone())
        );

        cleanup_test_dir(&home);
    }

    #[test]
    fn rejects_directories_without_ctox_markers() {
        let root = unique_test_dir("not-root");
        fs::create_dir_all(&root).unwrap();

        assert!(!looks_like_ctox_root(&root));

        cleanup_test_dir(&root);
    }

    #[test]
    fn workspace_root_override_accepts_valid_ctox_root_only() {
        let root = make_fake_ctox_root("env-valid-root");
        let invalid = unique_test_dir("env-invalid-root");
        fs::create_dir_all(&invalid).unwrap();

        std::env::set_var("CTOX_HOME", &root);
        assert_eq!(
            validated_workspace_root_override("CTOX_HOME"),
            Some(root.clone())
        );

        std::env::set_var("CTOX_HOME", &invalid);
        assert_eq!(validated_workspace_root_override("CTOX_HOME"), None);

        std::env::remove_var("CTOX_HOME");
        cleanup_test_dir(&root);
        cleanup_test_dir(&invalid);
    }

    fn make_fake_ctox_root(name: &str) -> PathBuf {
        let root = unique_test_dir(name);
        create_fake_ctox_root(&root);
        root
    }

    fn create_fake_ctox_root(root: &Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("contracts/history")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"ctox\"\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            root.join("contracts/history/creation-ledger.md"),
            "# ledger\n",
        )
        .unwrap();
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ctox-main-tests-{name}-{unique}"))
    }

    fn cleanup_test_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }
}
