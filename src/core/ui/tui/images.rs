//! Clipboard capture and drag-and-drop resolution for chat image
//! attachments.
use super::*;

/// Capture the current system clipboard as a PNG file and return the path
/// on success. Uses platform-native CLIs so we don't take a dependency on
/// `arboard` / objc-crate chains — this keeps the CTOX build portable and
/// avoids another hard-failure surface for macOS sandboxing.
///
/// - macOS: `pbpaste -Prefer png` writes raw PNG bytes to stdout.
/// - Linux/Wayland: `wl-paste --type image/png`.
/// - Linux/X11: `xclip -selection clipboard -t image/png -o`.
///
/// Returns None if no image is on the clipboard or the platform CLI is
/// missing / returned empty output.
pub(super) fn capture_clipboard_image_to_tempfile() -> Option<PathBuf> {
    use std::process::Command;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let tempfile = std::env::temp_dir().join(format!("ctox-clipboard-{timestamp}.png"));

    #[cfg(target_os = "macos")]
    let output = Command::new("pbpaste").args(["-Prefer", "png"]).output();

    #[cfg(all(unix, not(target_os = "macos")))]
    let output = {
        // Prefer Wayland if available, else fall back to xclip.
        let wl = Command::new("wl-paste")
            .args(["--type", "image/png"])
            .output();
        match wl {
            Ok(result) if result.status.success() && !result.stdout.is_empty() => Ok(result),
            _ => Command::new("xclip")
                .args(["-selection", "clipboard", "-t", "image/png", "-o"])
                .output(),
        }
    };

    #[cfg(not(unix))]
    let output: std::io::Result<std::process::Output> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "clipboard image paste is only supported on macOS/Linux in this build",
    ));

    let data = match output {
        Ok(result) if result.status.success() && !result.stdout.is_empty() => result.stdout,
        _ => return None,
    };
    // Sanity-check: PNG signature.
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    std::fs::write(&tempfile, &data).ok()?;
    Some(tempfile)
}

/// Parse a pasted-string or slash-command argument into a candidate image
/// attachment. Returns Some only if the string points to an existing,
/// readable file with an image extension within the size limit.
pub(super) fn try_resolve_image_attachment(input: &str, cwd: &Path) -> Option<PendingImage> {
    let trimmed = input.trim().trim_matches(|c| c == '"' || c == '\'');
    if trimmed.is_empty() {
        return None;
    }
    let raw = PathBuf::from(trimmed);
    let candidate = if raw.is_absolute() {
        raw
    } else {
        cwd.join(raw)
    };
    let canonical = std::fs::canonicalize(&candidate).ok()?;
    let metadata = std::fs::metadata(&canonical).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let extension = canonical
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)?;
    if !IMAGE_EXTENSIONS.iter().any(|ext| *ext == extension) {
        return None;
    }
    if metadata.len() > MAX_IMAGE_ATTACHMENT_BYTES {
        return None;
    }
    Some(PendingImage {
        path: canonical,
        size_bytes: metadata.len(),
    })
}
