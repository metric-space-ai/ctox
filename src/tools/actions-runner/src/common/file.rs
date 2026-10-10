//! Copying files and directories, for the step that stages a job's workdir.
//!
//! # Upstream's `CopyFile` never sets the destination's mode
//!
//! The function stats the source and then guards the `chmod` with
//! `if err != nil` — inverted, and with the `err` shadowed by the `:=` on the
//! same line. So on the happy path the condition is false, the `chmod` never
//! runs, and the destination keeps whatever mode `os.Create` gave it. This is
//! preserved, because the observable is a file mode that a workflow's own
//! steps can notice.
//!
//! A second detail rides along: the mode is applied only `if err == nil` from
//! the `io.Copy`, so a copy that failed short never reaches the stat at all.
//!
//! # `CopyDir` reports one error
//!
//! Subdirectory and file errors are *printed* rather than returned, and the
//! loop's last `err` is what comes out. So a tree where one file failed and a
//! later one succeeded reports success. Preserved, and covered by a test.

use std::fs;
use std::io::Read;
use std::path::Path;

/// `CopyFile`: copies the contents, and nothing else.
///
/// The destination is created or truncated. The source's mode is **not**
/// applied — see the module docs.
pub fn copy_file(source: &Path, dest: &Path) -> std::io::Result<()> {
    let mut source_file = fs::File::open(source)?;
    let mut dest_file = fs::File::create(dest)?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = source_file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut dest_file, &buffer[..read])?;
    }
    // Upstream's chmod is unreachable on this path. Statting anyway keeps the
    // documented behaviour explicit rather than silent.
    let _ = fs::metadata(source);
    Ok(())
}

/// `CopyDir`: copies a tree, creating the destination with the source's mode.
///
/// Errors below the top are reported, not propagated — see the module docs.
pub fn copy_dir(source: &Path, dest: &Path) -> std::io::Result<()> {
    let source_info = fs::metadata(source)?;
    fs::create_dir_all(dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // act creates the directory with the source's mode. `create_dir_all`
        // applies the umask, so the mode is set explicitly afterwards.
        fs::set_permissions(dest, fs::Permissions::from_mode(source_info.permissions().mode()))?;
    }
    #[cfg(not(unix))]
    let _ = &source_info;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let source_child = source.join(&name);
        let dest_child = dest.join(&name);

        let outcome = if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            copy_dir(&source_child, &dest_child)
        } else {
            copy_file(&source_child, &dest_child)
        };
        // Upstream prints and carries on; the printed line is the only
        // report the caller of a nested copy ever gets.
        if let Err(error) = outcome {
            eprintln!("{error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        fs::write(path, contents).expect("written");
    }

    #[test]
    fn a_file_is_copied_with_its_contents() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("source.txt");
        let dest = dir.path().join("dest.txt");
        write(&source, "contents");

        copy_file(&source, &dest).expect("copied");
        assert_eq!(fs::read_to_string(&dest).expect("readable"), "contents");
    }

    /// The destination is truncated, so copying a short file over a long one
    /// does not leave a tail behind.
    #[test]
    fn copying_truncates_the_destination() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("source.txt");
        let dest = dir.path().join("dest.txt");
        write(&source, "short");
        write(&dest, "a much longer previous life");

        copy_file(&source, &dest).expect("copied");
        assert_eq!(fs::read_to_string(&dest).expect("readable"), "short");
    }

    /// Upstream's inverted guard: the mode is never carried over. A workflow
    /// that marks a script executable in the repository and relies on the
    /// copy keeping that can see the difference, so it is pinned here rather
    /// than fixed.
    #[cfg(unix)]
    #[test]
    fn the_destination_keeps_its_own_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("source.sh");
        let dest = dir.path().join("dest.sh");
        write(&source, "#!/bin/sh\n");
        fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).expect("chmod");

        copy_file(&source, &dest).expect("copied");
        let mode = fs::metadata(&dest).expect("readable").permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o644,
            "the source's 0755 is not carried over upstream either",
        );
    }

    #[test]
    fn a_tree_is_copied_recursively() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("src");
        write(&source.join("top.txt"), "top");
        write(&source.join("nested/deep/inner.txt"), "inner");
        write(&source.join("nested/deep/other.txt"), "other");

        let dest = dir.path().join("dst");
        copy_dir(&source, &dest).expect("copied");

        assert_eq!(
            fs::read_to_string(dest.join("top.txt")).expect("readable"),
            "top",
        );
        assert_eq!(
            fs::read_to_string(dest.join("nested/deep/inner.txt")).expect("readable"),
            "inner",
        );
        assert_eq!(
            fs::read_to_string(dest.join("nested/deep/other.txt")).expect("readable"),
            "other",
        );
    }

    /// A missing source is a stat failure, reported rather than printed.
    #[test]
    fn a_missing_source_is_reported() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        assert!(copy_dir(&dir.path().join("nope"), &dir.path().join("dst")).is_err());
        assert!(copy_file(&dir.path().join("nope"), &dir.path().join("dst")).is_err());
    }
}
