fn is_systematic_research_job(job: &QueuedPrompt) -> bool {
    job.suggested_skill.as_deref() == Some("systematic-research")
        || job.prompt.contains("research.systematic.run")
        || job.prompt.contains("research.systematic.report.create")
        || job.prompt.contains("research.knowledge.refresh")
}

fn materialize_systematic_research_skill(
    root: &Path,
    job: &QueuedPrompt,
) -> Result<Option<PathBuf>> {
    if !is_systematic_research_job(job) {
        return Ok(None);
    }
    let workspace = job
        .workspace_root
        .as_deref()
        .map(Path::new)
        .context("systematic research requires a typed workspace root")?;
    std::fs::create_dir_all(workspace).with_context(|| {
        format!(
            "create systematic research task workspace {}",
            workspace.display()
        )
    })?;
    let export_root = workspace.join(".ctox/system-skills");
    let skill_dir =
        crate::skill_store::export_system_skill(root, "systematic-research", &export_root)
            .context("materialize systematic-research skill inside task workspace")?;
    Ok(Some(skill_dir))
}

#[derive(Debug, Serialize)]
struct StagedResearchInput {
    original_path: String,
    staged_path: String,
    byte_size: u64,
    sha256: String,
}

fn materialize_systematic_research_seed_inputs(
    root: &Path,
    job: &QueuedPrompt,
) -> Result<Vec<StagedResearchInput>> {
    if !is_systematic_research_job(job) {
        return Ok(Vec::new());
    }
    let workspace = job
        .workspace_root
        .as_deref()
        .map(Path::new)
        .context("systematic research requires a typed workspace root")?;
    let approved_roots = [
        root.join("imports"),
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".local/share/ctox/imports"),
    ]
    .into_iter()
    .filter_map(|path| std::fs::canonicalize(path).ok())
    .collect::<Vec<_>>();
    if approved_roots.is_empty() {
        return Ok(Vec::new());
    }

    let allowed_suffixes = [
        ".tar.gz", ".tgz", ".zip", ".json", ".jsonl", ".csv", ".parquet", ".pdf",
    ];
    let mut candidates = Vec::new();
    for token in job.prompt.split_whitespace() {
        let candidate = token.trim_matches(|character: char| {
            matches!(
                character,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        });
        if !candidate.starts_with('/') {
            continue;
        }
        let lowercase = candidate.to_ascii_lowercase();
        if !allowed_suffixes
            .iter()
            .any(|suffix| lowercase.ends_with(suffix))
        {
            continue;
        }
        let Ok(canonical) = std::fs::canonicalize(candidate) else {
            continue;
        };
        if canonical.is_file()
            && approved_roots
                .iter()
                .any(|approved_root| canonical.starts_with(approved_root))
            && !candidates.contains(&canonical)
        {
            candidates.push(canonical);
        }
    }

    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let destination_root = workspace.join("inputs/managed-seeds");
    std::fs::create_dir_all(&destination_root).with_context(|| {
        format!(
            "create systematic research seed directory {}",
            destination_root.display()
        )
    })?;
    let mut staged = Vec::new();
    for source in candidates {
        let filename = source
            .file_name()
            .context("managed research seed has no filename")?;
        let destination = destination_root.join(filename);
        let source_metadata = std::fs::metadata(&source)?;
        let source_sha256 = sha256_file(&source)?;
        if destination.exists() {
            let destination_metadata = std::fs::symlink_metadata(&destination)?;
            anyhow::ensure!(
                destination_metadata.file_type().is_file(),
                "managed research seed conflict at {}: existing staged input is not a regular file",
                destination.display()
            );
            let destination_sha256 = sha256_file(&destination)?;
            anyhow::ensure!(
                source_metadata.len() == destination_metadata.len()
                    && source_sha256 == destination_sha256,
                "managed research seed conflict at {}: existing staged input does not match {}",
                destination.display(),
                source.display()
            );
        } else {
            std::fs::copy(&source, &destination).with_context(|| {
                format!(
                    "stage managed research seed {} at {}",
                    source.display(),
                    destination.display()
                )
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o444))?;
            }
        }
        staged.push(StagedResearchInput {
            original_path: source.to_string_lossy().into_owned(),
            staged_path: destination.to_string_lossy().into_owned(),
            byte_size: source_metadata.len(),
            sha256: source_sha256,
        });
    }
    std::fs::write(
        destination_root.join("manifest.json"),
        serde_json::to_vec_pretty(&staged)?,
    )?;
    Ok(staged)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = {
        use sha2::Digest;
        sha2::Sha256::new()
    };
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        use sha2::Digest;
        hasher.update(&buffer[..read]);
    }
    use sha2::Digest;
    Ok(format!("{:x}", hasher.finalize()))
}

fn business_os_app_validation_may_own_completion(job: &QueuedPrompt) -> bool {
    !is_systematic_research_job(job)
        && business_os_app_module_target_from_metadata(&job.queue_task_metadata).is_some()
}

fn systematic_research_binding_from_prompt(prompt: &str) -> Result<(&str, &str)> {
    fn prompt_value<'a>(prompt: &'a str, label: &str) -> Option<&'a str> {
        prompt.lines().find_map(|line| {
            line.trim()
                .strip_prefix(label)
                .map(str::trim)
                .filter(|value| !value.is_empty() && *value != "latest")
        })
    }

    fn json_prompt_value<'a>(prompt: &'a str, key: &str) -> Option<&'a str> {
        let prefix = format!("\"{key}\":");
        prompt.lines().find_map(|line| {
            line.trim()
                .strip_prefix(&prefix)
                .map(str::trim)
                .map(|value| value.trim_end_matches(',').trim())
                .and_then(|value| value.strip_prefix('"'))
                .and_then(|value| value.strip_suffix('"'))
                .filter(|value| !value.is_empty() && *value != "latest")
        })
    }

    let run_id = prompt_value(prompt, "Research Run ID:")
        .or_else(|| json_prompt_value(prompt, "research_run_id"))
        .context("systematic research task is missing an explicit Research Run ID")?;
    let command_id = prompt_value(prompt, "Research Command ID:")
        .or_else(|| json_prompt_value(prompt, "research_command_id"))
        .context("systematic research task is missing an explicit Research Command ID")?;
    Ok((run_id, command_id))
}

fn systematic_research_binding(job: &QueuedPrompt) -> Result<(&str, &str)> {
    systematic_research_binding_from_prompt(&job.prompt)
}

fn preserve_systematic_research_binding(
    original: &QueuedPrompt,
    recovery: &mut QueuedPrompt,
) -> Result<()> {
    if !is_systematic_research_job(original) {
        return Ok(());
    }

    let (run_id, command_id) = systematic_research_binding(original)?;
    if let Ok((recovery_run_id, recovery_command_id)) =
        systematic_research_binding_from_prompt(&recovery.prompt)
    {
        anyhow::ensure!(
            recovery_run_id == run_id && recovery_command_id == command_id,
            "systematic research recovery attempted to change its immutable run binding"
        );
        return Ok(());
    }

    recovery.prompt = format!(
        "{}\n\nImmutable systematic research binding:\nResearch Run ID: {}\nResearch Command ID: {}",
        recovery.prompt.trim_end(),
        run_id,
        command_id
    );
    recovery.preview = clip_text(&recovery.prompt, 180);
    Ok(())
}

fn systematic_research_started_at(
    root: &Path,
    job: &QueuedPrompt,
    fallback_started_at: u64,
) -> u64 {
    let db_path = crate::paths::core_db(root);
    let message_keys = job.leased_message_keys.clone();
    crate::lcm::with_lcm_read(&db_path, move |conn| {
        let mut earliest = None;
        for message_key in &message_keys {
            let created_at = conn
                .query_row(
                    "SELECT MIN(created_at)
                 FROM ctox_harness_flow_events
                 WHERE message_key = ?1
                   AND event_kind = 'business_command_context_loaded'",
                    params![message_key],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten();
            let Some(epoch) = created_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .and_then(|value| u64::try_from(value.timestamp()).ok())
                .filter(|epoch| *epoch <= fallback_started_at)
            else {
                continue;
            };
            earliest = Some(earliest.map_or(epoch, |current: u64| current.min(epoch)));
        }
        Ok(earliest.unwrap_or(fallback_started_at))
    })
    .unwrap_or(fallback_started_at)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SystematicResearchDepth {
    Quick,
    Standard,
    Exhaustive,
}

impl SystematicResearchDepth {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "quick" => Some(Self::Quick),
            "standard" => Some(Self::Standard),
            "exhaustive" => Some(Self::Exhaustive),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Exhaustive => "exhaustive",
        }
    }
}

fn required_systematic_research_depth(job: &QueuedPrompt) -> SystematicResearchDepth {
    let prompt = job.prompt.to_ascii_lowercase();
    if [
        "deep research depth: exhaustive",
        "required deep research depth: exhaustive",
        "requested discovery depth: exhaustive",
        "--depth exhaustive",
        "\"depth\":\"exhaustive\"",
        "\"depth\": \"exhaustive\"",
        "with exhaustive depth",
        "exhaustive depth",
    ]
    .iter()
    .any(|marker| prompt.contains(marker))
    {
        SystematicResearchDepth::Exhaustive
    } else {
        SystematicResearchDepth::Standard
    }
}

#[derive(Clone, Copy, Debug)]
struct SystematicResearchCoverage {
    minimum_discovery_rounds: usize,
    minimum_scholarly_rounds: usize,
    target_verified_sources: usize,
}

fn systematic_research_numeric_requirement(
    prompt: &str,
    label: &str,
    json_key: &str,
) -> Option<usize> {
    prompt
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(label)
                .map(str::trim)
                .and_then(|value| value.parse::<usize>().ok())
        })
        .or_else(|| {
            let prefix = format!("\"{json_key}\":");
            prompt.lines().find_map(|line| {
                line.trim()
                    .strip_prefix(&prefix)
                    .map(str::trim)
                    .map(|value| value.trim_end_matches(',').trim())
                    .and_then(|value| value.parse::<usize>().ok())
            })
        })
}

fn required_systematic_research_coverage(job: &QueuedPrompt) -> SystematicResearchCoverage {
    SystematicResearchCoverage {
        minimum_discovery_rounds: systematic_research_numeric_requirement(
            &job.prompt,
            "Minimum Discovery Rounds:",
            "minimum_discovery_rounds",
        )
        .unwrap_or(1)
        .clamp(1, 24),
        minimum_scholarly_rounds: systematic_research_numeric_requirement(
            &job.prompt,
            "Minimum Scholarly Rounds:",
            "minimum_scholarly_rounds",
        )
        .unwrap_or(0)
        .clamp(0, 12),
        target_verified_sources: systematic_research_numeric_requirement(
            &job.prompt,
            "Target Verified Sources:",
            "target_verified_sources",
        )
        .unwrap_or(1)
        .clamp(1, 300),
    }
}

/// Typed discovery tools that satisfy the systematic-research discovery
/// receipt. `ctox_deep_research` is one optional broad discovery round, never
/// the required first move or the entire workflow: the agent may interleave
/// search, scholarly lookup, and typed reads freely (benchmark forensics H2).
/// An explicitly exhaustive command still requires one persisted exhaustive
/// sweep before completion.
const SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS: [&str; 3] = [
    "ctox_deep_research",
    "ctox_web_search",
    "ctox_scholarly_search",
];

/// Shell/exec tool surfaces whose command payload may carry an equivalent
/// typed CTOX Web CLI invocation. The typed `CtoxWebHandler` itself spawns
/// the same `ctox web …` CLI (see
/// `src/core/harness/core/src/tools/handlers/ctox_web.rs`), so a durable
/// `exec_command` invocation of `ctox web <subcommand>` runs the identical
/// server-owned pipeline and persists the identical receipts; only the
/// rollout surface differs. Completion validation therefore recognizes both
/// surfaces with the same run/command/workspace binding (F-010 layer 2).
const SYSTEMATIC_RESEARCH_EXEC_TOOLS: [&str; 2] = ["exec_command", "shell"];

/// An equivalent typed CTOX Web invocation recovered from an exec/shell
/// command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SystematicResearchCliCall {
    /// Logical typed-tool name (`ctox_web_search`, `ctox_web_read`,
    /// `ctox_scholarly_search`, or `ctox_deep_research`).
    tool: &'static str,
    depth: SystematicResearchDepth,
    no_workspace: bool,
}

/// Tokenize a shell command line honoring single/double quotes and
/// backslash escapes. Returns `None` on unterminated quotes.
fn tokenize_systematic_research_cli(cmd: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_content = false;
    let mut quote: Option<char> = None;
    let mut chars = cmd.chars();
    while let Some(ch) = chars.next() {
        match quote {
            Some(active) => {
                if ch == active {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    has_content = true;
                }
                '\\' => {
                    let next = chars.next()?;
                    current.push(next);
                    has_content = true;
                }
                // Shell control characters are token boundaries even when
                // written without surrounding whitespace (`'…'; ctox …`).
                ';' | '|' | '&' | '<' | '>' => {
                    if has_content {
                        tokens.push(std::mem::take(&mut current));
                        has_content = false;
                    }
                    tokens.push(ch.to_string());
                }
                ch if ch.is_whitespace() => {
                    if has_content {
                        tokens.push(std::mem::take(&mut current));
                        has_content = false;
                    }
                }
                ch => {
                    current.push(ch);
                    has_content = true;
                }
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    if has_content {
        tokens.push(current);
    }
    Some(tokens)
}

/// Parse an exec/shell tool payload into an equivalent typed CTOX Web CLI
/// invocation. Fail-closed: only one plain
/// `[ENV=…] ctox web <subcommand> [flags]` invocation is recognized; shell
/// operators, substitutions, pipelines, redirects, or chained commands make
/// the call unrecognizable so a model cannot smuggle fabricated envelope
/// text around the real CLI output.
fn parse_systematic_research_cli_call(arguments: &str) -> Option<SystematicResearchCliCall> {
    let payload = serde_json::from_str::<Value>(arguments).ok()?;
    let cmd = payload
        .get("cmd")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            // Legacy shell surface: direct argv, or a `bash -c/-lc "<cmd>"`
            // wrapper whose command string is re-validated below.
            let argv = payload.get("command").and_then(Value::as_array)?;
            let argv: Vec<&str> = argv.iter().filter_map(Value::as_str).collect();
            let first = argv.first()?;
            let first_base = first.rsplit('/').next().unwrap_or(first);
            if matches!(first_base, "bash" | "sh" | "zsh" | "dash")
                && argv.len() >= 3
                && argv[1].starts_with('-')
                && argv[1].contains('c')
            {
                return Some(argv[2].to_string());
            }
            Some(argv.join(" "))
        })?;
    let cmd = cmd.trim();
    if cmd.is_empty() || cmd.contains(['\n', '\r', '`']) || cmd.contains("$(") || cmd.contains("${")
    {
        return None;
    }
    let tokens = tokenize_systematic_research_cli(cmd)?;
    const OPERATORS: [&str; 11] = ["&&", "||", ";", "|", ">", ">>", "<", "<<", "&", "2>", "2>>"];
    if tokens
        .iter()
        .any(|token| OPERATORS.contains(&token.as_str()))
    {
        return None;
    }
    let mut tokens = tokens.into_iter().peekable();
    while tokens.peek().is_some_and(|token| {
        let mut split = token.splitn(2, '=');
        let name = split.next().unwrap_or_default();
        split.next().is_some()
            && !name.is_empty()
            && name.chars().enumerate().all(|(index, ch)| {
                ch == '_' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit())
            })
    }) {
        tokens.next();
    }
    let binary = tokens.next()?;
    if binary.rsplit('/').next().unwrap_or(binary.as_str()) != "ctox" {
        return None;
    }
    if tokens.next().as_deref() != Some("web") {
        return None;
    }
    let subcommand = tokens.next()?;
    let (tool, flags): (&'static str, Vec<String>) = match subcommand.as_str() {
        "search" => ("ctox_web_search", tokens.collect()),
        "read" => ("ctox_web_read", tokens.collect()),
        "deep-research" => ("ctox_deep_research", tokens.collect()),
        "scholarly" => {
            if tokens.next().as_deref() != Some("search") {
                return None;
            }
            ("ctox_scholarly_search", tokens.collect())
        }
        _ => return None,
    };
    let mut depth = SystematicResearchDepth::Standard;
    let mut no_workspace = false;
    let mut index = 0;
    while index < flags.len() {
        match flags[index].as_str() {
            "--depth" => {
                depth = SystematicResearchDepth::parse(flags.get(index + 1)?)?;
                index += 2;
            }
            "--no-workspace" => {
                no_workspace = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    Some(SystematicResearchCliCall {
        tool,
        depth,
        no_workspace,
    })
}

/// Extract the CTOX Web JSON envelope from a tool output string. Typed web
/// tools return the CLI stdout verbatim; exec outputs prepend
/// `Command:`/`Wall time:`/`Output:` sections and may truncate, so this
/// scans for the first embedded JSON document carrying the envelope shape.
fn extract_ctox_web_envelope(output: &str) -> Option<Value> {
    fn is_envelope(value: &Value) -> bool {
        value.is_object()
            && (value.get("ok").is_some()
                || value.get("workspace_evidence").is_some()
                || value.get("research_workspace").is_some()
                || value.get("results").is_some())
    }
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if is_envelope(&value) {
            return Some(value);
        }
        for key in ["output", "content"] {
            if let Some(inner) = value.get(key).and_then(Value::as_str) {
                if let Some(found) = extract_ctox_web_envelope(inner) {
                    return Some(found);
                }
            }
        }
    }
    for (index, ch) in output.char_indices() {
        if ch != '{' {
            continue;
        }
        let mut stream = serde_json::Deserializer::from_str(&output[index..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next() {
            if is_envelope(&value) {
                return Some(value);
            }
        }
    }
    None
}

fn validate_systematic_research_discovery_receipt(
    job: &QueuedPrompt,
    workspace: &Path,
    research_started_at: u64,
) -> Result<Value> {
    let (expected_run_id, expected_command_id) = systematic_research_binding(job)?;
    let codex_home =
        ctox_core::config::find_codex_home().context("resolve harness state directory")?;
    let state_db = [
        codex_home.join("state_5.sqlite"),
        codex_home.join("sqlite/state_5.sqlite"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .context("systematic research requires the durable harness state database")?;
    let conn = Connection::open_with_flags(
        &state_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open harness state database {}", state_db.display()))?;
    validate_systematic_research_discovery_receipt_from_conn(
        &conn,
        &codex_home,
        workspace,
        &expected_run_id,
        &expected_command_id,
        research_started_at,
        required_systematic_research_depth(job),
    )
}

fn validate_systematic_research_discovery_receipt_from_conn(
    conn: &Connection,
    codex_home: &Path,
    workspace: &Path,
    expected_run_id: &str,
    expected_command_id: &str,
    research_started_at: u64,
    required_depth: SystematicResearchDepth,
) -> Result<Value> {
    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("canonicalize research workspace {}", workspace.display()))?;
    let codex_home = codex_home.canonicalize().with_context(|| {
        format!(
            "canonicalize harness state directory {}",
            codex_home.display()
        )
    })?;
    let mut stmt = conn.prepare(
        "SELECT id, rollout_path
         FROM threads
         WHERE subagent_parent_thread_id IS NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut observed_calls = Vec::new();
    for row in rows {
        let (thread_id, rollout_path) = row?;
        let rollout_path = PathBuf::from(rollout_path);
        let Ok(rollout_path) = rollout_path.canonicalize() else {
            continue;
        };
        if !rollout_path.starts_with(&codex_home) {
            continue;
        }
        let file = std::fs::File::open(&rollout_path)
            .with_context(|| format!("open harness rollout {}", rollout_path.display()))?;
        let mut calls =
            BTreeMap::<String, (&'static str, SystematicResearchDepth, u64, &'static str)>::new();
        let mut run_bound = false;
        let mut command_bound = false;
        let mut workspace_bound = false;
        for line in BufReader::new(file).lines() {
            let line = line?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(timestamp) = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
                .and_then(|timestamp| u64::try_from(timestamp.timestamp()).ok())
            else {
                continue;
            };
            if timestamp < research_started_at {
                continue;
            }
            let payload = value.get("payload").unwrap_or(&Value::Null);
            if payload.get("type").and_then(Value::as_str) == Some("task_started") {
                run_bound = false;
                command_bound = false;
                workspace_bound = false;
                calls.clear();
            }
            if line.contains(expected_run_id) {
                run_bound = true;
            }
            if line.contains(expected_command_id) {
                command_bound = true;
            }
            if value.get("type").and_then(Value::as_str) == Some("turn_context") {
                workspace_bound = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .and_then(|cwd| Path::new(cwd).canonicalize().ok())
                    .is_some_and(|cwd| cwd == workspace);
            }
            match payload.get("type").and_then(Value::as_str) {
                Some("function_call") => {
                    let name = payload
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let Some(arguments) = payload.get("arguments").and_then(Value::as_str) else {
                        continue;
                    };
                    let (tool, depth, no_workspace, via): (
                        &'static str,
                        SystematicResearchDepth,
                        bool,
                        &'static str,
                    ) = if SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS.contains(&name) {
                        let tool: &'static str = SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS
                            .into_iter()
                            .find(|candidate| *candidate == name)
                            .expect("discovery tool membership checked above");
                        let Ok(arguments_value) = serde_json::from_str::<Value>(arguments) else {
                            continue;
                        };
                        let depth = arguments_value
                            .get("depth")
                            .and_then(Value::as_str)
                            .and_then(SystematicResearchDepth::parse)
                            .unwrap_or(SystematicResearchDepth::Standard);
                        let no_workspace = tool == "ctox_deep_research"
                            && arguments_value.get("no_workspace").and_then(Value::as_bool)
                                == Some(true);
                        (tool, depth, no_workspace, "")
                    } else if SYSTEMATIC_RESEARCH_EXEC_TOOLS.contains(&name) {
                        let Some(call) = parse_systematic_research_cli_call(arguments) else {
                            continue;
                        };
                        if !SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS.contains(&call.tool) {
                            continue;
                        }
                        (
                            call.tool,
                            call.depth,
                            call.no_workspace,
                            " via exec_command",
                        )
                    } else {
                        continue;
                    };
                    if !run_bound || !command_bound || !workspace_bound {
                        continue;
                    }
                    let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
                        continue;
                    };
                    if tool == "ctox_deep_research" && no_workspace {
                        observed_calls.push(format!(
                            "{tool} at depth {} (no_workspace){via}",
                            depth.as_str()
                        ));
                        continue;
                    }
                    calls.insert(call_id.to_string(), (tool, depth, timestamp, via));
                }
                Some("function_call_output") => {
                    let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some((tool, depth, called_at, via)) = calls.get(call_id) else {
                        continue;
                    };
                    let (tool, depth, called_at, via) = (*tool, *depth, *called_at, *via);
                    let Some(output) = payload.get("output").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(output) = extract_ctox_web_envelope(output) else {
                        observed_calls.push(format!("{tool}{via} (invalid output)"));
                        continue;
                    };
                    if output.get("ok").and_then(Value::as_bool) != Some(true) {
                        observed_calls.push(format!("{tool}{via} (failed)"));
                        continue;
                    }
                    // Search and scholarly envelopes carry ok/provider/results
                    // and persist their own receipts server-side; only a deep
                    // research sweep must additionally prove its research
                    // workspace inside the task workspace.
                    if tool != "ctox_deep_research" {
                        observed_calls.push(format!("{tool}{via}"));
                        if required_depth == SystematicResearchDepth::Exhaustive {
                            continue;
                        }
                        return Ok(serde_json::json!({
                            "tool": tool,
                            "transport": if via.is_empty() { "typed_tool" } else { "exec_cli" },
                            "provider": output.get("provider").cloned(),
                            "thread_id": thread_id,
                            "call_id": call_id,
                            "called_at_epoch": called_at,
                            "research_run_id": expected_run_id,
                            "research_command_id": expected_command_id,
                            "rollout_path": rollout_path,
                        }));
                    }
                    let research_workspace = output.get("research_workspace");
                    let research_workspace_path = research_workspace
                        .and_then(|receipt| {
                            receipt
                                .as_str()
                                .or_else(|| receipt.get("path").and_then(Value::as_str))
                        })
                        .map(PathBuf::from);
                    let persisted_workspace = research_workspace_path
                        .as_deref()
                        .and_then(|path| path.canonicalize().ok())
                        .is_some_and(|path| path.starts_with(&workspace));
                    let persisted_receipt_artifacts = research_workspace
                        .filter(|receipt| receipt.is_object())
                        .is_none_or(|receipt| {
                            ["manifest", "evidence_bundle"].into_iter().all(|field| {
                                receipt
                                    .get(field)
                                    .and_then(Value::as_str)
                                    .map(Path::new)
                                    .filter(|path| path.is_file())
                                    .and_then(|path| path.canonicalize().ok())
                                    .is_some_and(|path| path.starts_with(&workspace))
                            })
                        });
                    if !persisted_workspace || !persisted_receipt_artifacts {
                        observed_calls.push(format!(
                            "{tool} at depth {} (not persisted){via}",
                            depth.as_str()
                        ));
                        continue;
                    }
                    if depth < required_depth {
                        observed_calls.push(format!(
                            "{tool} at depth {} (shallower than requested {}){via}",
                            depth.as_str(),
                            required_depth.as_str()
                        ));
                        continue;
                    }
                    observed_calls.push(format!("{tool} at depth {}{via}", depth.as_str()));
                    return Ok(serde_json::json!({
                        "tool": tool,
                        "transport": if via.is_empty() { "typed_tool" } else { "exec_cli" },
                        "depth": depth.as_str(),
                        "thread_id": thread_id,
                        "call_id": call_id,
                        "called_at_epoch": called_at,
                        "research_run_id": expected_run_id,
                        "research_command_id": expected_command_id,
                        "research_workspace": research_workspace.cloned(),
                        "rollout_path": rollout_path,
                    }));
                }
                _ => {}
            }
        }
    }

    anyhow::bail!(
        "systematic research requires at least one successful typed discovery call ({}) or an equivalent `ctox web …` CLI invocation in the current durable research run{}; observed discovery calls: {}",
        SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS.join(", "),
        if required_depth == SystematicResearchDepth::Exhaustive {
            " and an exhaustive persisted ctox_deep_research sweep"
        } else {
            ""
        },
        if observed_calls.is_empty() {
            "none".to_string()
        } else {
            observed_calls.join(", ")
        }
    )
}

fn validate_systematic_research_discovery_coverage(
    job: &QueuedPrompt,
    discovery_receipt: &Value,
    research_started_at: u64,
) -> Result<Value> {
    let coverage = required_systematic_research_coverage(job);
    let (expected_run_id, expected_command_id) = systematic_research_binding(job)?;
    let expected_workspace = job
        .workspace_root
        .as_deref()
        .map(Path::new)
        .and_then(|path| path.canonicalize().ok())
        .context("systematic research discovery coverage requires a canonical workspace")?;
    let rollout_path = discovery_receipt
        .get("rollout_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .context("systematic research discovery receipt is missing rollout_path")?;
    let file = std::fs::File::open(&rollout_path)
        .with_context(|| format!("open harness rollout {}", rollout_path.display()))?;

    let mut run_bound = false;
    let mut command_bound = false;
    let mut workspace_bound = false;
    let mut calls = BTreeMap::<String, String>::new();
    let mut completed_call_ids = HashSet::<String>::new();
    let mut tool_types = HashSet::<String>::new();
    let mut scholarly_rounds = 0usize;
    let mut successful_rounds = 0usize;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(timestamp) = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
            .and_then(|timestamp| u64::try_from(timestamp.timestamp()).ok())
        else {
            continue;
        };
        if timestamp < research_started_at {
            continue;
        }
        let payload = value.get("payload").unwrap_or(&Value::Null);
        if payload.get("type").and_then(Value::as_str) == Some("task_started") {
            run_bound = false;
            command_bound = false;
            workspace_bound = false;
            calls.clear();
        }
        if line.contains(expected_run_id) {
            run_bound = true;
        }
        if line.contains(expected_command_id) {
            command_bound = true;
        }
        if value.get("type").and_then(Value::as_str) == Some("turn_context") {
            workspace_bound = payload
                .get("cwd")
                .and_then(Value::as_str)
                .and_then(|cwd| Path::new(cwd).canonicalize().ok())
                .is_some_and(|cwd| cwd == expected_workspace);
        }
        match payload.get("type").and_then(Value::as_str) {
            Some("function_call") if run_bound && command_bound && workspace_bound => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let tool: Option<&'static str> =
                    if SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS.contains(&name) {
                        SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS
                            .into_iter()
                            .find(|candidate| *candidate == name)
                    } else if SYSTEMATIC_RESEARCH_EXEC_TOOLS.contains(&name) {
                        payload
                            .get("arguments")
                            .and_then(Value::as_str)
                            .and_then(parse_systematic_research_cli_call)
                            .filter(|call| SYSTEMATIC_RESEARCH_DISCOVERY_TOOLS.contains(&call.tool))
                            .map(|call| call.tool)
                    } else {
                        None
                    };
                let Some(tool) = tool else {
                    continue;
                };
                let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
                    continue;
                };
                calls.insert(call_id.to_string(), tool.to_string());
            }
            Some("function_call_output") if run_bound && command_bound && workspace_bound => {
                let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(tool) = calls.get(call_id) else {
                    continue;
                };
                let output_ok = payload
                    .get("output")
                    .and_then(Value::as_str)
                    .and_then(extract_ctox_web_envelope)
                    .and_then(|output| output.get("ok").and_then(Value::as_bool))
                    == Some(true);
                if output_ok && completed_call_ids.insert(call_id.to_string()) {
                    successful_rounds = successful_rounds.saturating_add(1);
                    tool_types.insert(tool.clone());
                    if tool == "ctox_scholarly_search" {
                        scholarly_rounds = scholarly_rounds.saturating_add(1);
                    }
                }
            }
            _ => {}
        }
    }

    anyhow::ensure!(
        successful_rounds >= coverage.minimum_discovery_rounds,
        "systematic research requires at least {} successful typed discovery rounds; found {}",
        coverage.minimum_discovery_rounds,
        successful_rounds
    );
    anyhow::ensure!(
        scholarly_rounds >= coverage.minimum_scholarly_rounds,
        "systematic research requires at least {} successful scholarly rounds; found {}",
        coverage.minimum_scholarly_rounds,
        scholarly_rounds
    );
    anyhow::ensure!(
        coverage.minimum_discovery_rounds < 3 || tool_types.len() >= 2,
        "systematic research requires at least two discovery tool types; found {:?}",
        tool_types
    );
    Ok(serde_json::json!({
        "status": "pass",
        "successful_rounds": successful_rounds,
        "scholarly_rounds": scholarly_rounds,
        "tool_types": tool_types,
        "minimum_discovery_rounds": coverage.minimum_discovery_rounds,
        "minimum_scholarly_rounds": coverage.minimum_scholarly_rounds,
    }))
}

fn systematic_research_validation_receipt_path(job: &QueuedPrompt) -> Option<PathBuf> {
    job.workspace_root
        .as_deref()
        .map(PathBuf::from)
        .map(|workspace| {
            workspace
                .join(".ctox")
                .join("systematic-research-validation.json")
        })
}

fn validate_systematic_research_typed_web_read_receipts(
    job: &QueuedPrompt,
    workspace: &Path,
    manifest: &Value,
    research_started_at: u64,
) -> Result<()> {
    let (expected_run_id, expected_command_id) = systematic_research_binding(job)?;
    let codex_home =
        ctox_core::config::find_codex_home().context("resolve harness state directory")?;
    let state_db = [
        codex_home.join("state_5.sqlite"),
        codex_home.join("sqlite/state_5.sqlite"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .context("systematic research requires the durable harness state database")?;
    let conn = Connection::open_with_flags(
        &state_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open harness state database {}", state_db.display()))?;
    validate_systematic_research_typed_web_read_receipts_from_conn(
        &conn,
        &codex_home,
        workspace,
        &expected_run_id,
        &expected_command_id,
        manifest,
        research_started_at,
    )
}

fn validate_systematic_research_typed_web_read_receipts_from_conn(
    conn: &Connection,
    codex_home: &Path,
    workspace: &Path,
    expected_run_id: &str,
    expected_command_id: &str,
    manifest: &Value,
    research_started_at: u64,
) -> Result<()> {
    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("canonicalize research workspace {}", workspace.display()))?;
    let codex_home = codex_home.canonicalize().with_context(|| {
        format!(
            "canonicalize harness state directory {}",
            codex_home.display()
        )
    })?;
    let evidence = manifest
        .get("evidence")
        .and_then(Value::as_array)
        .context("evidence manifest has no evidence array")?;

    let mut required_receipts = HashSet::<(PathBuf, String, i64)>::new();
    for item in evidence {
        let evidence_id = item
            .get("evidence_id")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let retrieval = item
            .get("retrieval_receipt")
            .and_then(Value::as_object)
            .with_context(|| format!("evidence {evidence_id} has no retrieval_receipt"))?;
        anyhow::ensure!(
            retrieval.get("tool").and_then(Value::as_str) == Some("ctox_web_read"),
            "evidence {evidence_id} must use a direct typed ctox_web_read receipt"
        );
        let artifact = retrieval
            .get("receipt_artifact")
            .and_then(Value::as_object)
            .with_context(|| {
                format!("evidence {evidence_id} has no retrieval_receipt.receipt_artifact")
            })?;
        let relative_path = artifact
            .get("path")
            .and_then(Value::as_str)
            .with_context(|| format!("evidence {evidence_id} has no receipt artifact path"))?;
        let artifact_hash = artifact
            .get("sha256")
            .and_then(Value::as_str)
            .map(|value| {
                value
                    .strip_prefix("sha256:")
                    .unwrap_or(value)
                    .to_ascii_lowercase()
            })
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .with_context(|| {
                format!("evidence {evidence_id} has an invalid receipt artifact hash")
            })?;
        let relevance_score = item
            .get("relevance_score")
            .and_then(Value::as_i64)
            .filter(|score| (8..=10).contains(score))
            .with_context(|| {
                format!("evidence {evidence_id} has no eligible typed relevance score")
            })?;
        let receipt_path = workspace
            .join(relative_path)
            .canonicalize()
            .with_context(|| {
                format!("canonicalize evidence {evidence_id} receipt artifact {relative_path}")
            })?;
        anyhow::ensure!(
            receipt_path.starts_with(&workspace),
            "evidence {evidence_id} receipt artifact escapes the research workspace"
        );
        required_receipts.insert((receipt_path, artifact_hash, relevance_score));
    }

    let mut observed_receipts = HashSet::<(PathBuf, String, i64)>::new();
    let mut stmt = conn.prepare(
        "SELECT rollout_path
         FROM threads
         WHERE subagent_parent_thread_id IS NULL",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let rollout_path = PathBuf::from(row?);
        let Ok(rollout_path) = rollout_path.canonicalize() else {
            continue;
        };
        if !rollout_path.starts_with(&codex_home) {
            continue;
        }
        let file = std::fs::File::open(&rollout_path)
            .with_context(|| format!("open harness rollout {}", rollout_path.display()))?;
        let mut calls = HashSet::<String>::new();
        let mut run_bound = false;
        let mut command_bound = false;
        let mut workspace_bound = false;
        for line in BufReader::new(file).lines() {
            let line = line?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(timestamp) = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
                .and_then(|timestamp| u64::try_from(timestamp.timestamp()).ok())
            else {
                continue;
            };
            if timestamp < research_started_at {
                continue;
            }
            let payload = value.get("payload").unwrap_or(&Value::Null);
            if payload.get("type").and_then(Value::as_str) == Some("task_started") {
                run_bound = false;
                command_bound = false;
                workspace_bound = false;
                calls.clear();
            }
            if line.contains(expected_run_id) {
                run_bound = true;
            }
            if line.contains(expected_command_id) {
                command_bound = true;
            }
            if value.get("type").and_then(Value::as_str) == Some("turn_context") {
                workspace_bound = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .and_then(|cwd| Path::new(cwd).canonicalize().ok())
                    .is_some_and(|cwd| cwd == workspace);
            }
            match payload.get("type").and_then(Value::as_str) {
                Some("function_call") => {
                    let name = payload
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let is_web_read = name == "ctox_web_read"
                        || (SYSTEMATIC_RESEARCH_EXEC_TOOLS.contains(&name)
                            && payload
                                .get("arguments")
                                .and_then(Value::as_str)
                                .and_then(parse_systematic_research_cli_call)
                                .is_some_and(|call| call.tool == "ctox_web_read"));
                    if is_web_read && run_bound && command_bound && workspace_bound {
                        if let Some(call_id) = payload.get("call_id").and_then(Value::as_str) {
                            calls.insert(call_id.to_string());
                        }
                    }
                }
                Some("function_call_output") => {
                    let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
                        continue;
                    };
                    if !calls.contains(call_id) {
                        continue;
                    }
                    let Some(output) = payload
                        .get("output")
                        .and_then(Value::as_str)
                        .and_then(extract_ctox_web_envelope)
                    else {
                        continue;
                    };
                    if output.get("ok").and_then(Value::as_bool) != Some(true)
                        || output.get("evidence_eligible").and_then(Value::as_bool) != Some(true)
                        || output
                            .pointer("/workspace_evidence/persisted")
                            .and_then(Value::as_bool)
                            != Some(true)
                    {
                        continue;
                    }
                    let Some(receipt_path) = output
                        .pointer("/workspace_evidence/receipt_path")
                        .and_then(Value::as_str)
                        .map(PathBuf::from)
                        .and_then(|path| path.canonicalize().ok())
                    else {
                        continue;
                    };
                    if !receipt_path.starts_with(&workspace) {
                        continue;
                    }
                    let Some(receipt_hash) = output
                        .pointer("/workspace_evidence/receipt_sha256")
                        .and_then(Value::as_str)
                        .map(|value| {
                            value
                                .strip_prefix("sha256:")
                                .unwrap_or(value)
                                .to_ascii_lowercase()
                        })
                        .filter(|value| {
                            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                    else {
                        continue;
                    };
                    let Some(relevance_score) = output
                        .get("evidence_relevance_score")
                        .and_then(Value::as_i64)
                        .filter(|score| (8..=10).contains(score))
                    else {
                        continue;
                    };
                    observed_receipts.insert((receipt_path, receipt_hash, relevance_score));
                }
                _ => {}
            }
        }
    }

    let mut missing = required_receipts
        .difference(&observed_receipts)
        .map(|(path, _, score)| format!("{} (manifest score {score})", path.display()))
        .collect::<Vec<_>>();
    missing.sort();
    anyhow::ensure!(
        missing.is_empty(),
        "evidence receipt artifacts were not emitted by typed ctox_web_read calls or equivalent `ctox web read` CLI invocations in the current durable research run: {}",
        missing.join(", ")
    );
    Ok(())
}

fn validate_systematic_research_web_receipts(
    root: &Path,
    manifest: &Value,
    research_started_at: u64,
) -> Result<()> {
    fn normalized_sha256(value: &str) -> Option<&str> {
        let digest = value.strip_prefix("sha256:").unwrap_or(value);
        (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then_some(digest)
    }
    fn sha256_matches(left: Option<&str>, right: &str) -> bool {
        left.and_then(normalized_sha256)
            .is_some_and(|digest| digest.eq_ignore_ascii_case(right))
    }
    fn data_artifact_matches(root: &Path, doc: &Value, receipt: &Value, expected: &str) -> bool {
        use sha2::Digest;

        let is_data_file = receipt
            .get("content_kind")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.starts_with("data_"));
        if !is_data_file {
            return true;
        }
        let Some(raw_path) = doc.get("response_artifact_path").and_then(Value::as_str) else {
            return false;
        };
        let Ok(cache_root) = root.join("runtime/web_search_data_cache").canonicalize() else {
            return false;
        };
        let Ok(path) = Path::new(raw_path).canonicalize() else {
            return false;
        };
        if !path.starts_with(cache_root) {
            return false;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            return false;
        };
        if receipt.get("byte_count").and_then(Value::as_u64) != Some(bytes.len() as u64) {
            return false;
        }
        let actual = format!("{:x}", sha2::Sha256::digest(&bytes));
        actual.eq_ignore_ascii_case(expected)
    }
    fn extracted_text_matches(doc: &Value, expected: Option<&str>) -> bool {
        use sha2::Digest;

        let Some(expected) = expected else {
            return true;
        };
        if let Some(actual) = doc
            .get("extracted_text_sha256")
            .and_then(Value::as_str)
            .and_then(normalized_sha256)
        {
            return actual.eq_ignore_ascii_case(expected);
        }
        let Some(page_text) = doc.get("page_text").and_then(Value::as_str) else {
            return false;
        };
        let actual = format!("{:x}", sha2::Sha256::digest(page_text.as_bytes()));
        actual.eq_ignore_ascii_case(expected)
    }

    const MAX_WEB_RECEIPT_AGE_SECS: u64 = 7 * 24 * 60 * 60;
    let now = current_epoch_secs();
    let cache_path = root.join("runtime/web_search_page_cache.json");
    let cache_bytes = std::fs::read(&cache_path).with_context(|| {
        format!(
            "systematic research requires the server-owned CTOX Web Stack cache: {}",
            cache_path.display()
        )
    })?;
    let cache: Value = serde_json::from_slice(&cache_bytes)
        .with_context(|| format!("parse CTOX Web Stack cache {}", cache_path.display()))?;
    let entries = cache
        .get("entries")
        .and_then(Value::as_object)
        .context("CTOX Web Stack cache has no entries")?;
    let receipt_history = cache
        .get("receipt_history")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let evidence = manifest
        .get("evidence")
        .and_then(Value::as_array)
        .context("evidence manifest has no evidence array")?;

    let mut unmatched = Vec::new();
    for item in evidence {
        let evidence_id = item
            .get("evidence_id")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let canonical_url = item
            .get("canonical_url")
            .and_then(Value::as_str)
            .with_context(|| format!("evidence {evidence_id} has no canonical_url"))?;
        let snapshot_hash = item
            .get("snapshot_sha256")
            .and_then(Value::as_str)
            .with_context(|| format!("evidence {evidence_id} has no snapshot_sha256"))?;
        let normalized_snapshot_hash = normalized_sha256(snapshot_hash)
            .with_context(|| format!("evidence {evidence_id} has an invalid snapshot_sha256"))?;
        let http_status = item
            .get("http_status")
            .and_then(Value::as_u64)
            .with_context(|| format!("evidence {evidence_id} has no http_status"))?;
        let content_kind = item
            .get("content_kind")
            .and_then(Value::as_str)
            .with_context(|| format!("evidence {evidence_id} has no content_kind"))?;
        let content_scope = item
            .get("content_scope")
            .and_then(Value::as_str)
            .with_context(|| format!("evidence {evidence_id} has no content_scope"))?;
        let relevance_score = item
            .get("relevance_score")
            .and_then(Value::as_i64)
            .with_context(|| format!("evidence {evidence_id} has no relevance_score"))?;
        let manifest_receipt = item
            .get("retrieval_receipt")
            .and_then(Value::as_object)
            .with_context(|| format!("evidence {evidence_id} has no retrieval_receipt"))?;
        let manifest_request_url = manifest_receipt
            .get("request_url")
            .and_then(Value::as_str)
            .with_context(|| {
                format!("evidence {evidence_id} has no retrieval_receipt.request_url")
            })?;
        let manifest_final_url = manifest_receipt
            .get("final_url")
            .and_then(Value::as_str)
            .with_context(|| {
                format!("evidence {evidence_id} has no retrieval_receipt.final_url")
            })?;
        let manifest_checked_at = manifest_receipt
            .get("checked_at_epoch")
            .and_then(Value::as_u64)
            .with_context(|| {
                format!("evidence {evidence_id} has no retrieval_receipt.checked_at_epoch")
            })?;
        let manifest_byte_count = manifest_receipt
            .get("byte_count")
            .and_then(Value::as_u64)
            .with_context(|| {
                format!("evidence {evidence_id} has no retrieval_receipt.byte_count")
            })?;
        let manifest_content_kind = manifest_receipt
            .get("content_kind")
            .and_then(Value::as_str)
            .with_context(|| {
                format!("evidence {evidence_id} has no retrieval_receipt.content_kind")
            })?;
        let manifest_body_hash = manifest_receipt
            .get("body_sha256")
            .and_then(Value::as_str)
            .and_then(normalized_sha256)
            .with_context(|| {
                format!("evidence {evidence_id} has an invalid retrieval_receipt.body_sha256")
            })?;
        if manifest_final_url != canonical_url {
            anyhow::bail!(
                "evidence {evidence_id} canonical_url does not equal the immutable retrieval final_url"
            );
        }
        let extracted_text_sha256 = if content_kind == "data_file" {
            None
        } else {
            let raw = item
                .pointer("/extracted_text/sha256")
                .and_then(Value::as_str)
                .with_context(|| format!("evidence {evidence_id} has no extracted_text.sha256"))?;
            Some(normalized_sha256(raw).with_context(|| {
                format!("evidence {evidence_id} has an invalid extracted_text.sha256")
            })?)
        };

        let matching_entry = entries
            .values()
            .chain(receipt_history.iter())
            .find(|entry| {
                let doc = entry.get("doc").unwrap_or(&Value::Null);
                let receipt = doc.get("response_receipt").unwrap_or(&Value::Null);
                let response_kind = receipt
                    .get("content_kind")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let response_is_data = response_kind.starts_with("data_");
                let manifest_kind_matches = if response_is_data {
                    content_kind == "data_file"
                } else {
                    content_kind != "data_file" && content_scope == "full_text"
                };
                let url_matches = receipt.get("requested_url").and_then(Value::as_str)
                    == Some(manifest_request_url)
                    && receipt.get("final_url").and_then(Value::as_str) == Some(manifest_final_url)
                    && entry.get("final_url").and_then(Value::as_str) == Some(canonical_url);
                let created_at = entry
                    .get("created_at_epoch")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let checked_at = entry
                    .get("checked_at")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                url_matches
                    && created_at > 0
                    && checked_at > 0
                    && created_at >= research_started_at
                    && now.saturating_sub(created_at) <= MAX_WEB_RECEIPT_AGE_SECS
                    && checked_at == manifest_checked_at
                    && entry.get("evidence_eligible").and_then(Value::as_bool) == Some(true)
                    && doc.get("evidence_eligible").and_then(Value::as_bool) == Some(true)
                    && manifest_kind_matches
                    && (8..=10).contains(&relevance_score)
                    && entry
                        .get("evidence_relevance_score")
                        .and_then(Value::as_i64)
                        == Some(relevance_score)
                    && entry.get("http_status").and_then(Value::as_u64) == Some(http_status)
                    && sha256_matches(
                        entry.get("snapshot_hash").and_then(Value::as_str),
                        normalized_snapshot_hash,
                    )
                    && receipt.get("status").and_then(Value::as_u64) == Some(http_status)
                    && receipt.get("byte_count").and_then(Value::as_u64)
                        == Some(manifest_byte_count)
                    && receipt.get("content_kind").and_then(Value::as_str)
                        == Some(manifest_content_kind)
                    && sha256_matches(
                        receipt.get("sha256").and_then(Value::as_str),
                        normalized_snapshot_hash,
                    )
                    && sha256_matches(
                        receipt.get("sha256").and_then(Value::as_str),
                        manifest_body_hash,
                    )
                    && data_artifact_matches(root, doc, receipt, normalized_snapshot_hash)
                    && extracted_text_matches(doc, extracted_text_sha256)
            });
        if matching_entry.is_none() {
            let mut observed_scores = entries
                .values()
                .chain(receipt_history.iter())
                .filter_map(|entry| {
                    let receipt = entry.pointer("/doc/response_receipt")?;
                    (receipt.get("requested_url").and_then(Value::as_str)
                        == Some(manifest_request_url)
                        && receipt.get("final_url").and_then(Value::as_str)
                            == Some(manifest_final_url)
                        && entry.get("checked_at").and_then(Value::as_u64)
                            == Some(manifest_checked_at))
                    .then(|| {
                        entry
                            .get("evidence_relevance_score")
                            .and_then(Value::as_i64)
                    })
                    .flatten()
                })
                .collect::<Vec<_>>();
            observed_scores.sort_unstable();
            observed_scores.dedup();
            unmatched.push(format!(
                "{evidence_id}(manifest_score={relevance_score}, server_scores={observed_scores:?}, url={canonical_url})"
            ));
        }
    }
    anyhow::ensure!(
        unmatched.is_empty(),
        "evidence is not bound to a matching admitted CTOX Web Stack retrieval; unmatched entries: {}",
        unmatched.join("; ")
    );
    Ok(())
}

fn validate_systematic_research_workspace(
    root: &Path,
    job: &QueuedPrompt,
    expected_attempt_id: &str,
    research_started_at: u64,
) -> Result<PathBuf> {
    use sha2::Digest;

    let (expected_run_id, expected_command_id) = systematic_research_binding(job)?;
    let workspace = job
        .workspace_root
        .as_deref()
        .map(PathBuf::from)
        .context("systematic research requires a typed workspace root")?;
    if !workspace.is_dir() {
        anyhow::bail!(
            "systematic research workspace does not exist: {}",
            workspace.display()
        );
    }
    let manifest = workspace.join("validation/evidence-manifest.json");
    if !manifest.is_file() {
        anyhow::bail!(
            "no evidence manifest found at {}; systematic research accepts only this server-defined path",
            manifest.display()
        );
    }

    let validator =
        root.join("src/skills/system/research/systematic-research/scripts/evidence_guard.py");
    if !validator.is_file() {
        anyhow::bail!(
            "systematic research evidence guard is missing: {}",
            validator.display()
        );
    }

    let mut checked = Vec::new();
    {
        let manifest_bytes = std::fs::read(&manifest)?;
        let manifest_value: Value = serde_json::from_slice(&manifest_bytes)
            .with_context(|| format!("parse evidence manifest {}", manifest.display()))?;
        let actual_run_id = manifest_value
            .get("research_run_id")
            .and_then(Value::as_str)
            .context("evidence manifest is missing research_run_id")?;
        let actual_command_id = manifest_value
            .get("research_command_id")
            .and_then(Value::as_str)
            .context("evidence manifest is missing research_command_id")?;
        let actual_attempt_id = manifest_value
            .get("research_attempt_id")
            .and_then(Value::as_str)
            .context("evidence manifest is missing research_attempt_id")?;
        let manifest_run_id = manifest_value
            .get("run_id")
            .and_then(Value::as_str)
            .context("evidence manifest is missing run_id")?;
        // The attempt id deliberately does NOT take part in this comparison.
        // A systematic research run spans many harness turns - every retry
        // starts a new attempt - and the manifest is the run's accumulated
        // evidence, not one turn's output. Requiring the current attempt id
        // invalidated the previous turn's work on every retry: the model had
        // to spend its budget rebinding the manifest instead of researching,
        // and the SKF baseline died with three identical binding fields and a
        // single differing attempt. Run id and command id bind the manifest to
        // this run; the attempt is recorded for provenance only.
        let _ = actual_attempt_id;
        if actual_run_id != expected_run_id
            || manifest_run_id != expected_run_id
            || actual_command_id != expected_command_id
        {
            anyhow::bail!(
                "stale or foreign evidence manifest {}: expected run/command/attempt {expected_run_id}/{expected_command_id}/{expected_attempt_id}, found {actual_run_id}/{actual_command_id}/{actual_attempt_id}",
                manifest.display()
            );
        }
        let coverage = required_systematic_research_coverage(job);
        let verified_source_count = manifest_value
            .get("sources")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default();
        anyhow::ensure!(
            verified_source_count >= coverage.target_verified_sources,
            "systematic research target requires at least {} verified sources in the evidence manifest; found {}",
            coverage.target_verified_sources,
            verified_source_count
        );
        let discovery_receipt =
            validate_systematic_research_discovery_receipt(job, &workspace, research_started_at)?;
        let discovery_coverage = validate_systematic_research_discovery_coverage(
            job,
            &discovery_receipt,
            research_started_at,
        )?;
        validate_systematic_research_typed_web_read_receipts(
            job,
            &workspace,
            &manifest_value,
            research_started_at,
        )?;
        validate_systematic_research_web_receipts(root, &manifest_value, research_started_at)?;
        let output = Command::new("python3")
            .arg(&validator)
            .arg(&manifest)
            .arg("--base-dir")
            .arg(&workspace)
            .output()
            .with_context(|| {
                format!(
                    "execute systematic research evidence guard for {}",
                    manifest.display()
                )
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            anyhow::bail!(
                "evidence guard rejected {}: {}{}",
                manifest.display(),
                stdout,
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!("; stderr={}", clip_text(&stderr, 500))
                }
            );
        }
        checked.push(serde_json::json!({
            "manifest_path": manifest,
            "manifest_sha256": format!("{:x}", sha2::Sha256::digest(&manifest_bytes)),
            "validator_output": stdout,
            "discovery_receipt": discovery_receipt,
            "discovery_coverage": discovery_coverage,
            "verified_source_count": verified_source_count,
            "target_verified_sources": coverage.target_verified_sources,
        }));
    }

    let receipt_path = systematic_research_validation_receipt_path(job)
        .context("systematic research validation receipt requires a workspace root")?;
    let parent = receipt_path
        .parent()
        .context("systematic research validation receipt has no parent")?;
    std::fs::create_dir_all(parent)?;
    let receipt = serde_json::json!({
        "schema_version": "ctox.systematic-research.validation.v1",
        "status": "pass",
        "checked_at": now_iso_string(),
        "research_run_id": expected_run_id,
        "research_command_id": expected_command_id,
        "research_attempt_id": expected_attempt_id,
        "validator_path": validator,
        "manifests": checked,
    });
    std::fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt_path)
}
