// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded native build admission/steps. No bearer or SSH secret is persisted.
//! Each invocation authenticates the current owner, advances at most four chunks
//! or one remote stage, and releases its fenced local claim before returning.
use super::{
    build_delivery,
    build_job_store::{BuildJob, BuildJobStore, MAX_LEASE_MS},
    build_lane_runner,
    build_profile::{self, quote, RustBuildProfile},
    build_remote::{BuildRemote, RemoteRunStatus, UPLOAD_CHUNK_BYTES},
    build_source::{self, PublicGithubBase},
    computer_capabilities::{self, ComputerCapability},
    computer_endpoints::{ComputerEndpointRequest, EndpointUse},
};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    capability_token: String,
    request: Request,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Profile {
        computer_id: String,
        profile: RustBuildProfile,
    },
    Submit {
        source_root: PathBuf,
        staging_root: PathBuf,
        toolchain: String,
        computer_id: Option<String>,
        task_id: String,
        recipe: CargoRecipe,
        timeout_seconds: u32,
        public_base: Option<PublicBase>,
    },
    Step {
        job_id: String,
    },
    Status {
        job_id: String,
    },
    List {
        after_job_id: Option<String>,
    },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicBase {
    repository: String,
    revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoRecipe {
    kind: CargoKind,
    release: bool,
    manifest_path: Option<String>,
    package: Option<String>,
    test_filter: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CargoKind {
    Build,
    Check,
    Test,
}

struct Actor {
    owner: String,
    epoch: i64,
}

fn authenticate(root: &Path, token: &str) -> Result<Actor> {
    let (owner, role) = super::store::verify_unbound_capability_actor(root, token)
        .context("current unbound native owner capability required")?;
    ensure!(
        matches!(role.to_ascii_lowercase().as_str(), "chef" | "admin"),
        "Owner/Admin build authority required"
    );
    let claims = super::store::verified_capability_claims(root, token)
        .context("owner capability changed during authentication")?;
    ensure!(
        claims.user_id == owner && claims.role == role,
        "owner capability changed during authentication"
    );
    Ok(Actor {
        owner,
        epoch: claims.actor_epoch,
    })
}

fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "invalid opaque build identity"
    );
    Ok(())
}
fn relative(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && !value.contains('\0')
            && value.len() <= 4096
            && Path::new(value)
                .components()
                .all(|p| matches!(p, Component::Normal(_)))
            && !value.split('/').any(|p| p == ".git"),
        "invalid source-relative manifest path"
    );
    Ok(())
}

impl CargoRecipe {
    fn command(&self, profile: &RustBuildProfile, jobs: u16) -> Result<Vec<String>> {
        ensure!((1..=64).contains(&jobs), "invalid compiler worker cap");
        let action = match self.kind {
            CargoKind::Build => "build",
            CargoKind::Check => "check",
            CargoKind::Test => "test",
        };
        let mut cargo = vec![
            profile.cargo.clone(),
            action.into(),
            "--locked".into(),
            "--jobs".into(),
            jobs.to_string(),
        ];
        if self.release {
            cargo.push("--release".into());
        }
        if let Some(path) = &self.manifest_path {
            relative(path)?;
            cargo.extend(["--manifest-path".into(), path.clone()]);
        }
        if let Some(package) = &self.package {
            ensure!(
                !package.is_empty()
                    && package.len() <= 256
                    && package
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
                "invalid cargo package"
            );
            cargo.extend(["--package".into(), package.clone()]);
        }
        if let Some(filter) = &self.test_filter {
            ensure!(
                matches!(self.kind, CargoKind::Test)
                    && !filter.is_empty()
                    && !filter.starts_with('-')
                    && !filter.contains('\0')
                    && filter.len() <= 256,
                "invalid cargo test filter"
            );
            cargo.push(filter.clone());
        }
        if matches!(self.kind, CargoKind::Test) {
            cargo.extend(["--".into(), format!("--test-threads={jobs}")]);
        }
        let mut script = String::new();
        if let Some(prep) = &profile.ctox_prep {
            script.push_str(&format!("{} && ", quote(prep)?));
        }
        script.push_str("exec ");
        script.push_str(
            &cargo
                .iter()
                .map(|s| quote(s))
                .collect::<Result<Vec<_>>>()?
                .join(" "),
        );
        let mut command = vec!["/usr/bin/env".into()];
        command.extend(profile.environment()?);
        // These are compiler settings in this typed job, not ambient runtime switches.
        command.extend([
            "CARGO_PROFILE_DEV_DEBUG=0".into(),
            "CARGO_PROFILE_TEST_DEBUG=0".into(),
            "CARGO_INCREMENTAL=0".into(),
            "RUST_MIN_STACK=16777216".into(),
            format!("TOKIO_WORKER_THREADS={jobs}"),
            format!("UV_THREADPOOL_SIZE={jobs}"),
            "bash".into(),
            "-c".into(),
            script,
        ]);
        Ok(command)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    name: String,
    size: u64,
    chunks: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Admission {
    contract: String,
    owner_epoch: i64,
    computer_id: String,
    endpoint_ref: String,
    endpoint_fingerprint: String,
    compiler_fingerprint: String,
    profile: RustBuildProfile,
    source_head: String,
    source_content: String,
    source_id: String,
    task_id: String,
    local_directory: PathBuf,
    incoming_directory: String,
    preparation_script: String,
    run_directory: String,
    source_directory: String,
    target_directory: String,
    launch_script: String,
    uploads: Vec<Upload>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Progress {
    phase: String,
    upload_index: usize,
    upload_offset: u64,
    preparation_offset: u64,
    run_offset: u64,
    last_receipt: Option<RemoteRunStatus>,
    last_error: Option<String>,
}
impl Default for Progress {
    fn default() -> Self {
        Self {
            phase: "uploading".into(),
            upload_index: 0,
            upload_offset: 0,
            preparation_offset: 0,
            run_offset: 0,
            last_receipt: None,
            last_error: None,
        }
    }
}
#[derive(Serialize)]
struct Outcome {
    job: BuildJob,
    remote_status: Option<RemoteRunStatus>,
}

fn current_admission(job: &BuildJob, actor: &Actor) -> Result<Admission> {
    ensure!(job.owner == actor.owner, "build job owner mismatch");
    let admission: Admission = serde_json::from_value(job.admission.clone())?;
    ensure!(
        admission.contract == "ctox.native-build-job.v1" && admission.owner_epoch == actor.epoch,
        "build owner authority changed since admission"
    );
    Ok(admission)
}

fn public_probe(remote: &BuildRemote, profile: &RustBuildProfile, base: &PublicBase) -> Result<()> {
    let parts = base.repository.split('/').collect::<Vec<_>>();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|p| !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)))
            && base.revision.len() == 40
            && base.revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid anonymous GitHub base"
    );
    let script = r#"import json,subprocess,sys
r=json.load(sys.stdin)
env={"PATH":r["path"],"GIT_CONFIG_NOSYSTEM":"1","GIT_CONFIG_GLOBAL":"/dev/null","GIT_CONFIG_SYSTEM":"/dev/null","GIT_TERMINAL_PROMPT":"0","GIT_ASKPASS":"/bin/false"}
out=subprocess.check_output(["git","-c","credential.helper=","-c","http.extraHeader=","ls-remote","--exit-code","https://github.com/"+r["repo"]+".git","HEAD","refs/heads/*"],env=env,stderr=subprocess.DEVNULL,timeout=5)
if not any(line.split()[0].decode()==r["revision"] for line in out.splitlines()): raise RuntimeError("base is not anonymously advertised")
"#;
    let output=remote.binding().execute_generated(&format!("python3 -c {}",quote(script)?),
        &serde_json::to_vec(&json!({"repo":base.repository,"revision":base.revision,"path":profile.bin_dirs.join(":")}))?)?;
    ensure!(
        output.exit_code == 0,
        "public source base is not anonymously reachable"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn submit(
    root: &Path,
    actor: &Actor,
    source_root: &Path,
    staging_root: &Path,
    toolchain: &str,
    computer_id: Option<&str>,
    task_id: &str,
    recipe: &CargoRecipe,
    timeout_seconds: u32,
    public: Option<PublicBase>,
) -> Result<BuildJob> {
    identifier(&actor.owner)?;
    identifier(task_id)?;
    ensure!(
        (1..=86400).contains(&timeout_seconds),
        "invalid build deadline"
    );
    let job_id = uuid::Uuid::new_v4().to_string();
    let computers =
        computer_capabilities::load_registered_computer_capabilities(root, &actor.owner)?;
    ensure!(
        computers.len() <= 16,
        "native build selection supports at most sixteen computers per invocation"
    );
    let mut observations = Vec::new();
    let mut bindings = std::collections::BTreeMap::new();
    for computer in &computers {
        if computer.agentless || computer_id.is_some_and(|id| id != computer.computer_id) {
            continue;
        }
        let Some(ComputerCapability::Build(grant))=computer.capabilities.iter()
            .find(|cap|matches!(cap,ComputerCapability::Build(b) if b.toolchains.iter().any(|s|s==toolchain))) else { continue; };
        let Some(profile) =
            build_profile::load(root, &actor.owner, &computer.computer_id, toolchain)?
        else {
            continue;
        };
        profile.validate()?;
        let request = ComputerEndpointRequest {
            owner_user_id: actor.owner.clone(),
            computer_id: computer.computer_id.clone(),
            endpoint_ref: grant.ssh_endpoint_ref.clone(),
            usage: EndpointUse::Build,
        };
        // One connection attempt per candidate. Failed observations are not retried.
        if let Ok(remote) = BuildRemote::bind(root, request, &job_id) {
            if let Ok(observation) = remote.binding().availability() {
                observations.push(observation);
                bindings.insert(computer.computer_id.clone(), (remote, profile));
            }
        }
    }
    let target = computer_capabilities::select_build_target(
        &computers,
        &observations,
        toolchain,
        i64::try_from(super::store::now_ms())?,
    )?
    .context("no available registered build computer with this compiler profile")?;
    let (remote, profile) = bindings
        .remove(&target.computer_id)
        .context("selected build binding disappeared")?;
    let compiler_fingerprint = profile.fingerprint(remote.binding())?;
    if let Some(base) = &public {
        public_probe(&remote, &profile, base)?;
    }
    let captured = build_source::capture(
        source_root,
        staging_root,
        public.map(|b| PublicGithubBase {
            repository: b.repository,
            revision: b.revision,
        }),
    )?;
    let delivery = build_delivery::package(
        &target.config,
        &captured,
        staging_root,
        &job_id,
        &compiler_fingerprint,
    )?;
    let plan = build_lane_runner::plan(
        &target.config,
        &actor.owner,
        task_id,
        &job_id,
        &delivery.source_id,
        &recipe.command(&profile, target.config.jobs)?,
        timeout_seconds,
    )?;
    let local = tempfile::Builder::new()
        .prefix("ctox-build-job-")
        .tempdir_in(staging_root)?;
    private_directory(local.path())?;
    let mut uploads = Vec::new();
    for source in &delivery.uploads {
        let destination = local.path().join(source.remote_name);
        fs::copy(&source.local, &destination)?;
        let mut file = fs::File::open(&destination)?;
        let size = file.metadata()?.len();
        let mut chunks = Vec::new();
        let mut buffer = vec![0u8; UPLOAD_CHUNK_BYTES];
        loop {
            let count = read_chunk(&mut file, &mut buffer)?;
            if count == 0 {
                break;
            }
            chunks.push(format!("{:x}", Sha256::digest(&buffer[..count])));
        }
        file.sync_all()?;
        private_file(&destination, false)?;
        uploads.push(Upload {
            name: source.remote_name.into(),
            size,
            chunks,
        });
    }
    fs::File::open(local.path())?.sync_all()?;
    let admission = Admission {
        contract: "ctox.native-build-job.v1".into(),
        owner_epoch: actor.epoch,
        computer_id: target.computer_id,
        endpoint_ref: target.config.ssh_endpoint_ref,
        endpoint_fingerprint: remote.binding().fingerprint().into(),
        compiler_fingerprint,
        profile,
        source_head: captured.head_revision.clone(),
        source_content: captured.source_id.clone(),
        source_id: delivery.source_id.clone(),
        task_id: task_id.into(),
        local_directory: local.path().to_owned(),
        incoming_directory: delivery.incoming_dir.clone(),
        preparation_script: delivery.prepare_script.clone(),
        run_directory: plan.run_dir,
        source_directory: plan.source_dir,
        target_directory: plan.target_dir,
        launch_script: plan.script,
        uploads,
    };
    let store = BuildJobStore::open(root)?;
    let job = store.create(&actor.owner, &job_id, &serde_json::to_value(admission)?)?;
    // The SQL admission is durable before relinquishing automatic local cleanup.
    // Retention is owned by the job, never the live checkout.
    let _path = local.keep();
    Ok(job)
}

fn read_chunk(input: &mut impl Read, buffer: &mut [u8]) -> Result<usize> {
    let mut count = 0;
    while count < buffer.len() {
        let n = input.read(&mut buffer[count..])?;
        if n == 0 {
            break;
        }
        count += n;
    }
    Ok(count)
}
fn private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn private_file(path: &Path, writable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if writable { 0o600 } else { 0o400 }),
        )?;
    }
    Ok(())
}

fn local_root(admission: &Admission) -> Result<PathBuf> {
    let path = admission.local_directory.canonicalize()?;
    ensure!(
        path == admission.local_directory && path.is_dir(),
        "build source staging disappeared or changed"
    );
    Ok(path)
}
fn frozen_chunk(admission: &Admission, progress: &Progress) -> Result<Vec<u8>> {
    let upload = admission
        .uploads
        .get(progress.upload_index)
        .context("invalid build upload index")?;
    ensure!(
        matches!(
            upload.name.as_str(),
            "source.tar" | "manifest.json" | "commits.bundle"
        ) && progress.upload_offset < upload.size
            && progress.upload_offset % (UPLOAD_CHUNK_BYTES as u64) == 0,
        "invalid frozen source cursor"
    );
    let path = local_root(admission)?.join(&upload.name);
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == upload.size,
        "frozen source file changed"
    );
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(progress.upload_offset))?;
    let count =
        usize::try_from((upload.size - progress.upload_offset).min(UPLOAD_CHUNK_BYTES as u64))?;
    let mut bytes = vec![0u8; count];
    file.read_exact(&mut bytes)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    ensure!(
        upload.chunks.get(usize::try_from(
            progress.upload_offset / (UPLOAD_CHUNK_BYTES as u64)
        )?) == Some(&digest),
        "frozen source chunk changed since admission"
    );
    Ok(bytes)
}

fn save_log(
    admission: &Admission,
    name: &str,
    offset: u64,
    status: &RemoteRunStatus,
) -> Result<usize> {
    let bytes = STANDARD.decode(&status.log_base64)?;
    ensure!(
        bytes.len() <= 64 * 1024 && status.log_offset == offset + u64::try_from(bytes.len())?,
        "remote log cursor does not match binary bytes"
    );
    if bytes.is_empty() {
        return Ok(0);
    }
    let path = local_root(admission)?.join(name);
    if path.exists() {
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "unsafe native log spool"
        );
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&path)?;
    private_file(&path, true)?;
    let size = file.metadata()?.len();
    if size == offset {
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    } else {
        ensure!(
            size == status.log_offset,
            "native log spool cursor conflict"
        );
        file.seek(SeekFrom::Start(offset))?;
        let mut previous = vec![0u8; bytes.len()];
        file.read_exact(&mut previous)?;
        ensure!(previous == bytes, "native log replay differs");
    }
    Ok(bytes.len())
}

fn step(root: &Path, actor: &Actor, job_id: &str) -> Result<Outcome> {
    identifier(job_id)?;
    let mut store = BuildJobStore::open(root)?;
    let original = store
        .read(&actor.owner, job_id)?
        .context("owner build job not found")?;
    let admission = current_admission(&original, actor)?;
    if matches!(
        original.state.as_str(),
        "succeeded" | "failed" | "cancelled"
    ) {
        return Ok(Outcome {
            job: original,
            remote_status: None,
        });
    }
    let (claimed, lease) = store.claim(&actor.owner, job_id, original.revision, MAX_LEASE_MS)?;
    let mut revision = claimed.revision;
    let mut progress: Progress = serde_json::from_value(claimed.progress.clone())?;
    let remote = BuildRemote::resume(
        root,
        ComputerEndpointRequest {
            owner_user_id: actor.owner.clone(),
            computer_id: admission.computer_id.clone(),
            endpoint_ref: admission.endpoint_ref.clone(),
            usage: EndpointUse::Build,
        },
        job_id,
        &admission.endpoint_fingerprint,
    );
    let result = (|| -> Result<(Option<RemoteRunStatus>, Option<&'static str>)> {
        let remote = remote?;
        progress.last_error = None;
        match progress.phase.as_str() {
            "uploading" => {
                for _ in 0..4 {
                    if progress.upload_index == admission.uploads.len() {
                        progress.phase = "preparing".into();
                        break;
                    }
                    let upload = &admission.uploads[progress.upload_index];
                    if progress.upload_offset == upload.size {
                        progress.upload_index += 1;
                        progress.upload_offset = 0;
                        continue;
                    }
                    let bytes = frozen_chunk(&admission, &progress)?;
                    remote.upload_chunk(&upload.name, progress.upload_offset, &bytes)?;
                    progress.upload_offset += u64::try_from(bytes.len())?;
                    // Persist every acknowledgement before sending the next chunk.
                    revision = store
                        .checkpoint(
                            &lease,
                            revision,
                            &serde_json::to_value(&progress)?,
                            MAX_LEASE_MS,
                        )?
                        .revision;
                }
                Ok((None, None))
            }
            "preparing" => {
                remote.start_preparation_script(
                    &admission.incoming_directory,
                    &admission.preparation_script,
                )?;
                let status = remote.preparation_status(progress.preparation_offset)?;
                let count = save_log(
                    &admission,
                    "preparation.log",
                    progress.preparation_offset,
                    &status,
                )?;
                progress.preparation_offset = status.log_offset;
                if let Some(exit) = status.exit_code {
                    if count < 64 * 1024 {
                        if exit != 0 {
                            progress.phase = "done".into();
                            return Ok((Some(status), Some("failed")));
                        }
                        ensure!(
                            admission.profile.fingerprint(remote.binding())?
                                == admission.compiler_fingerprint,
                            "compiler assets changed since job admission"
                        );
                        let plan = build_lane_runner::BuildLanePlan {
                            run_dir: admission.run_directory.clone(),
                            source_dir: admission.source_directory.clone(),
                            target_dir: admission.target_directory.clone(),
                            script: admission.launch_script.clone(),
                        };
                        remote.launch(&plan, &admission.task_id)?;
                        progress.phase = "running".into();
                    }
                }
                Ok((Some(status), None))
            }
            "running" => {
                let status = remote.build_status(&admission.task_id, progress.run_offset)?;
                ensure!(status.exists, "admitted native build run disappeared");
                let count = save_log(&admission, "build.log", progress.run_offset, &status)?;
                progress.run_offset = status.log_offset;
                if let Some(exit) = status.exit_code {
                    if count < 64 * 1024 {
                        progress.phase = "done".into();
                        return Ok((
                            Some(status),
                            Some(if exit == 0 { "succeeded" } else { "failed" }),
                        ));
                    }
                }
                Ok((Some(status), None))
            }
            _ => anyhow::bail!("invalid persisted build phase"),
        }
    })();
    match result {
        Ok((status, terminal)) => {
            if let Some(value) = &status {
                let mut receipt = value.clone();
                receipt.log_base64.clear();
                progress.last_receipt = Some(receipt);
            }
            let value = serde_json::to_value(&progress)?;
            let job = if let Some(state) = terminal {
                store.complete(&lease, revision, &value, state)?
            } else {
                store.release(&lease, revision, &value)?
            };
            Ok(Outcome {
                job,
                remote_status: status,
            })
        }
        Err(error) => {
            // Release after a failed/ambiguous operation. The next step reconciles
            // the same bytes/script/run, rather than creating a new remote job.
            progress.last_error = Some(error.to_string().chars().take(1024).collect());
            store.release(&lease, revision, &serde_json::to_value(progress)?)?;
            Err(error)
        }
    }
}

pub(crate) fn handle_cli(root: &Path, args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 2 && args[0] == "--input",
        "usage: ctox build-job --input <private-json-file>"
    );
    let path = Path::new(&args[1]);
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 1024 * 1024,
        "native build input must be a regular file of at most 1 MiB"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "native build input must be private"
        );
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut bytes = Vec::new();
    options
        .open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "native build input grew beyond limit"
    );
    let input: Input = serde_json::from_slice(&bytes).context("invalid native build input")?;
    let actor = authenticate(root, &input.capability_token)?;
    let output: Value = match input.request {
        Request::Profile {
            computer_id,
            profile,
        } => {
            build_profile::save(root, &actor.owner, &computer_id, &profile)?;
            json!({"computer_id":computer_id,"profile":profile.name})
        }
        Request::Submit {
            source_root,
            staging_root,
            toolchain,
            computer_id,
            task_id,
            recipe,
            timeout_seconds,
            public_base,
        } => serde_json::to_value(submit(
            root,
            &actor,
            &source_root,
            &staging_root,
            &toolchain,
            computer_id.as_deref(),
            &task_id,
            &recipe,
            timeout_seconds,
            public_base,
        )?)?,
        Request::Step { job_id } => serde_json::to_value(step(root, &actor, &job_id)?)?,
        Request::Status { job_id } => {
            let job = BuildJobStore::open(root)?
                .read(&actor.owner, &job_id)?
                .context("owner build job not found")?;
            current_admission(&job, &actor)?;
            serde_json::to_value(job)?
        }
        Request::List { after_job_id } => serde_json::to_value(BuildJobStore::open(root)?.list(
            &actor.owner,
            after_job_id.as_deref().unwrap_or(""),
            100,
        )?)?,
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn admission(directory: &Path, bytes: &[u8]) -> Admission {
        let program = "/usr/bin/fixture".to_owned();
        Admission {
            contract: "ctox.native-build-job.v1".into(),
            owner_epoch: 4,
            computer_id: "computer".into(),
            endpoint_ref: "endpoint".into(),
            endpoint_fingerprint: "binding".into(),
            compiler_fingerprint: "a".repeat(64),
            profile: RustBuildProfile {
                name: "rust".into(),
                home: "/home/fixture".into(),
                bin_dirs: vec!["/usr/bin".into()],
                rustc: program.clone(),
                cargo: program.clone(),
                cc: program.clone(),
                cxx: program.clone(),
                protoc: program.clone(),
                node: program,
                libclang_dir: "/lib".into(),
                library_dirs: Vec::new(),
                protoc_include: None,
                ctox_prep: None,
            },
            source_head: "b".repeat(40),
            source_content: "c".repeat(64),
            source_id: "d".repeat(64),
            task_id: "task".into(),
            local_directory: directory.canonicalize().unwrap(),
            incoming_directory: "/lane/incoming/job".into(),
            preparation_script: "script".into(),
            run_directory: "/lane/runs/task/job".into(),
            source_directory: "/lane/sources/source".into(),
            target_directory: "/lane/target/source".into(),
            launch_script: "script".into(),
            uploads: vec![Upload {
                name: "source.tar".into(),
                size: bytes.len() as u64,
                chunks: bytes
                    .chunks(UPLOAD_CHUNK_BYTES)
                    .map(|c| format!("{:x}", Sha256::digest(c)))
                    .collect(),
            }],
        }
    }
    #[test]
    fn real_native_store_reopen_retains_each_ack_and_rejects_changed_source_or_owner_epoch(
    ) -> Result<()> {
        let root = tempfile::tempdir()?;
        let spool = tempfile::tempdir()?;
        let bytes = vec![42u8; UPLOAD_CHUNK_BYTES + 17];
        fs::write(spool.path().join("source.tar"), &bytes)?;
        let admission = admission(spool.path(), &bytes);
        let actor = Actor {
            owner: "owner-1".into(),
            epoch: 4,
        };
        let mut store = BuildJobStore::open(root.path())?;
        let job = store.create(&actor.owner, "job", &serde_json::to_value(&admission)?)?;
        let (_, lease) = store.claim(&actor.owner, "job", job.revision, MAX_LEASE_MS)?;
        let mut progress = Progress::default();
        assert_eq!(
            frozen_chunk(&admission, &progress)?,
            bytes[..UPLOAD_CHUNK_BYTES]
        );
        progress.upload_offset = UPLOAD_CHUNK_BYTES as u64;
        let saved = store.checkpoint(&lease, 1, &serde_json::to_value(&progress)?, MAX_LEASE_MS)?;
        store.release(&lease, saved.revision, &saved.progress)?;
        drop(store);
        let reopened = BuildJobStore::open(root.path())?;
        let saved = reopened.read(&actor.owner, "job")?.unwrap();
        let restored = current_admission(&saved, &actor)?;
        let progress: Progress = serde_json::from_value(saved.progress.clone())?;
        assert_eq!(
            frozen_chunk(&restored, &progress)?,
            bytes[UPLOAD_CHUNK_BYTES..]
        );
        assert!(current_admission(
            &saved,
            &Actor {
                owner: actor.owner.clone(),
                epoch: 5
            }
        )
        .is_err());
        assert!(current_admission(
            &saved,
            &Actor {
                owner: "other-owner".into(),
                epoch: 4
            }
        )
        .is_err());
        let mut changed = bytes;
        changed[UPLOAD_CHUNK_BYTES] = 1;
        fs::write(spool.path().join("source.tar"), changed)?;
        assert!(frozen_chunk(&restored, &progress).is_err());
        Ok(())
    }
    #[test]
    fn binary_log_replay_fsyncs_once_and_rejects_different_bytes() -> Result<()> {
        let spool = tempfile::tempdir()?;
        let admission = admission(spool.path(), b"source");
        let bytes = b"\0binary\xfflog";
        let status = RemoteRunStatus {
            exists: true,
            log_offset: bytes.len() as u64,
            log_base64: STANDARD.encode(bytes),
            ..Default::default()
        };
        assert_eq!(save_log(&admission, "build.log", 0, &status)?, bytes.len());
        assert_eq!(save_log(&admission, "build.log", 0, &status)?, bytes.len());
        assert_eq!(fs::read(spool.path().join("build.log"))?, bytes);
        let changed = RemoteRunStatus {
            log_base64: STANDARD.encode(b"\0binary\xfelog"),
            ..status
        };
        assert!(save_log(&admission, "build.log", 0, &changed).is_err());
        Ok(())
    }
}
