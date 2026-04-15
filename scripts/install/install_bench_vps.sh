#!/usr/bin/env bash
#
# install_bench_vps.sh — bring a fresh Ubuntu (22.04 / 24.04) VPS to a state
# where the CTOX Harbor / Terminal-Bench-2 adapter can run.
#
# Curated from a real first-run on a clean OVH Ubuntu 24.04 image. Each step
# is here because it bit us during that bootstrap.
#
# What this script does, in order:
#   1. Pause unattended-upgrades so apt-get does not race against it.
#   2. Install apt prereqs: build-essential, pkg-config, libssl-dev,
#      ca-certificates, curl, git, python3-venv, python3-pip.
#   3. Install Docker engine from the official docker.com apt repo
#      (NOT the snap package — that ships a second daemon that fights
#      systemd's docker.socket and breaks `docker ps`).
#   4. Add the current user to the `docker` group.
#   5. Install rustup + the stable toolchain.
#   6. Clone CTOX (or pull) into ~/ctox-bench. `--branch` overridable.
#   7. Drop the legacy contracts/history/creation-ledger.md stub.
#      Without it, looks_like_ctox_root() rejects ~/ctox-bench as a CTOX
#      root and CTOX_ROOT silently falls back to $HOME, which then misses
#      every state file CTOX expects.
#   8. Build ctox + codex-exec (release).
#   9. Create the Harbor venv (Python 3.12+), install harbor-framework
#      from github.com/laude-institute/harbor and the local
#      ctox-harbor adapter from harbor-adapter/.
#  10. Print a concise "what's next" summary so the operator can run a
#      smoke without further surgery.
#
# Usage:
#   bash scripts/install/install_bench_vps.sh
#
# Environment overrides:
#   CTOX_REPO_URL         https://github.com/metric-space-ai/ctox.git
#   CTOX_REPO_BRANCH      main
#   CTOX_INSTALL_DIR      $HOME/ctox-bench
#   HARBOR_REPO_URL       https://github.com/laude-institute/harbor.git
#   HARBOR_VENV_DIR       $HOME/harbor-venv
#   SKIP_BUILD=1          Skip the cargo builds (reuse existing target/)

set -euo pipefail

REPO_URL="${CTOX_REPO_URL:-https://github.com/metric-space-ai/ctox.git}"
REPO_BRANCH="${CTOX_REPO_BRANCH:-main}"
INSTALL_DIR="${CTOX_INSTALL_DIR:-$HOME/ctox-bench}"
HARBOR_REPO_URL="${HARBOR_REPO_URL:-https://github.com/laude-institute/harbor.git}"
HARBOR_VENV_DIR="${HARBOR_VENV_DIR:-$HOME/harbor-venv}"
SKIP_BUILD="${SKIP_BUILD:-0}"

log() { printf '\033[1;34m[bench-vps]\033[0m %s\n' "$*"; }

run_sudo() {
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    "$@"
  fi
}

step_pause_unattended_upgrades() {
  log "Pausing unattended-upgrades (it holds the dpkg lock on first boot)"
  run_sudo systemctl stop unattended-upgrades.service apt-daily.timer apt-daily-upgrade.timer 2>/dev/null || true
  run_sudo pkill -9 -f unattended-upgr 2>/dev/null || true
  # Wait for the lock to actually clear.
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if ! run_sudo fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
  done
  log "WARNING: dpkg lock still held — installs may fail"
}

step_apt_packages() {
  log "Installing apt prereqs"
  run_sudo apt-get update -qq
  run_sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    build-essential pkg-config libssl-dev ca-certificates curl git \
    python3-venv python3-pip
}

step_docker_engine() {
  if command -v docker >/dev/null 2>&1; then
    log "Docker already present: $(docker --version)"
  else
    log "Installing Docker Engine from docker.com repo (NOT snap — it conflicts with systemd's docker.socket)"
    run_sudo install -m 0755 -d /etc/apt/keyrings
    run_sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
    run_sudo chmod a+r /etc/apt/keyrings/docker.asc
    . /etc/os-release
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu ${VERSION_CODENAME} stable" |
      run_sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
    run_sudo apt-get update -qq
    run_sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq docker-ce docker-ce-cli containerd.io
    run_sudo systemctl enable --now docker
  fi

  # Disable any snap-installed docker — having both running is a guaranteed
  # rabbit hole (this bit us hard once already).
  if command -v snap >/dev/null 2>&1 && snap list docker >/dev/null 2>&1; then
    log "Disabling snap docker to avoid daemon conflict"
    run_sudo snap stop docker 2>/dev/null || true
    run_sudo snap disable docker 2>/dev/null || true
  fi

  if ! id -nG "$USER" | grep -qw docker; then
    log "Adding $USER to the docker group (re-login required for it to take effect)"
    run_sudo usermod -aG docker "$USER"
  fi
}

step_rustup() {
  if command -v cargo >/dev/null 2>&1 || [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    log "Rust already installed: $(rustc --version)"
  else
    log "Installing Rust toolchain via rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
      sh -s -- -y --default-toolchain stable --no-modify-path
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
}

step_clone_ctox() {
  if [ -d "$INSTALL_DIR/.git" ]; then
    log "CTOX checkout exists at $INSTALL_DIR — pulling latest of $REPO_BRANCH"
    git -C "$INSTALL_DIR" fetch --depth 1 origin "$REPO_BRANCH"
    git -C "$INSTALL_DIR" checkout -B "$REPO_BRANCH" "origin/$REPO_BRANCH"
  else
    log "Cloning CTOX into $INSTALL_DIR (branch=$REPO_BRANCH)"
    git clone --depth 1 --branch "$REPO_BRANCH" "$REPO_URL" "$INSTALL_DIR"
  fi
}

step_legacy_marker() {
  local marker="$INSTALL_DIR/contracts/history/creation-ledger.md"
  if [ ! -f "$marker" ]; then
    log "Creating legacy marker stub (looks_like_ctox_root requires it)"
    mkdir -p "$(dirname "$marker")"
    echo "# Legacy marker required by looks_like_ctox_root" > "$marker"
  fi
}

step_build_ctox() {
  if [ "$SKIP_BUILD" = "1" ]; then
    log "SKIP_BUILD=1 — skipping cargo build"
    return 0
  fi
  log "Building ctox (release)"
  ( cd "$INSTALL_DIR" && cargo build --release --bin ctox )
  log "Building codex-exec (release) — this takes ~12 min on a cold cache"
  ( cd "$INSTALL_DIR/tools/agent-runtime" && cargo build --release --bin codex-exec )
}

step_harbor_venv() {
  log "Creating Harbor venv at $HARBOR_VENV_DIR (Python 3.12+ required)"
  if [ ! -d "$HARBOR_VENV_DIR" ]; then
    python3 -m venv "$HARBOR_VENV_DIR"
  fi
  # shellcheck disable=SC1091
  . "$HARBOR_VENV_DIR/bin/activate"
  python -m pip install --upgrade --quiet pip
  log "Installing harbor-framework from $HARBOR_REPO_URL"
  if [ ! -d "$HOME/harbor-src" ]; then
    git clone --depth 1 "$HARBOR_REPO_URL" "$HOME/harbor-src"
  fi
  ( cd "$HOME/harbor-src" && pip install --quiet -e . )
  log "Installing ctox-harbor adapter"
  ( cd "$INSTALL_DIR/harbor-adapter" && pip install --quiet -e . )
  python -c 'from ctox_harbor import CtoxAgent; print("[bench-vps] adapter ok:", CtoxAgent.name())'
}

step_summary() {
  cat <<EOF

================================================================
[bench-vps] install complete

CTOX  : $INSTALL_DIR
ctox  : $INSTALL_DIR/target/release/ctox
codex : $INSTALL_DIR/tools/agent-runtime/target/release/codex-exec
harbor: $HARBOR_VENV_DIR  (source ./activate then \`harbor --help\`)

To run a 5-task smoke:

    . $HARBOR_VENV_DIR/bin/activate
    export OPENAI_API_KEY='sk-...'                  # or other provider key
    export CTOX_HOST_TARBALL=/tmp/ctox-bundle.tgz   # see bundle command below
    export CTOX_CHAT_TURN_TIMEOUT_SECS=1200
    export CTOX_BENCH_MODEL=gpt-5.4                  # or MiniMax-M2.7, etc.
    export CTOX_BENCH_PRESET=quality

    # Pack a clean bundle (excluding any per-run state):
    cd $HOME && tar --exclude='ctox-bench/.git' \\
                    --exclude='ctox-bench/target/debug' \\
                    --exclude='ctox-bench/target/release/build' \\
                    --exclude='ctox-bench/target/release/deps' \\
                    --exclude='ctox-bench/target/release/incremental' \\
                    --exclude='ctox-bench/target/release/.fingerprint' \\
                    --exclude='ctox-bench/tools/agent-runtime/target/debug' \\
                    --exclude='ctox-bench/tools/agent-runtime/target/release/build' \\
                    --exclude='ctox-bench/tools/agent-runtime/target/release/deps' \\
                    --exclude='ctox-bench/tools/agent-runtime/target/release/incremental' \\
                    --exclude='ctox-bench/tools/agent-runtime/target/release/.fingerprint' \\
                    -czf /tmp/ctox-bundle.tgz ctox-bench

    harbor run -d terminal-bench/terminal-bench-2 \\
      --agent-import-path ctox_harbor.agent:CtoxAgent \\
      -i terminal-bench/<task-name> \\
      -o \$HOME/ctox-tbench-jobs

NOTE: re-login is required for the docker group membership to take effect
in your current shell (or run \`newgrp docker\`).
================================================================

EOF
}

main() {
  step_pause_unattended_upgrades
  step_apt_packages
  step_docker_engine
  step_rustup
  step_clone_ctox
  step_legacy_marker
  step_build_ctox
  step_harbor_venv
  step_summary
}

main "$@"
