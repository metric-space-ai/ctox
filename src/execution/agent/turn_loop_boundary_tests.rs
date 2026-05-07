fn production_source(source: &str) -> String {
    let mut production = String::new();
    let mut skip_next_item = false;
    let mut skipping_item = false;
    let mut item_started = false;
    let mut brace_depth = 0i32;

    for line in source.lines() {
        let trimmed = line.trim();
        if !skip_next_item && !skipping_item && trimmed == "#[cfg(test)]" {
            skip_next_item = true;
            continue;
        }
        if skip_next_item {
            skip_next_item = false;
            skipping_item = true;
        }
        if skipping_item {
            let open = line.matches('{').count() as i32;
            let close = line.matches('}').count() as i32;
            if open > 0 || close > 0 || trimmed.ends_with(';') {
                item_started = true;
            }
            brace_depth += open - close;
            if item_started && brace_depth <= 0 && (trimmed.ends_with(';') || open > 0 || close > 0)
            {
                skipping_item = false;
                item_started = false;
                brace_depth = 0;
            }
            continue;
        }
        production.push_str(line);
        production.push('\n');
    }

    production
}

#[test]
fn production_turn_loop_stays_model_agnostic() {
    let production = production_source(include_str!("turn_loop.rs"));

    for forbidden in [
        "openai/gpt-oss-120b",
        "Qwen/Qwen3.5-",
        "google/gemma-4-",
        "nvidia/Nemotron-",
        "zai-org/GLM-4.7-Flash",
        "local GPT-OSS model",
        "local GLM model",
        "local Gemma 4 model",
        "local ChatML model",
        "ChatModelFamily::",
        "cargo run",
    ] {
        assert!(
            !production.contains(forbidden),
            "turn_loop production path leaked model-specific detail `{forbidden}`"
        );
    }
}

#[test]
fn production_review_sessions_use_read_only_tools() {
    let production = production_source(include_str!("direct_session.rs"));

    assert!(
        production.contains("start_review_with_read_only_tools")
            && production.contains("read_only_sandbox")
            && production.contains("SandboxMode::ReadOnly")
            && production.contains("dynamic_tools: disable_active_tools.then(Vec::new)"),
        "review sessions must expose tools for inspection while running under a read-only sandbox"
    );
}
