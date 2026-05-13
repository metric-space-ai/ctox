// Origin: CTOX
// License: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const ALLOW_MARKER: &str = "ctox-allow-direct-state-write";

    const CORE_GUARDS: &[&str] = &[
        "enforce_core_transition",
        "enforce_queue_route_status_transition",
        "enforce_ticket_event_route_status_transition",
        "enforce_ticket_case_create_transition",
        "enforce_ticket_self_work_state_transition",
        "enforce_ticket_case_state_transition",
        "enforce_ticket_case_close_transition",
    ];

    const PROTECTED_STATE_COLUMNS: &[ProtectedStateColumn] = &[
        ProtectedStateColumn {
            table: "communication_routing_state",
            column: "route_status",
            entity: "QueueItem",
        },
        ProtectedStateColumn {
            table: "ticket_event_routing_state",
            column: "route_status",
            entity: "QueueItem",
        },
        ProtectedStateColumn {
            table: "ticket_self_work_items",
            column: "state",
            entity: "WorkItem",
        },
        ProtectedStateColumn {
            table: "ticket_cases",
            column: "state",
            entity: "Ticket",
        },
    ];

    #[derive(Debug, Clone, Copy)]
    struct ProtectedStateColumn {
        table: &'static str,
        column: &'static str,
        entity: &'static str,
    }

    #[derive(Debug)]
    struct Violation {
        path: PathBuf,
        line: usize,
        table: &'static str,
        column: &'static str,
        entity: &'static str,
        statement_kind: &'static str,
        source: String,
    }

    #[test]
    fn protected_state_writes_are_core_guarded_or_explicit_test_fixtures() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let violations = scan_tracked_sources(&root).expect("state-write guard scan failed");
        assert!(
            violations.is_empty(),
            "protected state writes must go through core transition guards or carry an explicit `{ALLOW_MARKER}` test-fixture marker:\n{}",
            format_violations(&root, &violations)
        );
    }

    fn scan_tracked_sources(root: &Path) -> Result<Vec<Violation>, String> {
        let mut violations = Vec::new();
        for path in tracked_rust_sources(root)? {
            let absolute = root.join(&path);
            let contents = fs::read_to_string(&absolute)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            scan_source_file(&path, &contents, &mut violations);
        }
        Ok(violations)
    }

    fn tracked_rust_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("ls-files")
            .arg("--")
            .arg("*.rs")
            .output()
            .map_err(|err| format!("failed to run git ls-files: {err}"))?;
        if !output.status.success() {
            return rust_sources_from_src_tree(root);
        }
        let mut paths = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.starts_with("src/"))
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }

    fn rust_sources_from_src_tree(root: &Path) -> Result<Vec<PathBuf>, String> {
        let mut paths = Vec::new();
        collect_rust_sources(&root.join("src"), root, &mut paths)?;
        paths.sort();
        Ok(paths)
    }

    fn collect_rust_sources(
        dir: &Path,
        root: &Path,
        paths: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(format!("failed to read {}: {err}", dir.display())),
        };
        for entry in entries {
            let entry =
                entry.map_err(|err| format!("failed to read entry in {}: {err}", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
            if file_type.is_dir() {
                collect_rust_sources(&path, root, paths)?;
            } else if file_type.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some("rs")
            {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|err| format!("failed to relativize {}: {err}", path.display()))?;
                paths.push(relative.to_path_buf());
            }
        }
        Ok(())
    }

    fn scan_source_file(path: &Path, contents: &str, violations: &mut Vec<Violation>) {
        let lines = contents.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
        let lower_lines = lines
            .iter()
            .map(|line| line.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let fixture_ranges = explicit_test_fixture_ranges(&lines);
        for index in 0..lines.len() {
            let Some(write) = protected_write_at(&lower_lines, index) else {
                continue;
            };
            if has_core_guard_nearby(&lines, index)
                || has_local_allow_marker(&lines, index)
                || fixture_ranges.iter().any(|range| range.contains(&index))
            {
                continue;
            }
            violations.push(Violation {
                path: path.to_path_buf(),
                line: index + 1,
                table: write.table,
                column: write.column,
                entity: write.entity,
                statement_kind: write.statement_kind,
                source: lines[index].trim().to_string(),
            });
        }
    }

    fn protected_write_at(lower_lines: &[String], index: usize) -> Option<ProtectedWrite<'static>> {
        let current_line = lower_lines[index].as_str();
        let window = lower_lines[index..lower_lines.len().min(index + 18)].join(" ");
        let compact = collapse_sql_whitespace(&window);
        for protected in PROTECTED_STATE_COLUMNS {
            let update_start = format!("update {}", protected.table);
            let insert_start = format!("insert into {}", protected.table);
            if !current_line.contains(&update_start) && !current_line.contains(&insert_start) {
                continue;
            }
            if mutates_protected_column(&compact, protected, "update") {
                return Some(ProtectedWrite {
                    table: protected.table,
                    column: protected.column,
                    entity: protected.entity,
                    statement_kind: "UPDATE",
                });
            }
            if mutates_protected_column(&compact, protected, "insert") {
                return Some(ProtectedWrite {
                    table: protected.table,
                    column: protected.column,
                    entity: protected.entity,
                    statement_kind: "INSERT",
                });
            }
        }
        None
    }

    #[derive(Debug)]
    struct ProtectedWrite<'a> {
        table: &'a str,
        column: &'a str,
        entity: &'a str,
        statement_kind: &'static str,
    }

    fn mutates_protected_column(
        compact: &str,
        protected: &ProtectedStateColumn,
        kind: &str,
    ) -> bool {
        match kind {
            "update" => {
                let Some(start) = compact.find(&format!("update {}", protected.table)) else {
                    return false;
                };
                let tail = &compact[start..];
                let Some(set_clause) = tail.split_once(" set ").map(|(_, value)| value) else {
                    return false;
                };
                let set_clause = set_clause.split(" where ").next().unwrap_or(set_clause);
                contains_column_assignment(set_clause, protected.column)
            }
            "insert" => {
                let Some(start) = compact.find(&format!("insert into {}", protected.table)) else {
                    return false;
                };
                let tail = &compact[start..];
                let column_region = tail
                    .split(" values ")
                    .next()
                    .unwrap_or(tail)
                    .split(" select ")
                    .next()
                    .unwrap_or(tail);
                column_region
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .any(|token| token == protected.column)
            }
            _ => false,
        }
    }

    fn contains_column_assignment(set_clause: &str, column: &str) -> bool {
        let needle = format!("{column}");
        set_clause
            .split(',')
            .any(|assignment| match assignment.split_once('=') {
                Some((left, _)) => left
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .any(|token| token == needle),
                None => false,
            })
    }

    fn collapse_sql_whitespace(input: &str) -> String {
        input.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn has_core_guard_nearby(lines: &[String], index: usize) -> bool {
        let start = index.saturating_sub(40);
        let end = lines.len().min(index + 8);
        lines[start..end].iter().any(|line| {
            let trimmed = line.trim();
            CORE_GUARDS.iter().any(|guard| trimmed.contains(guard))
        })
    }

    fn has_local_allow_marker(lines: &[String], index: usize) -> bool {
        let start = index.saturating_sub(30);
        let end = lines.len().min(index + 2);
        lines[start..end]
            .iter()
            .any(|line| line.contains(ALLOW_MARKER))
    }

    fn explicit_test_fixture_ranges(lines: &[String]) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("mod tests {") {
                continue;
            }
            let has_cfg_test = lines[index.saturating_sub(3)..=index]
                .iter()
                .any(|candidate| candidate.contains("#[cfg(test)]"));
            if !has_cfg_test {
                continue;
            }
            let marker_end = lines.len().min(index + 20);
            let has_marker = lines[index..marker_end]
                .iter()
                .any(|candidate| candidate.contains(ALLOW_MARKER));
            if has_marker {
                ranges.push(index..lines.len());
            }
        }
        ranges
    }

    fn format_violations(root: &Path, violations: &[Violation]) -> String {
        violations
            .iter()
            .map(|violation| {
                format!(
                    "- {}:{}: {} {}.{} ({}) without nearby core guard: {}",
                    root.join(&violation.path).display(),
                    violation.line,
                    violation.statement_kind,
                    violation.table,
                    violation.column,
                    violation.entity,
                    violation.source
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
