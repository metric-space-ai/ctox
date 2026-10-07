// Origin: CTOX
// License: AGPL-3.0-only
//! Native owner-scoped compiler profiles. Build environment is explicit configuration.
use super::build_ssh::NativeBuildSsh;
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RustBuildProfile {
    pub name: String,
    pub home: String,
    pub bin_dirs: Vec<String>,
    pub rustc: String,
    pub cargo: String,
    pub cc: String,
    pub cxx: String,
    pub protoc: String,
    pub node: String,
    pub libclang_dir: String,
    #[serde(default)]
    pub library_dirs: Vec<String>,
    #[serde(default)]
    pub protoc_include: Option<String>,
    pub ctox_prep: Option<String>,
}

fn absolute(value: &str) -> Result<()> {
    ensure!(
        !value.contains('\0')
            && value.len() <= 4096
            && Path::new(value).is_absolute()
            && Path::new(value)
                .components()
                .all(|p| matches!(p, Component::RootDir | Component::Normal(_))),
        "compiler profile requires absolute normalized paths"
    );
    Ok(())
}

impl RustBuildProfile {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 128
                && self
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
            "invalid compiler profile name"
        );
        for path in [
            &self.home,
            &self.rustc,
            &self.cargo,
            &self.cc,
            &self.cxx,
            &self.protoc,
            &self.node,
            &self.libclang_dir,
        ] {
            absolute(path)?;
        }
        ensure!(
            !self.bin_dirs.is_empty() && self.bin_dirs.len() <= 16,
            "profile PATH must contain 1..16 directories"
        );
        for path in &self.bin_dirs {
            absolute(path)?;
            ensure!(!path.contains(':'), "colon in profile PATH");
        }
        ensure!(
            self.library_dirs.len() <= 16,
            "too many compiler library directories"
        );
        for path in &self.library_dirs {
            absolute(path)?;
            ensure!(!path.contains(':'), "colon in compiler library directory");
        }
        if let Some(path) = &self.protoc_include {
            absolute(path)?;
        }
        if let Some(path) = &self.ctox_prep {
            absolute(path)?;
        }
        Ok(())
    }

    pub(crate) fn environment(&self) -> Result<Vec<String>> {
        self.validate()?;
        let mut environment = vec![
            format!("HOME={}", self.home),
            format!("PATH={}", self.bin_dirs.join(":")),
            format!("RUSTC={}", self.rustc),
            format!("CC={}", self.cc),
            format!("CXX={}", self.cxx),
            format!("PROTOC={}", self.protoc),
            format!("LIBCLANG_PATH={}", self.libclang_dir),
            format!("CLANG_PATH={}", self.cc),
        ];
        if !self.library_dirs.is_empty() {
            environment.push(format!("LD_LIBRARY_PATH={}", self.library_dirs.join(":")));
        }
        if let Some(path) = &self.protoc_include {
            environment.push(format!("PROTOC_INCLUDE={path}"));
        }
        Ok(environment)
    }

    /// Hash actual compiler assets, their resolved paths/version output, and
    /// the complete typed profile. A mutable label is never a target identity.
    pub(crate) fn fingerprint(&self, ssh: &NativeBuildSsh) -> Result<String> {
        self.validate()?;
        let command = format!("python3 -c {}", quote(PROBE)?);
        let output = ssh.execute_generated(&command, &serde_json::to_vec(self)?)?;
        ensure!(output.exit_code == 0, "compiler profile probe failed");
        let digest = std::str::from_utf8(&output.stdout)?.trim();
        ensure!(
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid actual compiler fingerprint"
        );
        Ok(digest.into())
    }
}

pub(crate) fn quote(value: &str) -> Result<String> {
    ensure!(!value.contains('\0'), "NUL in generated compiler argument");
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

pub(crate) fn save(
    root: &Path,
    owner: &str,
    computer: &str,
    profile: &RustBuildProfile,
) -> Result<()> {
    profile.validate()?;
    let computers =
        super::computer_capabilities::load_registered_computer_capabilities(root, owner)?;
    ensure!(computers.iter().any(|c| c.computer_id == computer && !c.agentless
        && c.capabilities.iter().any(|cap| matches!(cap,
            super::computer_capabilities::ComputerCapability::Build(b) if b.toolchains.contains(&profile.name)))),
        "compiler profile requires the owner's assigned build grant");
    let conn = connection(root)?;
    conn.execute(
        "INSERT INTO native_build_profiles(owner,computer_id,name,profile_json)
        VALUES(?1,?2,?3,?4) ON CONFLICT(owner,computer_id,name)
        DO UPDATE SET profile_json=excluded.profile_json",
        params![
            owner,
            computer,
            profile.name,
            serde_json::to_string(profile)?
        ],
    )?;
    Ok(())
}

pub(crate) fn load(
    root: &Path,
    owner: &str,
    computer: &str,
    name: &str,
) -> Result<Option<RustBuildProfile>> {
    let conn = connection(root)?;
    let json: Option<String> = conn.query_row(
        "SELECT profile_json FROM native_build_profiles WHERE owner=?1 AND computer_id=?2 AND name=?3",
        params![owner, computer, name], |row| row.get(0)).optional()?;
    json.map(|json| serde_json::from_str(&json).context("invalid native compiler profile"))
        .transpose()
}

fn connection(root: &Path) -> Result<rusqlite::Connection> {
    let conn = super::store::open_store(root)?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS native_build_profiles(
        owner TEXT NOT NULL, computer_id TEXT NOT NULL, name TEXT NOT NULL, profile_json TEXT NOT NULL,
        PRIMARY KEY(owner,computer_id,name))")?;
    Ok(conn)
}

const PROBE: &str = r##"import hashlib,json,os,pathlib,signal,subprocess,sys
def expired(*args): raise TimeoutError("compiler probe deadline")
signal.signal(signal.SIGALRM,expired); signal.alarm(7)
profile=json.load(sys.stdin)
env={"HOME":profile["home"],"PATH":":".join(profile["bin_dirs"])}
env.update({"RUSTC":profile["rustc"],"CC":profile["cc"],"CXX":profile["cxx"],"PROTOC":profile["protoc"],"LIBCLANG_PATH":profile["libclang_dir"]})
if profile["library_dirs"]: env["LD_LIBRARY_PATH"]=":".join(profile["library_dirs"])
if profile["protoc_include"]: env["PROTOC_INCLUDE"]=profile["protoc_include"]
def output(command):
    return subprocess.check_output(command,env=env,stderr=subprocess.STDOUT,timeout=2).decode("utf-8")
programs={k:profile[k] for k in ("rustc","cargo","cc","cxx","protoc","node")}
for key in ("rustc","cargo"):
    if pathlib.Path(programs[key]).resolve(strict=True).name=="rustup": raise RuntimeError("use direct compiler binaries, not context-sensitive rustup shims")
versions={k:output([v,"-vV" if k=="rustc" else "--version"]) for k,v in programs.items()}
sysroot=pathlib.Path(output([profile["rustc"],"--print","sysroot"]).strip()).resolve(strict=True)
assets=set(programs.values())
assets.add(str(sysroot/"bin"/"rustc"))
assets.update(str(p) for pattern in ("librustc_driver*","libLLVM*") for p in (sysroot/"lib").glob(pattern))
clang=pathlib.Path(profile["libclang_dir"]).resolve(strict=True)
assets.update(str(p) for p in clang.glob("libclang.*"))
for name in profile["library_dirs"]:
    directory=pathlib.Path(name).resolve(strict=True)
    assets.update(str(p) for p in directory.glob("*.so*") if p.is_file())
if profile["protoc_include"]:
    directory=pathlib.Path(profile["protoc_include"]).resolve(strict=True)
    assets.update(str(p) for p in directory.rglob("*.proto") if p.is_file())
if profile["ctox_prep"]: assets.add(profile["ctox_prep"])
rows=[]; digests={}
for name in sorted(assets):
    path=pathlib.Path(name).resolve(strict=True)
    if not path.is_file(): raise RuntimeError("compiler asset is not a regular file")
    key=str(path)
    if key not in digests:
        digest=hashlib.sha256()
        with path.open("rb") as stream:
            while True:
                block=stream.read(64*1024)
                if not block: break
                digest.update(block)
        digests[key]=digest.hexdigest()
    rows.append([name,key,digests[key]])
if not any("libclang." in row[0] for row in rows): raise RuntimeError("profile has no libclang")
payload={"profile":profile,"versions":versions,"assets":rows}
print(hashlib.sha256(json.dumps(payload,sort_keys=True,separators=(",",":")).encode()).hexdigest())
"##;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    #[test]
    fn actual_compiler_asset_changes_identity_without_version_change() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::create_dir_all(dir.path().join("bin"))?;
        fs::create_dir_all(dir.path().join("lib"))?;
        fs::write(dir.path().join("lib/libclang.so"), b"first")?;
        let program = dir.path().join("bin/rustc");
        let script = format!("#!/bin/sh\nif [ \"$1\" = --print ]; then printf '%s\\n' '{}'; else echo unchanged-version; fi\n", dir.path().display());
        fs::write(&program, script)?;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700))?;
        let path = program.to_str().unwrap().to_owned();
        let profile = RustBuildProfile {
            name: "fixture".into(),
            home: dir.path().to_str().unwrap().into(),
            bin_dirs: vec!["/usr/bin".into(), "/bin".into()],
            rustc: path.clone(),
            cargo: path.clone(),
            cc: path.clone(),
            cxx: path.clone(),
            protoc: path.clone(),
            node: path,
            libclang_dir: dir.path().join("lib").to_str().unwrap().into(),
            library_dirs: Vec::new(),
            protoc_include: None,
            ctox_prep: None,
        };
        profile.validate()?;
        let probe = || -> Result<Vec<u8>> {
            let mut child = Command::new("python3")
                .args(["-c", PROBE])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(&profile)?)?;
            let output = child.wait_with_output()?;
            ensure!(output.status.success(), "compiler fixture failed");
            Ok(output.stdout)
        };
        let before = probe()?;
        fs::write(dir.path().join("lib/libclang.so"), b"second")?;
        assert_ne!(before, probe()?);
        Ok(())
    }
}
