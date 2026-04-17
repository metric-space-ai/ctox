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
fn production_surfaces_keep_model_catalog_data_in_registry() {
    let files = [
        ("main.rs", production_source(include_str!("main.rs"))),
        (
            "capabilities/scrape.rs",
            production_source(include_str!("capabilities/scrape.rs")),
        ),
        (
            "ui/tui/mod.rs",
            production_source(include_str!("ui/tui/mod.rs")),
        ),
        (
            "execution/models/supervisor.rs",
            production_source(include_str!("execution/models/supervisor.rs")),
        ),
    ];
    let forbidden = [
        "openai/gpt-oss-20b",
        "Qwen/Qwen3.5-35B-A3B",
        "google/gemma-4-31B-it",
        "nvidia/Nemotron-Cascade-2-30B-A3B",
        "zai-org/GLM-4.7-Flash",
        "Qwen/Qwen3-Embedding-0.6B",
        "engineai/Voxtral-Mini-4B-Realtime-2602",
        "engineai/Voxtral-4B-TTS-2603",
        "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
        "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
        "Qwen/Qwen3-VL-2B-Instruct",
    ];

    for (name, source) in files {
        for needle in forbidden {
            assert!(
                !source.contains(needle),
                "{name} production path leaked model catalog data `{needle}` instead of deriving it from the registry",
            );
        }
    }
}
