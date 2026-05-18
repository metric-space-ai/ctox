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
fn production_runtime_state_keeps_adapter_tuning_generic() {
    let production = production_source(include_str!("runtime_state.rs"));

    for forbidden in [
        "GptOssRuntimeTuning",
        ".gpt_oss",
        "alias = \"gpt_oss\"",
        "CTOX_GPT_OSS_HARMONY_REASONING_CAP",
        "CTOX_GPT_OSS_HARMONY_MAX_OUTPUT_TOKENS_CAP",
    ] {
        assert!(
            !production.contains(forbidden),
            "runtime_state production path leaked adapter-specific legacy detail `{forbidden}`"
        );
    }
}
