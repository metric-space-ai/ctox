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
fn production_engine_keeps_responses_compatibility_inside_adapters() {
    let production = production_source(include_str!("engine.rs"));

    for forbidden in [
        "HarmonyProxyRequest",
        "HarmonyFunctionCall",
        "HarmonyResponseItem",
        "HarmonyToolSpec",
        "parse_harmony_",
        "build_gpt_oss_harmony_prompt",
        "rewrite_responses_to_qwen_chat_completions",
        "rewrite_qwen_chat_completions_to_responses",
        "rewrite_responses_to_gpt_oss_completion",
        "rewrite_gpt_oss_completion_to_responses",
        "rewrite_gpt_oss_completion_to_sse",
        "build_gpt_oss_followup_completion_request",
        "should_use_gpt_oss_harmony_proxy",
        "Harmony-specific",
    ] {
        assert!(
            !production.contains(forbidden),
            "engine production path leaked adapter-owned compatibility detail `{forbidden}`"
        );
    }
}
