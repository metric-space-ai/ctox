// Origin: CTOX
// License: AGPL-3.0-only

//! Transport-neutral packages for a captured source, with atomic remote publish.
//! The caller authorizes the endpoint and owns bounded upload/execution. No host
//! table, credential, SSH connection or build admission is provided here.

use super::build_source::CapturedBuildSource;
use super::computer_capabilities::{validate_capabilities, BuildCapability, ComputerCapability};
use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
pub struct SourceUpload {
    pub local: PathBuf,
    pub remote_name: &'static str,
}

#[derive(Debug)]
pub struct BuildSourceDelivery {
    _staging: tempfile::TempDir,
    pub source_id: String,
    pub source_dir: String,
    pub incoming_dir: String,
    pub uploads: Vec<SourceUpload>,
    /// Run with Bash, with a 15 minute preparation deadline. Completion means
    /// verified source publication, never slot admission or a successful build.
    pub prepare_script: String,
}

#[derive(Serialize)]
struct Entry {
    path: String,
    executable: bool,
    symlink_target: Option<String>,
    size: Option<u64>,
    sha256: Option<String>,
}

#[derive(Serialize)]
struct Manifest<'a> {
    schema: &'static str,
    source_id: &'a str,
    head: &'a str,
    repository: Option<&'a str>,
    base: Option<&'a str>,
    entries: &'a [Entry],
    uploaded: &'a [String],
    deleted: &'a [String],
}

fn relative(path: &str) -> Result<()> {
    anyhow::ensure!(
        !path.is_empty()
            && Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && !path.split('/').any(|part| part == ".git"),
        "unsafe source delivery path"
    );
    Ok(())
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn quote(value: &str) -> Result<String> {
    anyhow::ensure!(
        !value.contains('\0'),
        "source preparation argument contains NUL"
    );
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

/// Package only an unmodified result of build_source::capture. The caller
/// supplies a non-secret SHA256 of the actual toolchain/profile, not merely a
/// mutable toolchain name. Different compiler states receive different targets.
pub fn package(
    grant: &BuildCapability,
    source: &CapturedBuildSource,
    staging_root: &Path,
    run_id: &str,
    toolchain_fingerprint: &str,
) -> Result<BuildSourceDelivery> {
    validate_capabilities(&mut vec![ComputerCapability::Build(grant.clone())], false)?;
    anyhow::ensure!(
        !run_id.is_empty()
            && run_id.len() <= 128
            && run_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid source delivery run"
    );
    anyhow::ensure!(
        hex(&source.source_id, 64)
            && hex(&source.head_revision, 40)
            && hex(toolchain_fingerprint, 64),
        "invalid source or toolchain identity"
    );
    if let Some(base) = &source.public_base {
        let parts: Vec<_> = base.repository.split('/').collect();
        anyhow::ensure!(
            parts.len() == 2
                && parts.iter().all(|p| !p.is_empty()
                    && *p != "."
                    && *p != ".."
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)))
                && hex(&base.revision, 40),
            "invalid anonymous GitHub base"
        );
    }
    let source_id = format!(
        "{:x}",
        Sha256::digest(format!(
            "ctox.build-delivery.v1:{}:{}",
            source.source_id, toolchain_fingerprint
        ))
    );
    let tree = source.tree();
    let staging_root = staging_root.canonicalize()?;
    anyhow::ensure!(
        !staging_root.starts_with(&tree),
        "delivery staging is inside source"
    );
    let staging = tempfile::Builder::new()
        .prefix("ctox-source-delivery-")
        .tempdir_in(staging_root)?;
    let mut entries = Vec::new();
    for file in &source.files {
        relative(&file.path)?;
        let path = tree.join(&file.path);
        let metadata = fs::symlink_metadata(&path)?;
        let (size, sha256) = if let Some(target) = &file.symlink_target {
            anyhow::ensure!(
                metadata.file_type().is_symlink()
                    && fs::read_link(&path)?.to_str() == Some(target.as_str()),
                "frozen link changed"
            );
            (None, None)
        } else {
            anyhow::ensure!(metadata.is_file(), "frozen file changed type");
            let mut input = fs::File::open(path)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            (Some(metadata.len()), Some(format!("{:x}", hash.finalize())))
        };
        entries.push(Entry {
            path: file.path.clone(),
            executable: file.executable,
            symlink_target: file.symlink_target.clone(),
            size,
            sha256,
        });
    }
    let uploaded = if source.public_base.is_some() {
        source.overlay_paths.clone()
    } else {
        source.files.iter().map(|f| f.path.clone()).collect()
    };
    for path in uploaded.iter().chain(source.deleted_paths.iter()) {
        relative(path)?;
    }
    anyhow::ensure!(
        uploaded
            .iter()
            .all(|p| entries.iter().any(|e| &e.path == p)),
        "overlay is outside frozen source"
    );
    let names = staging.path().join("names");
    let mut nul_names = Vec::new();
    for path in &uploaded {
        nul_names.extend_from_slice(path.as_bytes());
        nul_names.push(0);
    }
    fs::write(&names, nul_names)?;
    let archive = staging.path().join("source.tar");
    // NUL input keeps filenames beginning with '-' or containing newlines literal.
    let status = Command::new("tar")
        .arg("--create")
        .arg("--file")
        .arg(&archive)
        .arg("--directory")
        .arg(&tree)
        .arg("--null")
        .arg("--no-recursion")
        .arg("--files-from")
        .arg(&names)
        .status()
        .context("package frozen source")?;
    anyhow::ensure!(status.success(), "source archive creation failed");
    fs::remove_file(names)?;
    let manifest = Manifest {
        schema: "ctox.build-delivery.v1",
        source_id: &source_id,
        head: &source.head_revision,
        repository: source.public_base.as_ref().map(|b| b.repository.as_str()),
        base: source.public_base.as_ref().map(|b| b.revision.as_str()),
        entries: &entries,
        uploaded: &uploaded,
        deleted: &source.deleted_paths,
    };
    let manifest_path = staging.path().join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
    let mut uploads = vec![
        SourceUpload {
            local: archive,
            remote_name: "source.tar",
        },
        SourceUpload {
            local: manifest_path,
            remote_name: "manifest.json",
        },
    ];
    if let Some(bundle) = &source.bundle {
        let destination = staging.path().join("commits.bundle");
        fs::copy(bundle, &destination)?;
        uploads.push(SourceUpload {
            local: destination,
            remote_name: "commits.bundle",
        });
    }
    let source_dir = format!("{}/sources/{source_id}", grant.lane_root);
    let incoming_dir = format!("{}/incoming/{run_id}", grant.lane_root);
    let mut prepare_script = format!(
        "#!/bin/bash\nset -eu\numask 077\ntimeout --signal=TERM --kill-after=10s 900s python3 - {} {} {} <<'CTOX_SOURCE_PY'\n",
        quote(&grant.lane_root)?, quote(run_id)?, grant.disk_floor_gib,
    );
    prepare_script.push_str(PREPARE);
    prepare_script.push_str("\nCTOX_SOURCE_PY\n");
    Ok(BuildSourceDelivery {
        _staging: staging,
        source_id,
        source_dir,
        incoming_dir,
        uploads,
        prepare_script,
    })
}

const PREPARE: &str = r#"import hashlib,json,os,pathlib,stat,subprocess,sys,tarfile
import fcntl
def require(condition,message):
    if not condition: raise RuntimeError(message)
root=pathlib.Path(sys.argv[1])
run=sys.argv[2]
require(root.resolve(strict=True)==root and root.is_dir(), "lane root is not canonical")
require(os.statvfs(root).f_bavail*os.statvfs(root).f_frsize >= int(sys.argv[3])*1024**3, "disk floor")
incoming=root/"incoming"/run
require(incoming.resolve(strict=True)==incoming and incoming.is_dir(), "unsafe incoming directory")
raw=(incoming/"manifest.json").read_bytes()
manifest=json.loads(raw)
require(manifest["schema"]=="ctox.build-delivery.v1", "unknown manifest")
identity=manifest["source_id"]
require(len(identity)==64 and all(c in "0123456789abcdef" for c in identity), "invalid identity")
def safe(name):
    p=pathlib.PurePosixPath(name)
    require(name and not p.is_absolute() and p.as_posix()==name and all(c not in (".","..",".git") for c in p.parts), "unsafe source path")
    return p.parts
entries={e["path"]:e for e in manifest["entries"]}
require(len(entries)==len(manifest["entries"]), "duplicate source entry")
for name in list(entries)+manifest["deleted"]+manifest["uploaded"]: safe(name)
require(len(set(manifest["uploaded"]))==len(manifest["uploaded"]), "duplicate upload")
require(set(manifest["uploaded"])<=entries.keys(), "unknown upload")
def owned_directory(path):
    if not path.exists(): path.mkdir(mode=0o700)
    require(path.resolve(strict=True)==path and path.is_dir(), "unsafe lane directory")
sources=root/"sources"
prepared=root/"prepared"
owned_directory(sources)
owned_directory(prepared)
locks=root/"source-locks"
owned_directory(locks)
lock_path=locks/(identity+".lock")
require(not lock_path.is_symlink(), "unsafe source lock")
publication_lock=lock_path.open("a")
try: fcntl.flock(publication_lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
except BlockingIOError: sys.exit(75)
destination=sources/identity
marker=prepared/(identity+".json")
def file_path(base,name,create=False):
    parts=safe(name)
    parent=base
    for part in parts[:-1]:
        parent=parent/part
        if create and not parent.exists(): parent.mkdir(mode=0o700)
        require(parent.is_dir() and not parent.is_symlink(), "source parent is not a directory")
    return parent/parts[-1]
def verify(base):
    require(base.resolve(strict=True)==base and base.is_dir(), "unsafe published source")
    seen=set()
    for directory,dirs,files in os.walk(base,followlinks=False):
        if pathlib.Path(directory)==base and manifest["repository"] is not None:
            dirs[:]=[d for d in dirs if d!=".git"]
        for d in dirs[:]:
            path=pathlib.Path(directory)/d
            if path.is_symlink():
                dirs.remove(d)
                files.append(d)
        for name in files: seen.add((pathlib.Path(directory)/name).relative_to(base).as_posix())
    require(seen==entries.keys(), "source tree differs from capture")
    for name,entry in entries.items():
        path=file_path(base,name)
        mode=path.lstat().st_mode
        if entry["symlink_target"] is not None:
            require(stat.S_ISLNK(mode) and os.readlink(path)==entry["symlink_target"], "link differs")
        else:
            require(stat.S_ISREG(mode) and path.stat().st_size==entry["size"] and bool(mode&0o111)==entry["executable"], "file metadata differs")
            h=hashlib.sha256()
            with path.open("rb") as f:
                for chunk in iter(lambda:f.read(65536),b""): h.update(chunk)
            require(h.hexdigest()==entry["sha256"], "source hash differs")
if destination.exists() or destination.is_symlink():
    require(marker.is_file() and not marker.is_symlink() and marker.read_bytes()==raw, "unverified source already exists")
    verify(destination)
    print("SOURCE_ALREADY_PREPARED",identity)
    sys.exit(0)
stage=sources/("."+identity+"-"+run+".partial")
stage.mkdir(mode=0o700)
if manifest["repository"] is not None:
    parts=manifest["repository"].split("/")
    require(len(parts)==2 and all(p and p not in (".","..") and all(c.isascii() and (c.isalnum() or c in "-_.") for c in p) for p in parts), "unsafe public repository")
    for revision in [manifest["base"],manifest["head"]]:
        require(len(revision)==40 and all(c in "0123456789abcdef" for c in revision), "invalid public revision")
    environment={k:v for k,v in os.environ.items() if not k.startswith("GIT_") and k!="SSH_AUTH_SOCK"}
    environment.update(GIT_CONFIG_NOSYSTEM="1",GIT_CONFIG_GLOBAL="/dev/null",GIT_TERMINAL_PROMPT="0")
    def git(*args,directory=stage,check=True):
        return subprocess.run(["git","-c","credential.helper=","-c","core.hooksPath=/dev/null","-C",str(directory),*args],
            env=environment,stdin=subprocess.DEVNULL,stderr=None if check else subprocess.DEVNULL,
            check=check,timeout=300)
    mirrors=root/"git-mirrors"
    owned_directory(mirrors)
    mirror_key=hashlib.sha256(manifest["repository"].encode()).hexdigest()
    mirror=mirrors/mirror_key
    owned_directory(mirror)
    mirror_lock_path=locks/("github-"+mirror_key+".lock")
    require(not mirror_lock_path.is_symlink(), "unsafe mirror lock")
    with mirror_lock_path.open("a") as mirror_lock:
        try: fcntl.flock(mirror_lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError: sys.exit(75)
        if not (mirror/"HEAD").exists(): git("init","-q","--bare",directory=mirror)
        reference="refs/heads/ctox-base-"+manifest["base"]
        if git("cat-file","-e",manifest["base"]+"^{commit}",directory=mirror,check=False).returncode:
            git("fetch","-q","--no-tags","https://github.com/"+manifest["repository"]+".git",
                manifest["base"],directory=mirror)
        git("update-ref",reference,manifest["base"],directory=mirror)
        git("symbolic-ref","HEAD",reference,directory=mirror)
        # Independent objects survive mirror cleanup; no alternates or hardlinks.
        git("clone","-q","--no-checkout","--no-hardlinks",str(mirror),str(stage),directory=mirrors)
    bundle=incoming/"commits.bundle"
    if manifest["head"]!=manifest["base"]:
        require(bundle.is_file() and not bundle.is_symlink(), "commit bundle absent")
        git("bundle","unbundle",str(bundle))
    git("checkout","-q","--detach",manifest["head"])
for name in manifest["deleted"] if manifest["repository"] is not None else []:
    path=file_path(stage,name)
    if path.exists() or path.is_symlink():
        require(not path.is_dir() or path.is_symlink(), "deletion is a directory")
        path.unlink()
seen=set()
with tarfile.open(incoming/"source.tar",mode="r:") as archive:
    for member in archive:
        name=member.name
        safe(name)
        require(name in manifest["uploaded"] and name not in seen, "unexpected archive member")
        seen.add(name)
        entry=entries[name]
        path=file_path(stage,name,True)
        if path.exists() or path.is_symlink():
            require(not path.is_dir() or path.is_symlink(), "overlay is a directory")
            path.unlink()
        if entry["symlink_target"] is not None:
            require(member.issym() and member.linkname==entry["symlink_target"], "archive link differs")
            path.symlink_to(member.linkname)
        else:
            require(member.isfile() and member.size==entry["size"], "archive file differs")
            stream=archive.extractfile(member)
            with path.open("xb") as output:
                for chunk in iter(lambda:stream.read(65536),b""): output.write(chunk)
                output.flush()
                os.fsync(output.fileno())
            path.chmod(0o700 if entry["executable"] else 0o600)
require(seen==set(manifest["uploaded"]), "archive members missing")
verify(stage)
# The source lock spans verification and publish, including cache reuse.
# No replacement or reuse of an unverified source tree is permitted.
require(not destination.exists() and not destination.is_symlink(), "source publication raced")
os.rename(stage,destination)
temporary=prepared/("."+identity+"-"+run+".tmp")
with temporary.open("xb") as output:
    output.write(raw)
    output.flush()
    os.fsync(output.fileno())
os.rename(temporary,marker)
for directory in [sources,prepared]:
    descriptor=os.open(directory,os.O_RDONLY|os.O_DIRECTORY)
    try: os.fsync(descriptor)
    finally: os.close(descriptor)
print("SOURCE_PREPARED",identity)
"#;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::business_os::build_source::{capture, PublicGithubBase};
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, BuildCapability) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let staging = temp.path().join("staging");
        let lane = temp.path().join("lane ' $(touch INJECTED)");
        for path in [&root, &staging, &lane] {
            fs::create_dir(path).unwrap();
        }
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "delivery@example.invalid"]);
        git(&root, &["config", "user.name", "DeliveryFixture"]);
        fs::write(root.join(".gitignore"), "ignored\n").unwrap();
        fs::write(root.join("tracked"), "committed").unwrap();
        fs::create_dir(root.join("old-parent")).unwrap();
        fs::write(root.join("old-parent/deleted"), "deleted").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "base"]);
        let grant = BuildCapability {
            ssh_endpoint_ref: "fixture".into(),
            slots: 1,
            jobs: 2,
            lane_root: lane.to_str().unwrap().into(),
            disk_floor_gib: 1,
            toolchains: vec!["rust".into()],
        };
        (temp, root, staging, grant)
    }

    fn upload(delivery: &BuildSourceDelivery) {
        fs::create_dir_all(&delivery.incoming_dir).unwrap();
        for item in &delivery.uploads {
            fs::copy(
                &item.local,
                Path::new(&delivery.incoming_dir).join(item.remote_name),
            )
            .unwrap();
        }
    }

    fn prepare(delivery: &BuildSourceDelivery) -> std::process::Output {
        Command::new("bash")
            .arg("-c")
            .arg(&delivery.prepare_script)
            .output()
            .unwrap()
    }

    #[test]
    fn private_source_is_verified_and_reused_with_literal_names_links_and_modes() {
        let (_temp, root, staging, grant) = fixture();
        fs::remove_file(root.join("old-parent/deleted")).unwrap();
        fs::write(root.join("ignored"), "not exported").unwrap();
        fs::write(
            root.join("--checkpoint-action=exec=touch INJECTED"),
            "literal option",
        )
        .unwrap();
        fs::write(root.join("name\nwith'quotes"), "frozen").unwrap();
        fs::set_permissions(root.join("tracked"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("name\nwith'quotes", root.join("link")).unwrap();
        let snapshot = capture(&root, &staging, None).unwrap();
        let delivery = package(&grant, &snapshot, &staging, "first", &"a".repeat(64)).unwrap();
        // Delivery owns its archive independently of capture and the live checkout.
        fs::write(root.join("name\nwith'quotes"), "later").unwrap();
        drop(snapshot);
        upload(&delivery);
        let result = prepare(&delivery);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let target = Path::new(&delivery.source_dir);
        assert_eq!(
            fs::read(target.join("name\nwith'quotes")).unwrap(),
            b"frozen"
        );
        assert_eq!(
            fs::read_link(target.join("link")).unwrap(),
            PathBuf::from("name\nwith'quotes")
        );
        assert!(
            fs::metadata(target.join("tracked"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111
                != 0
        );
        assert!(!target.join(".git").exists());
        assert!(!target.join("ignored").exists());
        assert!(!target.join("old-parent/deleted").exists());
        assert!(!target.join("INJECTED").exists());
        assert!(prepare(&delivery).status.success());
        fs::write(target.join("tracked"), "corrupt cached source").unwrap();
        assert!(!prepare(&delivery).status.success());
    }

    #[test]
    fn corrupted_archive_never_publishes_a_source() {
        let (_temp, root, staging, grant) = fixture();
        let snapshot = capture(&root, &staging, None).unwrap();
        let delivery = package(&grant, &snapshot, &staging, "corrupt", &"a".repeat(64)).unwrap();
        upload(&delivery);
        // Same filename and length, wrong bytes: metadata alone cannot admit it.
        fs::write(root.join("tracked"), "wrongdata").unwrap();
        let archive = Path::new(&delivery.incoming_dir).join("source.tar");
        let status = Command::new("tar")
            .arg("-cf")
            .arg(archive)
            .arg("-C")
            .arg(&root)
            .args([".gitignore", "tracked", "old-parent/deleted"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!prepare(&delivery).status.success());
        assert!(!Path::new(&delivery.source_dir).exists());
        assert!(!Path::new(&grant.lane_root)
            .join("prepared")
            .join(format!("{}.json", delivery.source_id))
            .exists());
    }

    #[test]
    fn public_upload_contains_only_overlay_bundle_and_non_secret_manifest() {
        let (_temp, root, staging, grant) = fixture();
        let base = git(&root, &["rev-parse", "HEAD"]);
        fs::write(root.join("committed-later"), "local commit").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "unpushed"]);
        fs::write(root.join("dirty"), "dirty bytes").unwrap();
        let snapshot = capture(
            &root,
            &staging,
            Some(PublicGithubBase {
                repository: "metric-space-ai/ctox".into(),
                revision: base,
            }),
        )
        .unwrap();
        let first = package(&grant, &snapshot, &staging, "public", &"a".repeat(64)).unwrap();
        let second = package(
            &grant,
            &snapshot,
            &staging,
            "other_compiler",
            &"b".repeat(64),
        )
        .unwrap();
        assert_ne!(first.source_id, second.source_id);
        assert_eq!(first.uploads.len(), 3);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&first.uploads[1].local).unwrap()).unwrap();
        assert_eq!(manifest["uploaded"], serde_json::json!(["dirty"]));
        assert!(manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "committed-later"));
        let listing = Command::new("tar")
            .arg("-tf")
            .arg(&first.uploads[0].local)
            .output()
            .unwrap();
        assert!(listing.status.success());
        assert_eq!(String::from_utf8(listing.stdout).unwrap(), "dirty\n");
        assert!(first.prepare_script.contains("credential.helper="));
        assert!(first.prepare_script.contains("core.hooksPath=/dev/null"));
        // Preseed a trusted mirror fixture. This exercises actual offline
        // checkout/bundle/overlay reconstruction, not GitHub network availability.
        let mirror_key = format!("{:x}", Sha256::digest(b"metric-space-ai/ctox"));
        let mirror = Path::new(&grant.lane_root)
            .join("git-mirrors")
            .join(mirror_key);
        fs::create_dir_all(&mirror).unwrap();
        git(&mirror, &["init", "-q", "--bare"]);
        git(
            &mirror,
            &[
                "fetch",
                "-q",
                root.to_str().unwrap(),
                &snapshot.public_base.as_ref().unwrap().revision,
            ],
        );
        upload(&first);
        let result = prepare(&first);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let target = Path::new(&first.source_dir);
        assert_eq!(
            fs::read(target.join("committed-later")).unwrap(),
            b"local commit"
        );
        assert_eq!(fs::read(target.join("dirty")).unwrap(), b"dirty bytes");
        assert!(target.join(".git").is_dir());
    }
}
