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

/// tests-5(b): marker that sanctions a single legitimate `harness.model`
/// conditioning (a hardware/kernel quirk pin), distinguishing it from a banned
/// model-conditioned planner heuristic.
const PLANNER_FIREWALL_ALLOW: &str = "planner-model-firewall-allow";

/// tests-5(b): structural firewall over the planner. Instead of a fixed denylist
/// of specific model-id substrings (which a NEW vendor prefix slips past
/// untouched), flag the model-conditioning SHAPES generically: `harness.model`
/// adjacent to a comparison operator, and `matches!`/`match` over `harness.model`
/// against quoted vendor-id literals. Bare `harness.model` value-uses (passed
/// into capability-based ranking/placement functions) are intentionally NOT
/// flagged — that is the planner's correct, model-agnostic shape. A conditioning
/// block carrying the `planner-model-firewall-allow` marker is whitelisted.
fn model_conditioned_planner_violations(production: &str) -> Vec<String> {
    const OPERATORS: &[&str] = &[
        "==",
        "!=",
        ".contains(",
        ".starts_with(",
        ".ends_with(",
        ".eq_ignore_ascii_case(",
    ];
    let lines: Vec<&str> = production.lines().collect();
    let mut violations = Vec::new();

    for (i, raw) in lines.iter().enumerate() {
        let trimmed = raw.trim();
        // Strip any trailing line comment so a commented counter-example
        // (e.g. `// harness.model == "x"`) is never flagged as live conditioning.
        // (Splitting on `//` truncates at most a string's interior `//`, which
        // still precedes the operator/scrutinee we look for.)
        let code = raw.split("//").next().unwrap_or(raw);

        // (1) Operator-adjacency: `harness.model <op>` on a single line. For each
        // occurrence, look at what immediately follows (after optional space).
        let mut rest = code;
        while let Some(pos) = rest.find("harness.model") {
            let after = &rest[pos + "harness.model".len()..];
            // Skip a longer field like `harness.model_path` (next char is ident).
            let is_whole_field = !after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            if is_whole_field {
                let after_trimmed = after.trim_start();
                if let Some(op) = OPERATORS.iter().find(|op| after_trimmed.starts_with(*op)) {
                    // `==`/`!=` must be a comparison, not the start of `=>`/`=`.
                    violations.push(format!(
                        "line {}: harness.model is operator-conditioned (`{op}`): {trimmed}",
                        i + 1
                    ));
                }
            }
            rest = &rest[pos + "harness.model".len()..];
        }

        // (2) matches!/match over harness.model with quoted vendor-id literals.
        let is_scrutinee_line = trimmed == "harness.model,"
            || trimmed == "&harness.model,"
            || trimmed == "harness.model.as_str(),";
        let single_line_match = {
            let norm = code.split_whitespace().collect::<Vec<_>>().join(" ");
            norm.contains("match harness.model {")
                || norm.contains("match harness.model.as_str() {")
                || norm.contains("match &harness.model {")
        };
        let scrutinee_after_macro = is_scrutinee_line
            && i > 0
            && (lines[i - 1].trim().ends_with("matches!(")
                || lines[i - 1].trim() == "match"
                || lines[i - 1].trim().ends_with("match"));
        if scrutinee_after_macro || single_line_match {
            let arms_end = (i + 12).min(lines.len());
            let arms = lines[i + 1..arms_end].join(" ");
            let has_vendor_literal = arms.contains('"');
            // Whitelist ONLY when the marker sits as a COMMENT in the few lines
            // immediately above the conditioning site (its own block) — never in a
            // string/code literal, and never in the wider arm span — so a marker
            // can't accidentally (or via a planted string literal) whitelist an
            // unrelated conditioning that merely happens to be near.
            let whitelisted = lines[i.saturating_sub(5)..=i].iter().any(|l| {
                let t = l.trim();
                t.starts_with("//") && t.contains(PLANNER_FIREWALL_ALLOW)
            });
            if has_vendor_literal && !whitelisted {
                violations.push(format!(
                    "line {}: matches!/match on harness.model against quoted model-id literal(s)",
                    i + 1
                ));
            }
        }
    }

    violations
}

#[test]
fn production_runtime_plan_avoids_model_conditioned_planner_heuristics() {
    let production = production_source(include_str!("runtime_plan.rs"));
    let violations = model_conditioned_planner_violations(&production);
    assert!(
        violations.is_empty(),
        "runtime_plan production path leaked model-conditioned planner logic:\n{}",
        violations.join("\n")
    );
}

#[test]
fn model_conditioned_planner_scanner_flags_synthetic_conditioning() {
    // Pin the scanner's own sensitivity: each conditioning shape is caught, a
    // bare value-use is not, and the allow-marker whitelists a sanctioned pin.
    let conditioned = "fn plan() {\n\
        \x20   if harness.model == \"vendor/new-model\" { tweak(); }\n\
        \x20   if harness.model.contains(\"new-prefix-\") { tweak(); }\n\
        \x20   if harness.model.starts_with(\"vendor/\") { tweak(); }\n\
        \x20   // harness.model.contains(\"commented\") — a counter-example in a comment, NOT flagged\n\
        \x20   let v = matches!(\n\
        \x20       harness.model,\n\
        \x20       \"vendor/another-id\"\n\
        \x20   );\n\
        \x20   let ok = rank(root, harness.model, preset);\n\
        \x20   let name = harness.model.to_string();\n\
        }\n";
    let v = model_conditioned_planner_violations(conditioned);
    assert!(
        v.iter().any(|s| s.contains("`==`")),
        "== conditioning must be flagged: {v:?}"
    );
    assert!(
        v.iter().any(|s| s.contains(".contains(")),
        ".contains conditioning must be flagged: {v:?}"
    );
    assert!(
        v.iter().any(|s| s.contains(".starts_with(")),
        ".starts_with conditioning must be flagged: {v:?}"
    );
    assert!(
        v.iter().any(|s| s.contains("matches!/match")),
        "matches! conditioning must be flagged: {v:?}"
    );
    // The bare value-use `rank(root, harness.model, preset)` and the
    // `harness.model.to_string()` must NOT be flagged.
    assert_eq!(
        v.len(),
        4,
        "exactly the four conditioning shapes, no bare uses: {v:?}"
    );

    let allowed = "fn plan() {\n\
        \x20   // planner-model-firewall-allow: hardware quirk pin\n\
        \x20   let v = matches!(\n\
        \x20       harness.model,\n\
        \x20       \"vendor/quirk-id\"\n\
        \x20   );\n\
        }\n";
    assert!(
        model_conditioned_planner_violations(allowed).is_empty(),
        "the allow-marker must whitelist a sanctioned pin: {:?}",
        model_conditioned_planner_violations(allowed)
    );

    // The marker only counts as a COMMENT: the same token in a string literal
    // must NOT whitelist a real conditioning below it.
    let planted_string_marker = "fn plan() {\n\
        \x20   let label = \"planner-model-firewall-allow\";\n\
        \x20   let v = matches!(\n\
        \x20       harness.model,\n\
        \x20       \"vendor/sneaky-id\"\n\
        \x20   );\n\
        }\n";
    assert!(
        !model_conditioned_planner_violations(planted_string_marker).is_empty(),
        "a marker in a string literal must NOT whitelist real conditioning"
    );
}
