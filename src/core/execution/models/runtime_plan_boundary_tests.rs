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
fn production_runtime_plan_avoids_model_conditioned_planner_heuristics() {
    let production = production_source(include_str!("runtime_plan.rs"));

    for forbidden in [
        "harness.model.contains(",
        "harness.model ==",
        "if harness.model ==",
        "if harness.model.contains(",
        "contains(\"gemma-4-\")",
        "== \"openai/gpt-oss-120b\"",
        "Qwen/Qwen3.5-35B-A3B",
        "google/gemma-4-31B-it",
    ] {
        assert!(
            !production.contains(forbidden),
            "runtime_plan production path leaked model-conditioned planner logic `{forbidden}`"
        );
    }
}
