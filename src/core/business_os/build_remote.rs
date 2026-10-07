// Origin: CTOX
// License: AGPL-3.0-only

//! Short, resumable source operations for a registered native build endpoint.
//! The lane account owns its directories exclusively; this is not a sandbox.

use super::build_delivery::BuildSourceDelivery;
use super::build_lane_runner::BuildLanePlan;
use super::build_ssh::NativeBuildSsh;
use super::computer_endpoints::ComputerEndpointRequest;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const UPLOAD_CHUNK_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteRunStatus {
    pub exists: bool,
    pub pid: Option<u32>,
    pub exit_code: Option<u32>,
    pub slot: Option<u16>,
    pub started: Option<String>,
    pub finished: Option<String>,
    pub log_offset: u64,
    pub log_base64: String,
}

pub(crate) struct BuildRemote {
    ssh: NativeBuildSsh,
    job_id: String,
}

fn quote(value: &str) -> Result<String> {
    ensure!(!value.contains('\0'), "NUL in generated build argument");
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid build identifier"
    );
    Ok(())
}

fn python(script: &str, args: &[String]) -> Result<String> {
    let mut parts = vec!["python3".to_owned(), "-c".to_owned(), quote(script)?];
    for arg in args {
        parts.push(quote(arg)?);
    }
    Ok(parts.join(" "))
}

impl BuildRemote {
    pub(crate) fn bind(
        root: &Path,
        request: ComputerEndpointRequest,
        job_id: &str,
    ) -> Result<Self> {
        identifier(job_id)?;
        Ok(Self {
            ssh: NativeBuildSsh::bind(root, request)?,
            job_id: job_id.into(),
        })
    }

    pub(crate) fn resume(
        root: &Path,
        request: ComputerEndpointRequest,
        job_id: &str,
        fingerprint: &str,
    ) -> Result<Self> {
        identifier(job_id)?;
        Ok(Self {
            ssh: NativeBuildSsh::resume(root, request, fingerprint)?,
            job_id: job_id.into(),
        })
    }

    pub(crate) fn binding(&self) -> &NativeBuildSsh {
        &self.ssh
    }

    fn args(&self) -> Vec<String> {
        vec![
            self.ssh.grant().lane_root.clone(),
            self.job_id.clone(),
            self.ssh.fingerprint().into(),
        ]
    }

    /// A replay after an ambiguous response verifies already-written bytes.
    /// Every acknowledged chunk is flushed before its progress is checkpointed.
    pub(crate) fn upload_chunk(&self, name: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        ensure!(
            matches!(name, "source.tar" | "manifest.json" | "commits.bundle"),
            "unknown build upload"
        );
        ensure!(
            !bytes.is_empty() && bytes.len() <= UPLOAD_CHUNK_BYTES,
            "invalid build chunk size"
        );
        let mut args = self.args();
        args.extend([name.into(), offset.to_string()]);
        let output = self.ssh.execute_generated(&python(UPLOAD, &args)?, bytes)?;
        ensure!(output.exit_code == 0, "native build source upload rejected");
        Ok(())
    }

    /// Source preparation is detached and bounded on the host. If acknowledgement
    /// is lost, query the same preparation directory; never overwrite/relaunch.
    pub(crate) fn start_preparation(&self, delivery: &BuildSourceDelivery) -> Result<()> {
        self.start_preparation_script(&delivery.incoming_dir, &delivery.prepare_script)
    }

    pub(crate) fn start_preparation_script(
        &self,
        incoming_directory: &str,
        script: &str,
    ) -> Result<()> {
        ensure!(
            incoming_directory
                == format!("{}/incoming/{}", self.ssh.grant().lane_root, self.job_id),
            "source delivery belongs to another lane/job"
        );
        let output = self
            .ssh
            .execute_generated(&python(PREPARE_LAUNCH, &self.args())?, script.as_bytes())?;
        ensure!(
            output.exit_code == 0 || output.exit_code == 73,
            "native source preparation launch rejected"
        );
        Ok(())
    }

    pub(crate) fn preparation_status(&self, log_offset: u64) -> Result<RemoteRunStatus> {
        self.status(
            &format!(
                "{}/preparations/{}",
                self.ssh.grant().lane_root,
                self.job_id
            ),
            log_offset,
        )
    }

    pub(crate) fn launch(&self, plan: &BuildLanePlan, task_id: &str) -> Result<()> {
        identifier(task_id)?;
        ensure!(
            plan.run_dir
                == format!(
                    "{}/runs/{}/{}",
                    self.ssh.grant().lane_root,
                    task_id,
                    self.job_id
                ),
            "build plan belongs to another lane/job"
        );
        let output = self
            .ssh
            .execute_generated("bash -s", plan.script.as_bytes())?;
        ensure!(
            output.exit_code == 0 || output.exit_code == 73,
            "native build launch rejected"
        );
        Ok(())
    }

    pub(crate) fn build_status(&self, task_id: &str, log_offset: u64) -> Result<RemoteRunStatus> {
        identifier(task_id)?;
        self.status(
            &format!(
                "{}/runs/{}/{}",
                self.ssh.grant().lane_root,
                task_id,
                self.job_id
            ),
            log_offset,
        )
    }

    fn status(&self, directory: &str, log_offset: u64) -> Result<RemoteRunStatus> {
        let args = vec![
            self.ssh.grant().lane_root.clone(),
            directory.into(),
            log_offset.to_string(),
        ];
        let output = self.ssh.execute_generated(&python(STATUS, &args)?, &[])?;
        ensure!(output.exit_code == 0, "native build status rejected");
        let status: RemoteRunStatus = serde_json::from_slice(&output.stdout)?;
        ensure!(
            status.exit_code.is_none_or(|exit| exit <= 255)
                && status
                    .slot
                    .is_none_or(|slot| (1..=self.ssh.grant().slots).contains(&slot))
                && status.pid.is_none_or(|pid| pid > 0)
                && status.log_offset >= log_offset
                && status.log_offset - log_offset <= 64 * 1024,
            "invalid native build receipt"
        );
        Ok(status)
    }
}

const UPLOAD: &str = r##"import json,os,pathlib,sys
def require(value,message):
    if not value: raise RuntimeError(message)
root=pathlib.Path(sys.argv[1])
job=sys.argv[2]
fingerprint=sys.argv[3]
name=sys.argv[4]
offset=int(sys.argv[5])
require(root.resolve(strict=True)==root and root.is_dir(), "unsafe lane")
require(name in ("source.tar","manifest.json","commits.bundle") and offset>=0, "invalid chunk")
incoming=root/"incoming"
incoming.mkdir(mode=0o700,exist_ok=True)
require(incoming.resolve(strict=True)==incoming and not incoming.is_symlink(), "unsafe incoming root")
directory=incoming/job
directory.mkdir(mode=0o700,exist_ok=True)
require(directory.resolve(strict=True)==directory and not directory.is_symlink(), "unsafe incoming job")
marker=directory/".ctox-build-binding"
expected=json.dumps({"job":job,"fingerprint":fingerprint},sort_keys=True).encode()
try:
    fd=os.open(marker,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
except FileExistsError:
    require(not marker.is_symlink() and marker.read_bytes()==expected, "another job binding")
else:
    with os.fdopen(fd,"wb") as stream:
        stream.write(expected); stream.flush(); os.fsync(stream.fileno())
    dfd=os.open(directory,os.O_DIRECTORY); os.fsync(dfd); os.close(dfd)
data=sys.stdin.buffer.read(512*1024+1)
require(0<len(data)<=512*1024, "invalid chunk size")
fd=os.open(directory/name,os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600)
with os.fdopen(fd,"r+b") as stream:
    size=os.fstat(stream.fileno()).st_size
    if size==offset:
        stream.seek(offset); stream.write(data); stream.flush(); os.fsync(stream.fileno())
    elif size==offset+len(data):
        stream.seek(offset); require(stream.read(len(data))==data, "replay differs")
    else:
        raise RuntimeError("unexpected upload offset")
dfd=os.open(directory,os.O_DIRECTORY); os.fsync(dfd); os.close(dfd)
"##;

const PREPARE_LAUNCH: &str = r##"import hashlib,json,os,pathlib,subprocess,sys
def require(value,message):
    if not value: raise RuntimeError(message)
root=pathlib.Path(sys.argv[1]); job=sys.argv[2]; fingerprint=sys.argv[3]
require(root.resolve(strict=True)==root and root.is_dir(), "unsafe lane")
incoming=root/"incoming"/job
require(incoming.resolve(strict=True)==incoming and incoming.is_dir(), "unsafe incoming job")
binding=incoming/".ctox-build-binding"
expected=json.dumps({"job":job,"fingerprint":fingerprint},sort_keys=True).encode()
require(not binding.is_symlink() and binding.read_bytes()==expected, "another source binding")
data=sys.stdin.buffer.read(1024*1024+1)
require(len(data)<=1024*1024 and data.startswith(b"#!/bin/bash\n"), "invalid generated preparation")
digest=hashlib.sha256(data).hexdigest().encode()
parent=root/"preparations"; parent.mkdir(mode=0o700,exist_ok=True)
require(parent.resolve(strict=True)==parent and not parent.is_symlink(), "unsafe preparation root")
directory=parent/job
try: directory.mkdir(mode=0o700)
except FileExistsError:
    require(directory.resolve(strict=True)==directory and directory.is_dir(), "unsafe preparation job")
    saved=directory/".ctox-build-binding"; script_hash=directory/"script.sha256"
    require(not saved.is_symlink() and saved.read_bytes()==expected
        and not script_hash.is_symlink() and script_hash.read_bytes()==digest, "another preparation binding")
    sys.exit(73)
def write(name,data):
    fd=os.open(directory/name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    with os.fdopen(fd,"wb") as stream:
        stream.write(data); stream.flush(); os.fsync(stream.fileno())
write(".ctox-build-binding",expected)
write("script.sha256",digest)
write("prepare.sh",data)
runner=b'''import datetime,os,pathlib,signal,subprocess,sys
root=pathlib.Path(sys.argv[1])
def stamp(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def write(name,value):
    path=root/(name+".tmp")
    with path.open("w") as stream:
        stream.write(str(value)+"\\n"); stream.flush(); os.fsync(stream.fileno())
    os.replace(path,root/name)
    fd=os.open(root,os.O_DIRECTORY); os.fsync(fd); os.close(fd)
write("started",stamp())
child=subprocess.Popen(["bash",str(root/"prepare.sh")],start_new_session=True)
try: code=child.wait(timeout=920)
except subprocess.TimeoutExpired:
    os.killpg(child.pid,signal.SIGTERM)
    try: child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid,signal.SIGKILL); child.wait()
    code=124
write("finished",stamp())
write("exit",code if code>=0 else min(255,128-code))
'''
write("runner.py",runner)
with (directory/"log").open("xb") as log:
    child=subprocess.Popen(["python3",str(directory/"runner.py"),str(directory)],
        stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
write("pid",str(child.pid).encode()+b"\n")
fd=os.open(directory,os.O_DIRECTORY); os.fsync(fd); os.close(fd)
"##;

const STATUS: &str = r##"import base64,json,pathlib,sys
def require(value,message):
    if not value: raise RuntimeError(message)
root=pathlib.Path(sys.argv[1]); directory=pathlib.Path(sys.argv[2]); offset=int(sys.argv[3])
require(root.resolve(strict=True)==root and root.is_dir(), "unsafe lane")
require(directory.is_relative_to(root) and offset>=0, "unsafe status request")
result={"exists":directory.exists(),"pid":None,"exit_code":None,"slot":None,
        "started":None,"finished":None,"log_offset":offset,"log_base64":""}
if result["exists"]:
    require(directory.resolve(strict=True)==directory and directory.is_dir(), "unsafe run")
    for name,key in (("pid","pid"),("exit","exit_code"),("slot","slot"),("started","started"),("finished","finished")):
        path=directory/name
        if path.exists():
            require(not path.is_symlink() and path.is_file() and path.stat().st_size<=256,"unsafe receipt")
            value=path.read_text().strip()
            result[key]=int(value) if key in ("pid","exit_code","slot") else value
    path=directory/"log"
    if path.exists():
        require(not path.is_symlink() and path.is_file() and path.stat().st_size>=offset,"unsafe log")
        with path.open("rb") as stream:
            stream.seek(offset); data=stream.read(64*1024)
        result["log_offset"]+=len(data)
        result["log_base64"]=base64.b64encode(data).decode("ascii")
print(json.dumps(result))
"##;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        process::{Command, Stdio},
    };

    fn upload(root: &Path, name: &str, offset: u64, data: &[u8]) -> bool {
        let mut child = Command::new("python3")
            .args(["-c", UPLOAD])
            .arg(root)
            .args(["job-1", "fingerprint-1", name, &offset.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(data).unwrap();
        child.wait().unwrap().success()
    }

    #[test]
    fn ambiguous_upload_replays_bytes_without_appending_or_replacing() {
        let root = tempfile::tempdir().unwrap();
        assert!(upload(root.path(), "source.tar", 0, b"\0'$(literal)\xff"));
        assert!(upload(root.path(), "source.tar", 0, b"\0'$(literal)\xff"));
        assert!(!upload(root.path(), "source.tar", 0, b"\0'$(changed)\xff"));
        let path = root.path().join("incoming/job-1/source.tar");
        let saved = fs::read(&path).unwrap();
        assert!(upload(
            root.path(),
            "source.tar",
            saved.len() as u64,
            b"tail"
        ));
        assert_eq!(fs::read(path).unwrap(), [saved, b"tail".to_vec()].concat());
        assert!(!upload(root.path(), "../escape", 0, b"no"));
    }

    #[test]
    fn upload_rejects_symlink_and_another_job_binding() {
        let root = tempfile::tempdir().unwrap();
        assert!(upload(root.path(), "manifest.json", 0, b"{}"));
        let directory = root.path().join("incoming/job-1");
        let victim = root.path().join("victim");
        fs::write(&victim, b"unchanged").unwrap();
        std::os::unix::fs::symlink(&victim, directory.join("source.tar")).unwrap();
        assert!(!upload(root.path(), "source.tar", 0, b"replace"));
        assert_eq!(fs::read(&victim).unwrap(), b"unchanged");
        fs::write(directory.join(".ctox-build-binding"), b"another binding").unwrap();
        assert!(!upload(root.path(), "manifest.json", 2, b"bad"));
    }

    #[test]
    fn detached_preparation_is_reconciled_without_a_second_launcher() {
        let root = tempfile::tempdir().unwrap();
        assert!(upload(root.path(), "manifest.json", 0, b"{}"));
        let script = b"#!/bin/bash\nsleep 0.1\nprintf 'prepared-once\\n'\n";
        let launch = |input: &[u8]| {
            let mut child = Command::new("python3")
                .args(["-c", PREPARE_LAUNCH])
                .arg(root.path())
                .args(["job-1", "fingerprint-1"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            child.wait().unwrap().code().unwrap()
        };
        assert_eq!(launch(script), 0);
        assert_eq!(launch(script), 73);
        assert_eq!(launch(b"#!/bin/bash\nprintf changed\n"), 1);
        let directory = root.path().join("preparations/job-1");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !directory.join("exit").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "preparation did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(fs::read_to_string(directory.join("exit")).unwrap(), "0\n");
        assert_eq!(
            fs::read_to_string(directory.join("log")).unwrap(),
            "prepared-once\n"
        );
        assert!(directory.join("pid").is_file());
        assert!(directory.join("finished").is_file());
    }

    #[test]
    fn binary_log_cursor_is_bounded_and_reopens_without_loss() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("runs/task/job");
        fs::create_dir_all(&directory).unwrap();
        let log = vec![0xff; 70 * 1024];
        fs::write(directory.join("log"), &log).unwrap();
        fs::write(directory.join("exit"), b"17\n").unwrap();
        let read = |offset: u64| {
            let output = Command::new("python3")
                .args(["-c", STATUS])
                .arg(root.path())
                .arg(&directory)
                .arg(offset.to_string())
                .output()
                .unwrap();
            assert!(output.status.success());
            serde_json::from_slice::<RemoteRunStatus>(&output.stdout).unwrap()
        };
        let first = read(0);
        assert_eq!(first.log_offset, 64 * 1024);
        assert_eq!(first.exit_code, Some(17));
        let second = read(first.log_offset);
        assert_eq!(second.log_offset, log.len() as u64);
        use base64::Engine;
        let bytes = [
            base64::engine::general_purpose::STANDARD
                .decode(first.log_base64)
                .unwrap(),
            base64::engine::general_purpose::STANDARD
                .decode(second.log_base64)
                .unwrap(),
        ]
        .concat();
        assert_eq!(bytes, log);
    }
}
