// ref: internal/store/gitstore_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: includes typed network authority, lease, recovery, and maintenance coverage
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use git2::build::CheckoutBuilder;
use git2::{IndexAddOption, Oid, Repository, Signature};
use serde_json::json;
use tempfile::TempDir;

use crate::sdk::cliproxy::auth::{Auth, AuthStore, AuthStoreError};

use super::gitstore::{
    GitCredentialRef, GitPushRequest, GitRemoteRequest, GitStoreConfig, GitTokenStore,
    GitTransportAuthority, GitTransportError,
};

fn commit_all(repo: &Repository, message: &str) -> Oid {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Store Test", "store@example.test").unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    match parent.as_ref() {
        Some(parent) => repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[parent],
            )
            .unwrap(),
        None => repo
            .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
            .unwrap(),
    }
}

fn push_branch(repo: &Repository, branch: &str) {
    repo.find_remote("origin")
        .unwrap()
        .push(&[format!("refs/heads/{branch}:refs/heads/{branch}")], None)
        .unwrap();
}

fn checkout_branch(repo: &Repository, branch: &str, start: Option<Oid>) {
    let reference = format!("refs/heads/{branch}");
    let has_commit = if let Some(start) = start {
        repo.reference(&reference, start, true, "test branch")
            .unwrap();
        true
    } else {
        false
    };
    repo.set_head(&reference).unwrap();
    if has_commit {
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout)).unwrap();
    }
}

fn setup_remote(root: &Path) -> (PathBuf, PathBuf) {
    let remote_path = root.join("remote.git");
    let remote = Repository::init_bare(&remote_path).unwrap();
    let seed_path = root.join("seed");
    let seed = Repository::init(&seed_path).unwrap();
    seed.remote("origin", remote_path.to_str().unwrap())
        .unwrap();

    checkout_branch(&seed, "trunk", None);
    fs::write(seed_path.join("branch.txt"), b"remote default branch\n").unwrap();
    fs::create_dir_all(seed_path.join("auths")).unwrap();
    fs::create_dir_all(seed_path.join("config")).unwrap();
    fs::write(seed_path.join("auths/.gitkeep"), b"").unwrap();
    fs::write(seed_path.join("config/.gitkeep"), b"").unwrap();
    let trunk = commit_all(&seed, "trunk");
    push_branch(&seed, "trunk");

    checkout_branch(&seed, "release/2026", Some(trunk));
    fs::write(seed_path.join("branch.txt"), b"release branch\n").unwrap();
    commit_all(&seed, "release");
    push_branch(&seed, "release/2026");
    remote.set_head("refs/heads/trunk").unwrap();
    (remote_path, seed_path)
}

fn store(remote: &Path, workspace: &Path, branch: Option<&str>) -> GitTokenStore {
    GitTokenStore::new(GitStoreConfig {
        remote: remote.to_string_lossy().into_owned(),
        branch: branch.map(str::to_owned),
        auth_dir: workspace.join("auths"),
    })
    .unwrap()
}

fn branch_name(repo_path: &Path) -> String {
    Repository::open(repo_path)
        .unwrap()
        .head()
        .unwrap()
        .shorthand()
        .unwrap()
        .to_owned()
}

fn advance(seed_path: &Path, branch: &str, content: &[u8]) {
    let seed = Repository::open(seed_path).unwrap();
    let oid = seed.refname_to_id(&format!("refs/heads/{branch}")).unwrap();
    checkout_branch(&seed, branch, Some(oid));
    fs::write(seed_path.join("branch.txt"), content).unwrap();
    commit_all(&seed, "advance");
    push_branch(&seed, branch);
}

#[derive(Default)]
struct TransportEvents {
    cloned: usize,
    fetched: usize,
    pushed: Vec<GitPushRequest>,
    maintained: usize,
}

struct RecordingNetworkAuthority {
    backing: PathBuf,
    events: Mutex<TransportEvents>,
}

impl RecordingNetworkAuthority {
    fn new(backing: PathBuf) -> Self {
        Self {
            backing,
            events: Mutex::new(TransportEvents::default()),
        }
    }
}

impl GitTransportAuthority for RecordingNetworkAuthority {
    fn clone_repository(&self, request: &GitRemoteRequest) -> Result<(), GitTransportError> {
        git2::build::RepoBuilder::new()
            .clone(self.backing.to_str().unwrap(), &request.repository)
            .map_err(|_| GitTransportError::Unavailable)?;
        let repo = Repository::open(&request.repository)
            .map_err(|_| GitTransportError::InvalidResponse)?;
        repo.remote_set_url("origin", &request.remote)
            .map_err(|_| GitTransportError::InvalidResponse)?;
        self.events.lock().unwrap().cloned += 1;
        Ok(())
    }

    fn fetch(&self, _request: &GitRemoteRequest) -> Result<(), GitTransportError> {
        self.events.lock().unwrap().fetched += 1;
        Ok(())
    }

    fn default_branch(
        &self,
        _request: &GitRemoteRequest,
    ) -> Result<Option<String>, GitTransportError> {
        Ok(Some("trunk".to_owned()))
    }

    fn push(&self, request: &GitPushRequest) -> Result<(), GitTransportError> {
        self.events.lock().unwrap().pushed.push(request.clone());
        Ok(())
    }

    fn maintenance(&self, _request: &GitRemoteRequest) -> Result<(), GitTransportError> {
        self.events.lock().unwrap().maintained += 1;
        Ok(())
    }
}

fn remote_has_path(remote_path: &Path, branch: &str, path: &str) -> bool {
    let repo = Repository::open_bare(remote_path).unwrap();
    let oid = repo.refname_to_id(&format!("refs/heads/{branch}")).unwrap();
    let exists = repo
        .find_commit(oid)
        .unwrap()
        .tree()
        .unwrap()
        .get_path(Path::new(path))
        .is_ok();
    exists
}
