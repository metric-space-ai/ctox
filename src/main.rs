use anyhow::Context;
use std::path::{Path, PathBuf};

mod autonomy;
mod capabilities;
mod context;
mod doc_stack;
mod execution;
mod export;
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
            if !foreground {
                anyhow::bail!(
                    "usage: ctox service --foreground \
                     [--autonomy progressive|balanced|defensive] \
                     [--auto-approve-gates (deprecated alias for --autonomy progressive)]"
                );
            }
            if let Some(level_str) = find_flag_value(&args[1..], "--autonomy") {
                let level = autonomy::AutonomyLevel::from_str_lossy(level_str);
                std::env::set_var("CTOX_AUTONOMY_LEVEL", level.as_str());
            } else if flags.contains(&"--auto-approve-gates") {
                eprintln!(
                    "warning: --auto-approve-gates is deprecated; use --autonomy progressive"
                );
                std::env::set_var("CTOX_AUTONOMY_LEVEL", "progressive");
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
        Some("run-once") => handle_run_once(&root, &args[1..]),
        _ => {
            let clean_room_families = model_registry::supported_clean_room_family_selectors().join("|");
            anyhow::bail!(
                "usage:\n  ctox\n  ctox version\n  ctox start\n  ctox stop\n  ctox status\n  ctox service --foreground\n  ctox run-once --brief <text> [--model <id>] [--quality|--performance] [--workspace <path>] [--atif-out <path>] [--thread-key <key>]\n  ctox governance <subcommand> ...\n  ctox state-invariants [--conversation-id <id>]\n  ctox update status\n  ctox update check\n  ctox update channel show\n  ctox update channel set-github --repo <owner/repo> [--api-base <url>] [--token-env <env-var>]\n  ctox update channel clear\n  ctox update adopt [--install-root <path>] [--state-root <path>] [--release <name>] [--skip-build] [--force]\n  ctox update apply --source <path> [--release <name>] [--force] [--keep-failed-release]\n  ctox update apply --latest [--force] [--keep-failed-release]\n  ctox update apply --version <tag> [--force] [--keep-failed-release]\n  ctox update rollback\n  ctox source-status\n  ctox clean-room-baseline-plan <{}> [prompt]\n  ctox clean-room-rewrite-responses <json-path>\n  ctox runtime switch <model> <quality|performance>\n  ctox serve-responses-proxy\n  ctox serve-litert-bridge --config <json-path>\n  ctox boost status\n  ctox boost start [--minutes <n>] [--model <id>] [--reason <text>]\n  ctox boost stop\n  ctox tui\n  ctox channel <subcommand> ...\n  ctox follow-up <subcommand> ...\n  ctox plan <subcommand> ...\n  ctox schedule <subcommand> ...\n  ctox secret <subcommand> ...\n  ctox ticket <subcommand> ...\n  ctox lcm-init <db-path>\n  ctox lcm-add-message <db-path> <conversation-id> <role> <content>\n  ctox lcm-compact <db-path> <conversation-id> [token-budget] [--force]\n  ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]\n  ctox lcm-describe <db-path> <summary-id>\n  ctox lcm-expand <db-path> <summary-id> [depth] [--messages] [token-cap]\n  ctox lcm-dump <db-path> <conversation-id>\n  ctox lcm-refresh-continuity <db-path> <conversation-id>\n  ctox lcm-show-continuity <db-path> <conversation-id>\n  ctox lcm-run-fixture <db-path> <fixture-path>\n  ctox continuity-init <db-path> <conversation-id>\n  ctox continuity-show <db-path> <conversation-id> [narrative|anchors|focus]\n  ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>\n  ctox continuity-log <db-path> <conversation-id> [narrative|anchors|focus]\n  ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>\n  ctox continuity-forgotten <db-path> <conversation-id> [narrative|anchors|focus] [query]\n  ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>\n  ctox context-health <db-path> <conversation-id> [latest-user-prompt] [token-budget]\n  ctox chat-prompt-export [--db <path>] [--conversation-id <id>] [--output <path>]\n  ctox context-stress <db-path> [conversation-id] [iterations] [token-budget]\n  ctox context-retrieve [--db <path>] [--conversation-id <id>] --mode <current|continuity|forgotten|search|describe|expand> [--kind <narrative|anchors|focus>] [--query <text>] [--summary-id <id>] [--limit <n>] [--depth <n>] [--messages] [--token-cap <n>]",
                clean_room_families
            )
        }
    }
}

/// `ctox run-once` — drive a single CTOX mission from the initial brief all
/// the way to its done-gate, then emit a trajectory.
///
/// A mission in CTOX is *not* a single LLM call: the agent can enqueue
/// follow-ups, plans can emit new steps between turns, and those extend the
/// mission until the continuity state reaches `is_open == false`. This CLI
/// replicates the inner loop that `start_prompt_worker` runs inside the
/// service daemon, without the service daemon itself — bench harnesses get
/// the full CTOX operating model (Plan → Turn → Follow-up → Turn → … →
/// Done-gate) in a single invocation.
///
/// Exit semantics for harness consumption:
///   0   — mission closed (is_open == false). Done-gate reached.
///   1   — a turn failed (error propagated via `anyhow`).
///   2   — mission blocked (no more pending work but mission still open).
///   4   — turn-cap reached (`--max-turns`, default 30).
fn handle_run_once(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let brief = find_flag_value(args, "--brief").context(
        "usage: ctox run-once --brief <text> [--model <id>] [--quality|--performance] \
         [--workspace <path>] [--atif-out <path>] [--thread-key <key>] [--max-turns <n>] \
         [--autonomy progressive|balanced|defensive] \
         [--auto-approve-gates (deprecated alias for --autonomy progressive)]",
    )?;
    // Autonomy level: benchmark harnesses (Terminal-Bench) set
    // `--autonomy progressive` to skip the owner-approval handshake.
    // Default is whatever the TUI left in CTOX_AUTONOMY_LEVEL (via
    // engine.env), falling back to balanced.
    if let Some(level_str) = find_flag_value(args, "--autonomy") {
        let level = autonomy::AutonomyLevel::from_str_lossy(level_str);
        std::env::set_var("CTOX_AUTONOMY_LEVEL", level.as_str());
    } else if args.iter().any(|arg| arg == "--auto-approve-gates") {
        eprintln!("warning: --auto-approve-gates is deprecated; use --autonomy progressive");
        std::env::set_var("CTOX_AUTONOMY_LEVEL", "progressive");
    }
    let model = find_flag_value(args, "--model");
    let preset = if args.iter().any(|arg| arg == "--quality") {
        Some("quality")
    } else if args.iter().any(|arg| arg == "--performance") {
        Some("performance")
    } else {
        None
    };
    let workspace_opt = find_flag_value(args, "--workspace").map(PathBuf::from);
    let atif_out = find_flag_value(args, "--atif-out").map(PathBuf::from);
    let thread_key = find_flag_value(args, "--thread-key")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("run-once-{}", chrono::Utc::now().timestamp_millis()));
    let max_turns: usize = find_flag_value(args, "--max-turns")
        .map(|v| v.parse::<usize>())
        .transpose()
        .context("failed to parse --max-turns")?
        .unwrap_or(30);

    if let Some(model_id) = model {
        let outcome = runtime_control::execute_runtime_switch(root, model_id, preset)?;
        eprintln!(
            "ctox run-once: model switch requested={} active_model={} phase={:?}",
            model_id, outcome.active_model, outcome.phase
        );
    } else if preset.is_some() {
        anyhow::bail!("--quality/--performance requires --model <id>");
    }

    // If the selected model needs the CTOX gateway proxy for wire-protocol
    // translation (e.g. MiniMax: codex-exec speaks /v1/responses, MiniMax
    // direct only has /v1/chat/completions), spawn the gateway in a
    // background thread so codex-exec has somewhere to talk.
    if let Some(model_id) = model {
        if engine::default_api_provider_for_model(model_id) == "minimax" {
            let root_for_proxy = root.to_path_buf();
            std::thread::Builder::new()
                .name("ctox-run-once-proxy".to_string())
                .spawn(move || {
                    let config = gateway::ProxyConfig::resolve_with_root(&root_for_proxy);
                    eprintln!(
                        "ctox run-once: spawning gateway on {} → upstream={}",
                        config.listen_addr(),
                        config.upstream_base_url
                    );
                    if let Err(err) = gateway::serve_proxy(config) {
                        eprintln!("ctox run-once: gateway exited with error: {err}");
                    }
                })
                .context("failed to spawn gateway thread")?;
            // Wait briefly for the gateway to bind its port.
            let listen_port = runtime_state::load_or_resolve_runtime_state(root)
                .ok()
                .and_then(|state| Some(state.proxy_port))
                .unwrap_or(12434);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                if std::net::TcpStream::connect(("127.0.0.1", listen_port)).is_ok() {
                    eprintln!("ctox run-once: gateway up on 127.0.0.1:{listen_port}");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
    }

    let db_path = root.join("runtime/ctox_lcm.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let conversation_id =
        inference::turn_loop::conversation_id_for_thread_key(Some(thread_key.as_str()));

    eprintln!(
        "ctox run-once: thread_key={} conversation_id={} brief_chars={} max_turns={}",
        thread_key,
        conversation_id,
        brief.chars().count(),
        max_turns
    );

    // Seed the mission with the brief as a queue task so it flows through
    // exactly the same lease/ack machinery that channel-sourced prompts
    // use in the service daemon.
    let initial_title = {
        let mut s: String = brief.chars().take(80).collect();
        if brief.chars().count() > 80 {
            s.push('…');
        }
        format!("run-once: {s}")
    };
    let _seed = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: initial_title,
            prompt: brief.to_string(),
            thread_key: thread_key.clone(),
            workspace_root: workspace_opt
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
        },
    )
    .context("failed to seed initial queue task for run-once mission")?;

    let lease_owner = "run-once";
    let mut mission_status: &str = "open";
    let mut last_error: Option<String> = None;
    let mut last_reply = String::new();
    let mut turns_run: usize = 0;

    let mut current = lease_next_for_thread(root, &thread_key, lease_owner)?;
    if current.is_none() {
        anyhow::bail!(
            "failed to lease newly-created queue task for thread {thread_key}"
        );
    }

    while let Some((prompt_text, leased_keys, ws_override)) = current.take() {
        turns_run += 1;
        let workspace_ref = ws_override.as_deref().or(workspace_opt.as_deref());
        let force_continuity_refresh = leased_keys
            .iter()
            .any(|key| key.starts_with("plan:system::"));

        eprintln!(
            "ctox run-once: turn {}/{} leased={:?} prompt_chars={}",
            turns_run,
            max_turns,
            leased_keys,
            prompt_text.chars().count()
        );

        let turn_result = inference::turn_loop::run_chat_turn_with_events_extended(
            root,
            &db_path,
            &prompt_text,
            workspace_ref,
            conversation_id,
            None,
            force_continuity_refresh,
            |event| eprintln!("[ctox run-once t{turns_run}] {event}"),
        );

        match &turn_result {
            Ok(reply) => {
                last_reply.clone_from(reply);
                if let Err(err) = channels::ack_leased_messages(root, &leased_keys, "handled") {
                    eprintln!("ctox run-once: ack handled failed: {err}");
                }
                for key in &leased_keys {
                    if key.starts_with("plan:system::") {
                        if let Err(err) = plan::complete_step_by_message_key(root, key, reply) {
                            eprintln!(
                                "ctox run-once: complete_step_by_message_key({key}) failed: {err}"
                            );
                        }
                    }
                }
            }
            Err(err) => {
                let err_text = err.to_string();
                last_error = Some(err_text.clone());
                eprintln!("ctox run-once: turn {turns_run} failed: {err_text}");
                let _ = channels::ack_leased_messages(root, &leased_keys, "failed");

                // Service-daemon parity: a turn that hit the runtime time
                // budget is recoverable. CTOX' production behaviour is to
                // enqueue a high-priority continuation slice with a "pick
                // up where you left off" prompt and let the next turn
                // resume from the LCM-persisted state. Without this,
                // run-once misrepresents CTOX in benchmarks — weak models
                // that legitimately need more wall time would get scored
                // as full failures instead of as multi-slice missions.
                if is_turn_timeout_blocker(&err_text) && turns_run < max_turns {
                    match enqueue_timeout_continuation(
                        root,
                        &thread_key,
                        workspace_opt
                            .as_ref()
                            .map(|p| p.to_string_lossy().into_owned())
                            .as_deref(),
                        brief,
                        &err_text,
                        leased_keys.first().map(String::as_str),
                    ) {
                        Ok(title) => {
                            eprintln!(
                                "ctox run-once: enqueued timeout continuation: {title}"
                            );
                            // Fall through: sync mission state, then lease
                            // the freshly enqueued continuation in the
                            // next iteration.
                        }
                        Err(enq_err) => {
                            eprintln!(
                                "ctox run-once: failed to enqueue continuation: {enq_err}"
                            );
                            mission_status = "failed";
                            break;
                        }
                    }
                } else {
                    mission_status = "failed";
                    break;
                }
            }
        }

        // Narrow mid-work detection: only the unambiguous case. An
        // unclosed `<think>` means the reasoning was cut off mid-
        // stream and the model never got to act. Text-pattern matches
        // on intent prefixes ("I'll ...", "Now ...") were too brittle
        // across languages and model phrasings and created downstream
        // false-positives in the tool-activity gate. The system-
        // prompt Mission Control Contract now handles the cooperative
        // case; the heuristic only catches the hard truncation case.
        let last_reply_is_mid_work = turn_result
            .as_ref()
            .ok()
            .map(|r| reply_looks_mid_work(r))
            .unwrap_or(false);
        if last_reply_is_mid_work && turns_run < max_turns {
            match enqueue_midwork_continuation(
                root,
                &thread_key,
                workspace_opt
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .as_deref(),
                brief,
                leased_keys.first().map(String::as_str),
            ) {
                Ok(title) => {
                    eprintln!(
                        "ctox run-once: turn {} ended mid-think — enqueued continuation: {title}",
                        turns_run
                    );
                }
                Err(err) => {
                    eprintln!(
                        "ctox run-once: failed to enqueue mid-work continuation: {err}"
                    );
                }
            }
        }

        // Explicit closure check — only trust `mission_status == "done"`
        // or `continuation_mode in {closed, dormant}`. Do NOT use
        // `is_open=false` on its own: for a fresh mission the Focus
        // document is still empty, which the derivation function
        // reports as `is_open=false` even though the mission just
        // started. The service daemon (service.rs:2320) already treats
        // `is_open` as an idle-detection signal, not a termination
        // signal; run-once now matches.
        let explicitly_closed =
            match lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())
                .and_then(|engine| engine.sync_mission_state_from_continuity(conversation_id))
            {
                Ok(state) => {
                    let status = state.mission_status.to_ascii_lowercase();
                    let mode = state.continuation_mode.to_ascii_lowercase();
                    let closed =
                        status == "done" || mode == "closed" || mode == "dormant";
                    eprintln!(
                        "ctox run-once: after t{} status={} mode={} explicit_closed={} mid_work={}",
                        turns_run, status, mode, closed, last_reply_is_mid_work
                    );
                    closed
                }
                Err(err) => {
                    eprintln!(
                        "ctox run-once: sync_mission_state_from_continuity failed: {err}; treating mission as still open"
                    );
                    false
                }
            };

        if explicitly_closed {
            mission_status = "handled";
            break;
        }

        if turns_run >= max_turns {
            mission_status = "cap";
            break;
        }

        // Let plan steps that are now due emit new plan:system:: messages
        // into the queue. This is the hook the watcher thread normally runs
        // on its timer; run-once triggers it synchronously between turns.
        if let Err(err) = plan::emit_due_steps(root) {
            eprintln!("ctox run-once: plan::emit_due_steps failed: {err}");
        }

        current = lease_next_for_thread(root, &thread_key, lease_owner)?;
        if current.is_none() {
            // No pending queue work AND mission wasn't explicitly
            // closed. This is the "natural end" case — model
            // completed the task, didn't queue follow-ups. We treat
            // it as handled; if the verifier disagrees, that's a
            // model-side fail per the bench rules.
            mission_status = "handled";
            break;
        }
    }

    if let Some(out_path) = atif_out.as_deref() {
        let settings = runtime_env::effective_runtime_env_map(root).unwrap_or_default();
        let model_name = runtime_env::effective_chat_model_from_map(&settings)
            .unwrap_or_else(|| "unknown".to_string());
        let notes = match mission_status {
            "handled" => Some(format!("mission handled in {turns_run} turn(s)")),
            "failed" => Some(format!(
                "mission failed on turn {turns_run}: {}",
                last_error.as_deref().unwrap_or("<no error text>")
            )),
            "blocked" => Some(format!(
                "mission blocked after {turns_run} turn(s): no more pending work but mission still open"
            )),
            "cap" => Some(format!(
                "mission hit --max-turns cap ({max_turns}); mission still open"
            )),
            _ => None,
        };
        let trajectory = export::atif::build_trajectory(
            &db_path,
            conversation_id,
            &thread_key,
            "ctox",
            env!("CARGO_PKG_VERSION"),
            &model_name,
            notes,
        )?;
        export::atif::write_trajectory(&trajectory, out_path)
            .with_context(|| format!("failed to write ATIF trajectory to {}", out_path.display()))?;
        eprintln!(
            "ctox run-once: wrote ATIF trajectory to {}",
            out_path.display()
        );
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": mission_status,
            "conversation_id": conversation_id,
            "thread_key": thread_key,
            "turns_run": turns_run,
            "reply_chars": last_reply.chars().count(),
            "last_error": last_error,
        }))?
    );

    match mission_status {
        "handled" => Ok(()),
        "failed" => Err(anyhow::anyhow!(
            last_error.unwrap_or_else(|| "mission failed".to_string())
        )),
        "blocked" => std::process::exit(2),
        "cap" => std::process::exit(4),
        _ => std::process::exit(5),
    }
}

/// Detect whether an error from `run_chat_turn_with_events_extended` was
/// caused by hitting the per-turn time budget. Mirrors the heuristic used
/// by `service::is_turn_timeout_blocker` so run-once and the service stay
/// in agreement on what counts as a recoverable timeout.
fn is_turn_timeout_blocker(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("timed out after") || lowered.contains("time budget")
}

/// Detect the one unambiguous mid-work signal: a reply that contains an
/// opening `<think>` tag but no matching close tag. This means the
/// reasoning block was cut off mid-stream (upstream truncation, token
/// cap, connection drop inside the stream, …) and the model never got
/// to act on its thinking.
///
/// Earlier versions of this heuristic also pattern-matched intent
/// prefixes ("I'll ...", "Now ...", etc.) and very short replies
/// without completion keywords. Those were too brittle across model
/// phrasings / languages and produced downstream false-positives
/// (e.g. queuing a continuation after the work was already done, which
/// then tripped the tool-activity gate). The cooperative path is now
/// handled by the Mission Control Contract in the system prompt; this
/// heuristic only catches the hard truncation case.
fn reply_looks_mid_work(reply: &str) -> bool {
    // Signal 1: unclosed `<think>` — the hard-truncation case. The
    // reasoning stream was cut before the model got to act.
    if reply.contains("<think>") && !reply.contains("</think>") {
        return true;
    }
    // Signals 2 + 3 look at the visible tail outside any closed
    // `<think>...</think>` block. Both require absence of a completion
    // keyword so legitimate short finals ("Done.", "Answer written to
    // /app/answer.txt.") stay accepted.
    let visible = strip_closed_think_block(reply);
    let trimmed = visible.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    const COMPLETION_KEYWORDS: &[&str] = &[
        "done",
        "complete",
        "completed",
        "finished",
        "verified",
        "wrote",
        "saved",
        "answer",
        "result",
        "passed",
        "ready",
        "in place",
        "successfully",
    ];
    let has_completion_signal =
        COMPLETION_KEYWORDS.iter().any(|kw| lowered.contains(kw));
    if has_completion_signal {
        return false;
    }
    // Signal 2: last sentence starts with "Now " / "Then " / "Next " —
    // an implicit imperative continuation the model announced but did
    // not carry out. Observed repeatedly with M2.7 on bench tasks.
    let last_sentence = trimmed
        .rsplit(|c: char| c == '.' || c == '!' || c == '\n')
        .find(|s| !s.trim().is_empty())
        .unwrap_or(trimmed)
        .trim()
        .to_ascii_lowercase();
    if last_sentence.starts_with("now ")
        || last_sentence.starts_with("then ")
        || last_sentence.starts_with("next ")
    {
        return true;
    }
    // Signal 3: a very short reply with no completion signal is almost
    // always a partial intent the model didn't follow through on.
    if trimmed.chars().count() < 100 {
        return true;
    }
    false
}

fn strip_closed_think_block(reply: &str) -> String {
    let mut out = String::with_capacity(reply.len());
    let mut rest = reply;
    loop {
        match rest.find("<think>") {
            None => {
                out.push_str(rest);
                break;
            }
            Some(start) => {
                out.push_str(&rest[..start]);
                let after = &rest[start + "<think>".len()..];
                match after.find("</think>") {
                    None => {
                        // Unclosed — Signal 1 already handled this.
                        // Preserve the opening tag so downstream text
                        // isn't silently dropped.
                        out.push_str(&rest[start..]);
                        break;
                    }
                    Some(end) => {
                        rest = &after[end + "</think>".len()..];
                    }
                }
            }
        }
    }
    out
}

/// Mid-work parallel to `enqueue_timeout_continuation`. Queues a follow-up
/// slice asking the model to carry out the action it just announced but
/// didn't yet execute.
fn enqueue_midwork_continuation(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
    original_brief: &str,
    parent_message_key: Option<&str>,
) -> anyhow::Result<String> {
    let goal_clip: String = original_brief.chars().take(60).collect();
    let title = format!("Continue mid-work: {goal_clip}");
    let prompt = format!(
        "Your previous turn ended after announcing an intent (\"I'll ...\", \
         \"Let me ...\", etc.) without carrying it out. The mission is not \
         complete. Continue from where you left off:\n\n\
         Current task:\n{}\n\n\
         Required actions:\n\
         - carry out the action you just announced\n\
         - make concrete tool calls (file edits, shell commands, tests) — \
         do not merely describe what you plan to do\n\
         - the mission only ends when the required files or state are in \
         place and verified\n\
         - if more than one turn is still needed, leave exactly one open \
         CTOX plan or queue item before the turn ends",
        original_brief
    );
    let view = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title,
            prompt,
            thread_key: thread_key.to_string(),
            workspace_root: workspace_root.map(str::to_owned),
            priority: "high".to_string(),
            suggested_skill: None,
            parent_message_key: parent_message_key.map(str::to_owned),
        },
    )?;
    Ok(view.title)
}

#[cfg(test)]
mod run_once_tests {
    use super::*;

    #[test]
    fn mid_work_detects_unclosed_think() {
        assert!(reply_looks_mid_work("<think>I'm still analyzing the problem"));
    }

    #[test]
    fn mid_work_detects_now_imperative_after_closed_think() {
        // Regression: bench smoke run smk-cnt-m2-1776261892. M2.7 closed
        // its <think> block and then said "Now let me load the metadata
        // subset..." — that is mid-work, not a final answer. Without
        // this signal the mission terminated after turn 1 with zero
        // tool activity.
        assert!(reply_looks_mid_work(
            "<think>yes</think>\n\nNow let me load the metadata subset and inspect the full structure."
        ));
    }

    #[test]
    fn mid_work_detects_short_reply_without_completion_keyword() {
        // Regression: bench smoke run smk-git-m2-1776261892. M2.7 reply
        // "<think>yes</think>\n\nConfig is valid. Now let me start nginx."
        // — short, imperative-only, no completion signal.
        assert!(reply_looks_mid_work(
            "<think>yes</think>\n\nConfig is valid. Now let me start nginx."
        ));
    }

    #[test]
    fn mid_work_accepts_closed_think_with_completion_keyword_in_tail() {
        // Long final answer with a completion keyword stays accepted
        // even if it mentions "now" elsewhere.
        assert!(!reply_looks_mid_work(
            "<think>counting</think>\n\nDone. The tokenizer run is complete and the answer 79586 is written to /app/answer.txt."
        ));
    }

    #[test]
    fn mid_work_accepts_real_completion() {
        assert!(!reply_looks_mid_work(
            "<think>counted</think>\n\nThere are 79586 deepseek tokens. The answer has been written to /app/answer.txt."
        ));
    }

    #[test]
    fn mid_work_accepts_plain_final_answer() {
        assert!(!reply_looks_mid_work(
            "The required file /app/vm.js is in place and verified against the reference frame. Done."
        ));
    }

    #[test]
    fn mid_work_accepts_reply_without_think() {
        // Empty reply: no signals to trip.
        assert!(!reply_looks_mid_work(""));
        // Long reply without think, no imperative prefix, no completion
        // keyword — still not flagged (we don't infer mid-work from
        // length alone above the 100-char threshold).
        assert!(!reply_looks_mid_work(
            "The request was unambiguous and I have carried it out exactly as specified, though I will not repeat the full description here."
        ));
        // Short reply WITH completion keyword stays accepted.
        assert!(!reply_looks_mid_work("Done."));
        assert!(!reply_looks_mid_work("Answer saved."));
    }
}

/// Mirrors the recovery path that `service::maybe_enqueue_timeout_continuation`
/// runs inside the daemon: enqueue a high-priority follow-up slice with a
/// "continue from the latest saved state" prompt. The next mission-loop
/// iteration leases this continuation and resumes the mission with the
/// existing LCM context intact.
///
/// We deliberately keep this thin (no governance event recording) since
/// run-once is single-mission and doesn't need the cross-mission audit
/// trail the service produces.
fn enqueue_timeout_continuation(
    root: &Path,
    thread_key: &str,
    workspace_root: Option<&str>,
    original_brief: &str,
    blocker: &str,
    parent_message_key: Option<&str>,
) -> anyhow::Result<String> {
    let goal_clip: String = original_brief.chars().take(60).collect();
    let title = format!("Continue after timeout: {goal_clip}");
    let blocker_clip: String = blocker.trim().chars().take(220).collect();
    let prompt = format!(
        "Continue the interrupted task from the latest saved state.\n\n\
         Current task:\n{}\n\n\
         Runtime stop:\n{}\n\n\
         Required actions:\n\
         - re-check repo, runtime, queue, progress artifacts, and continuity\n\
         - preserve any work that already landed\n\
         - continue with the next smallest concrete step\n\
         - if more than one turn is still needed, leave exactly one open CTOX plan or queue item before the turn ends\n\
         - a sentence in the reply does not count as open work",
        original_brief, blocker_clip
    );
    let view = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title,
            prompt,
            thread_key: thread_key.to_string(),
            workspace_root: workspace_root.map(str::to_owned),
            priority: "high".to_string(),
            suggested_skill: None,
            parent_message_key: parent_message_key.map(str::to_owned),
        },
    )?;
    Ok(view.title)
}

/// Lease at most one pending queue message whose thread matches our mission.
/// `channels::lease_pending_inbound_messages` leases across the whole queue;
/// we filter to our thread here. Foreign messages (which shouldn't exist in
/// a bench container but may exist in an exploratory host run) are released
/// back with status `blocked` rather than consumed.
fn lease_next_for_thread(
    root: &Path,
    thread_key: &str,
    lease_owner: &str,
) -> anyhow::Result<Option<(String, Vec<String>, Option<PathBuf>)>> {
    let leased = channels::lease_pending_inbound_messages(root, 32, lease_owner)?;
    let mut matched: Option<(String, Vec<String>, Option<PathBuf>)> = None;
    for msg in leased {
        if matched.is_none() && msg.thread_key == thread_key {
            let prompt = msg.body_text;
            let keys = vec![msg.message_key];
            let workspace = msg.workspace_root.map(PathBuf::from);
            matched = Some((prompt, keys, workspace));
        } else {
            let _ = channels::ack_leased_messages(root, &[msg.message_key], "blocked");
        }
    }
    Ok(matched)
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
