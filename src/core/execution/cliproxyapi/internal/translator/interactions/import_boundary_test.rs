// ref: internal/translator/interactions/import_boundary_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fs;
use std::path::Path;

fn collect_violations(root: &Path, directory: &Path, violations: &mut Vec<String>) {
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        let entries =
            fs::read_dir(&path).unwrap_or_else(|error| panic!("scan {}: {error}", path.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| panic!("scan {}: {error}", path.display()));
            let entry_path = entry.path();
            if entry_path.is_dir() {
                pending.push(entry_path);
            } else if entry_path.extension().and_then(|value| value.to_str()) == Some("rs")
                && imports_gemini_translator(&entry_path)
            {
                violations.push(relative_path(root, &entry_path));
            }
        }
    }
}

fn imports_gemini_translator(path: &Path) -> bool {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    source
        .match_indices("internal::translator::")
        .any(|(offset, _)| {
            source[offset..]
                .split_once(';')
                .map_or(&source[offset..], |(statement, _)| statement)
                .contains("::gemini")
        })
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
