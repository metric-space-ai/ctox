#!/usr/bin/env bash
# ╔══════════════════════════════════════════════════════════════════╗
# ║  CTOX Installer                                                 ║
# ║  curl -fsSL https://raw.githubusercontent.com/metric-space-ai/  ║
# ║  ctox/main/install.sh | bash                                    ║
# ╚══════════════════════════════════════════════════════════════════╝
set -euo pipefail

# ── Configurable defaults ────────────────────────────────────────────────────
CTOX_REPO="${CTOX_REPO:-https://github.com/metric-space-ai/ctox.git}"
CTOX_BRANCH="${CTOX_BRANCH:-main}"
INSTALL_ROOT="${CTOX_INSTALL_ROOT:-$HOME/.local/lib/ctox}"
STATE_ROOT="${CTOX_STATE_ROOT:-$HOME/.local/state/ctox}"
CACHE_ROOT="${CTOX_CACHE_ROOT:-$HOME/.cache/ctox}"
BIN_DIR="${CTOX_BIN_DIR:-$HOME/.local/bin}"
TOOLS_ROOT="${CTOX_TOOLS_ROOT:-$STATE_ROOT/tools}"
DEPENDENCIES_ROOT="${CTOX_DEPENDENCIES_ROOT:-$STATE_ROOT/dependencies}"
TOOLS_ROOT_EXPLICIT=0
DEPENDENCIES_ROOT_EXPLICIT=0
[[ -n "${CTOX_TOOLS_ROOT:-}" ]] && TOOLS_ROOT_EXPLICIT=1
[[ -n "${CTOX_DEPENDENCIES_ROOT:-}" ]] && DEPENDENCIES_ROOT_EXPLICIT=1

# CLI flags
BACKEND_FLAG="${CTOX_BACKEND:-}"
MODEL_FLAG="${CTOX_MODEL:-}"

# Default model — Gemma4-4B runs on CPU as minimal fallback
DEFAULT_MODEL="google/gemma-4-E4B-it"

# ── Internal state ───────────────────────────────────────────────────────────
SCRIPT_DIR=""
IS_ONLINE_INSTALL=0
PLATFORM=""
ARCH=""
ENGINE_FEATURES=""
CUDA_HOME_RESOLVED=""
SELECTED_MODEL=""
CURRENT_STEP=0
REDRAW_INIT=0

# ── ANSI & glyphs ───────────────────────────────────────────────────────────
readonly C_RESET=$'\033[0m'
readonly C_BOLD=$'\033[1m'
readonly C_DIM=$'\033[2m'
readonly C_ITALIC=$'\033[3m'
readonly C_GREEN=$'\033[38;5;114m'
readonly C_YELLOW=$'\033[38;5;221m'
readonly C_CYAN=$'\033[38;5;81m'
readonly C_RED=$'\033[38;5;203m'
readonly C_WHITE=$'\033[97m'
readonly C_GREY=$'\033[38;5;245m'
readonly C_BLUE=$'\033[38;5;111m'
readonly C_MAGENTA=$'\033[38;5;176m'

readonly G_CHECK="${C_GREEN}\xe2\x9c\x94${C_RESET}"    # ✔
readonly G_ARROW="${C_CYAN}\xe2\x96\xb6${C_RESET}"     # ▶
readonly G_CROSS="${C_RED}\xe2\x9c\x98${C_RESET}"       # ✘
readonly G_DOT="${C_GREY}\xe2\x97\x8b${C_RESET}"        # ○
readonly G_SPIN_CHARS=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏')

declare -a STEP_STATUS=()
declare -a STEP_TEXT=()

# ── TUI rendering ───────────────────────────────────────────────────────────
tui_banner() {
  printf '\n'
  printf '  %b%b┌─────────────────────────────────────────────┐%b\n' "$C_BOLD" "$C_CYAN" "$C_RESET"
  printf '  %b%b│%b  %b%bC T O X%b   Installer                        %b%b│%b\n' "$C_BOLD" "$C_CYAN" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET" "$C_BOLD" "$C_CYAN" "$C_RESET"
  printf '  %b%b└─────────────────────────────────────────────┘%b\n' "$C_BOLD" "$C_CYAN" "$C_RESET"
  printf '\n'
}

tui_register_step() {
  STEP_TEXT+=("$1")
  STEP_STATUS+=("pending")
}

tui_start_step() {
  local idx="$1"
  STEP_STATUS[$idx]="active"
  CURRENT_STEP=$((idx + 1))
  tui_redraw
}

tui_complete_step() {
  local idx="$1"
  local detail="${2:-}"
  STEP_STATUS[$idx]="done"
  if [[ -n "$detail" ]]; then
    STEP_TEXT[$idx]="${STEP_TEXT[$idx]}  ${C_GREY}${detail}${C_RESET}"
  fi
  tui_redraw
}

tui_fail_step() {
  local idx="$1"
  local detail="${2:-}"
  STEP_STATUS[$idx]="fail"
  if [[ -n "$detail" ]]; then
    STEP_TEXT[$idx]="${STEP_TEXT[$idx]}  ${C_RED}${detail}${C_RESET}"
  fi
  tui_redraw
}

tui_redraw() {
  local i count="${#STEP_TEXT[@]}"

  # Erase previous render (steps + progress bar + blank)
  if [[ "$REDRAW_INIT" == "1" ]]; then
    printf '\033[%dA' "$((count + 3))" 2>/dev/null || true
  fi
  REDRAW_INIT=1

  for ((i = 0; i < count; i++)); do
    local icon
    case "${STEP_STATUS[$i]}" in
      done)   icon="  $G_CHECK" ;;
      active) icon="  $G_ARROW" ;;
      fail)   icon="  $G_CROSS" ;;
      *)      icon="  $G_DOT" ;;
    esac
    printf '\033[2K%b %b\n' "$icon" "${STEP_TEXT[$i]}"
  done

  # Progress bar
  local done_count=0
  for ((i = 0; i < count; i++)); do
    [[ "${STEP_STATUS[$i]}" == "done" ]] && ((done_count++)) || true
  done
  local pct=0
  [[ "$count" -gt 0 ]] && pct=$((done_count * 100 / count))
  local bar_width=36
  local filled=$((pct * bar_width / 100))
  local empty=$((bar_width - filled))

  printf '\033[2K\n'
  printf '\033[2K  %b' "$C_CYAN"
  local j
  for ((j = 0; j < filled; j++)); do printf '━'; done
  printf '%b' "$C_DIM"
  for ((j = 0; j < empty; j++)); do printf '╌'; done
  printf '%b  %b%d%%%b\n' "$C_RESET" "$C_WHITE" "$pct" "$C_RESET"
}

tui_success() {
  printf '\n'
  printf '  %b%b┌─────────────────────────────────────────────┐%b\n' "$C_BOLD" "$C_GREEN" "$C_RESET"
  printf '  %b%b│%b  %b%bInstallation erfolgreich!%b                    %b%b│%b\n' "$C_BOLD" "$C_GREEN" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET" "$C_BOLD" "$C_GREEN" "$C_RESET"
  printf '  %b%b└─────────────────────────────────────────────┘%b\n' "$C_BOLD" "$C_GREEN" "$C_RESET"
  printf '\n'

  # Service status
  local service_running=0
  if command -v systemctl >/dev/null 2>&1 && systemctl --user is-active --quiet ctox.service 2>/dev/null; then
    service_running=1
  fi

  if [[ "$service_running" -eq 1 ]]; then
    printf '  %b%b\xe2\x9c\x94 CTOX Service läuft im Hintergrund%b\n' "$C_BOLD" "$C_GREEN" "$C_RESET"
    printf '\n'
  fi

  printf '  %bTUI öffnen:%b       %b%bctox%b\n' "$C_GREY" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET"
  printf '  %bStatus prüfen:%b    %b%bctox status%b\n' "$C_GREY" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET"
  printf '  %bUpdate:%b           %b%bctox update apply --latest%b\n' "$C_GREY" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET"
  printf '  %bService steuern:%b  %b%bctox start%b / %b%bctox stop%b\n' "$C_GREY" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET" "$C_BOLD" "$C_WHITE" "$C_RESET"
  printf '\n'

  local shell_rc_hint="${1:-}"
  if [[ -n "$shell_rc_hint" ]]; then
    printf '  %bHinweis: Starte eine neue Shell oder führe aus:%b\n' "$C_YELLOW" "$C_RESET"
    printf '  %b%bsource %s%b\n' "$C_BOLD" "$C_WHITE" "$shell_rc_hint" "$C_RESET"
    printf '\n'
  fi
}

tui_fatal() {
  printf '\n'
  printf '  %b%b✘ %s%b\n\n' "$C_BOLD" "$C_RED" "$1" "$C_RESET" >&2
  exit 1
}

tui_note() {
  printf '\n  %b%s%b\n' "$C_GREY" "$1" "$C_RESET" >&2
}

tui_module_start() {
  printf '\n  %b%b▶ %s%b\n' "$C_BOLD" "$C_CYAN" "$1" "$C_RESET" >&2
}

tui_module_done() {
  local label="$1"
  local started="$2"
  local finished; finished="$(date +%s)"
  printf '  %b✔ %s completed in %ss%b\n' "$C_GREEN" "$label" "$((finished - started))" "$C_RESET" >&2
}

run_build_module() {
  local label="$1"
  local workdir="$2"
  shift 2
  local started; started="$(date +%s)"
  tui_module_start "Compiling ${label}"
  printf '  %bdir:%b %s\n' "$C_GREY" "$C_RESET" "$workdir" >&2
  printf '  %bcmd:%b' "$C_GREY" "$C_RESET" >&2
  printf ' %q' "$@" >&2
  printf '\n' >&2
  (cd "$workdir" && "$@")
  tui_module_done "$label" "$started"
}

# ── Interactive backend selector ─────────────────────────────────────────────
tui_select_backend() {
  local detected_gpu="$1"     # "nvidia", "metal", "none"
  local cuda_ready="$2"       # "yes" or "no"
  local gpu_name="${3:-}"

  # Build option list
  local -a options=()
  local -a option_keys=()
  local recommended=""

  if [[ "$detected_gpu" == "nvidia" && "$cuda_ready" == "yes" ]]; then
    options+=("${C_GREEN}${C_BOLD}CUDA${C_RESET}    ${C_GREY}NVIDIA GPU acceleration (recommended)${C_RESET}")
    option_keys+=("cuda")
    recommended="cuda"
  elif [[ "$detected_gpu" == "nvidia" && "$cuda_ready" == "no" ]]; then
    options+=("${C_YELLOW}${C_BOLD}CUDA${C_RESET}    ${C_GREY}NVIDIA GPU (CUDA toolkit will be installed)${C_RESET}")
    option_keys+=("cuda")
    recommended="cuda"
  fi

  if [[ "$PLATFORM" == "macos" ]]; then
    options+=("${C_BLUE}${C_BOLD}Metal${C_RESET}   ${C_GREY}Apple GPU acceleration (recommended)${C_RESET}")
    option_keys+=("metal")
    recommended="metal"
  fi

  options+=("${C_GREY}${C_BOLD}CPU${C_RESET}     ${C_GREY}No GPU acceleration${C_RESET}")
  option_keys+=("cpu")

  [[ -z "$recommended" ]] && recommended="cpu"

  # If only one real option, auto-select
  if [[ "${#options[@]}" -eq 1 ]]; then
    ENGINE_FEATURES=""
    return
  fi

  printf '\n'
  printf '  %b%bCompute-Backend wählen:%b\n' "$C_BOLD" "$C_WHITE" "$C_RESET"
  if [[ -n "$gpu_name" ]]; then
    printf '  %bErkannt: %s%b\n' "$C_GREY" "$gpu_name" "$C_RESET"
  fi
  printf '\n'

  local i
  for ((i = 0; i < ${#options[@]}; i++)); do
    local marker="  "
    if [[ "${option_keys[$i]}" == "$recommended" ]]; then
      marker="${C_CYAN}▸${C_RESET} "
    fi
    printf '  %b [%b%d%b] %b\n' "$marker" "$C_WHITE" "$((i + 1))" "$C_RESET" "${options[$i]}"
  done

  printf '\n'
  printf '  %bAuswahl [1-%d] (Enter = %s): %b' "$C_BOLD" "${#options[@]}" "$recommended" "$C_RESET"

  local choice
  read -r choice </dev/tty 2>/dev/null || choice=""

  local selected="$recommended"
  if [[ -n "$choice" && "$choice" =~ ^[0-9]+$ ]] && [[ "$choice" -ge 1 && "$choice" -le "${#options[@]}" ]]; then
    selected="${option_keys[$((choice - 1))]}"
  fi

  case "$selected" in
    cuda)
      local features="cuda"
      local fa; fa="$(pick_flash_attn_feature || true)"
      if [[ -n "$fa" ]]; then
        features="$features $fa"
      else
        printf '  %b%bFlashAttention deaktiviert — GPU compute cap < sm_80 oder manuell abgeschaltet.%b\n' \
          "$C_BOLD" "$C_YELLOW" "$C_RESET"
        printf '  %bDecode-Pfad fällt auf Referenz-Attention zurück (~2–3× langsamer).%b\n' \
          "$C_GREY" "$C_RESET"
      fi
      if command -v ldconfig >/dev/null 2>&1; then
        ldconfig -p 2>/dev/null | grep -q 'libnccl' && features="$features nccl"
        ldconfig -p 2>/dev/null | grep -q 'libcudnn' && features="$features cudnn"
      fi
      ENGINE_FEATURES="$features"
      ;;
    metal)
      ENGINE_FEATURES="metal accelerate"
      ;;
    cpu)
      ENGINE_FEATURES=""
      ;;
  esac

  # Clear the selection UI to keep things tidy
  local lines_to_clear=$((${#options[@]} + 5))
  for ((i = 0; i < lines_to_clear; i++)); do
    printf '\033[1A\033[2K'
  done
}

# ── Interactive model selector ───────────────────────────────────────────────
tui_select_model() {
  local has_gpu="$1"  # "yes" or "no"

  # Build model list based on available compute
  local -a models=()
  local -a model_ids=()
  local -a model_descs=()

  if [[ "$has_gpu" == "yes" ]]; then
    models+=("openai/gpt-oss-20b")
    model_descs+=("${C_BOLD}GPT-OSS 20B${C_RESET}         ${C_GREY}Leistungsstärkstes Modell via Candle (empfohlen mit GPU)${C_RESET}")

    models+=("google/gemma-4-26B-A4B-it")
    model_descs+=("${C_BOLD}Gemma4 26B A4B${C_RESET}      ${C_GREY}MoE-Modell via Candle, 4B aktive Parameter${C_RESET}")

    models+=("Qwen/Qwen3.5-9B")
    model_descs+=("${C_BOLD}Qwen 3.5 9B${C_RESET}         ${C_GREY}Multilingual via Candle${C_RESET}")
  fi

  # Gemma4-E4B: always available, served via Candle (works on CPU and GPU).
  models+=("$DEFAULT_MODEL")
  model_descs+=("${C_BOLD}Gemma4 E4B${C_RESET}          ${C_GREY}Candle-Engine, läuft auf CPU und GPU (Standard)${C_RESET}")

  # If only one option or flag provided, skip selection
  if [[ -n "$MODEL_FLAG" ]]; then
    SELECTED_MODEL="$MODEL_FLAG"
    return
  fi
  if [[ "${#models[@]}" -eq 1 ]]; then
    SELECTED_MODEL="$DEFAULT_MODEL"
    return
  fi

  printf '\n'
  printf '  %b%bStandard-Modell wählen:%b\n' "$C_BOLD" "$C_WHITE" "$C_RESET"
  printf '  %bDas Modell wird beim ersten Start heruntergeladen.%b\n' "$C_GREY" "$C_RESET"
  printf '\n'

  local i default_idx=1
  for ((i = 0; i < ${#models[@]}; i++)); do
    local marker="  "
    if [[ "${models[$i]}" == "$DEFAULT_MODEL" && "$has_gpu" != "yes" ]]; then
      marker="${C_CYAN}▸${C_RESET} "
      default_idx=$((i + 1))
    elif [[ "$i" -eq 0 && "$has_gpu" == "yes" ]]; then
      marker="${C_CYAN}▸${C_RESET} "
      default_idx=1
    fi
    printf '  %b [%b%d%b] %b\n' "$marker" "$C_WHITE" "$((i + 1))" "$C_RESET" "${model_descs[$i]}"
  done

  local default_name
  default_name="$(basename "${models[$((default_idx - 1))]}")"
  printf '\n'
  printf '  %bAuswahl [1-%d] (Enter = %s): %b' "$C_BOLD" "${#models[@]}" "$default_name" "$C_RESET"

  local choice
  read -r choice </dev/tty 2>/dev/null || choice=""

  if [[ -n "$choice" && "$choice" =~ ^[0-9]+$ ]] && [[ "$choice" -ge 1 && "$choice" -le "${#models[@]}" ]]; then
    SELECTED_MODEL="${models[$((choice - 1))]}"
  else
    SELECTED_MODEL="${models[$((default_idx - 1))]}"
  fi

  # Clear selection UI
  local lines_to_clear=$((${#models[@]} + 5))
  for ((i = 0; i < lines_to_clear; i++)); do
    printf '\033[1A\033[2K'
  done
}

# ── Platform detection ───────────────────────────────────────────────────────
detect_platform() {
  PLATFORM="$(uname -s)"
  ARCH="$(uname -m)"
  case "$PLATFORM" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *)      tui_fatal "Unsupported platform: $PLATFORM" ;;
  esac
  case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)             tui_fatal "Unsupported architecture: $ARCH" ;;
  esac
}

platform_summary() {
  local os_pretty=""
  case "$PLATFORM" in
    linux) os_pretty="Linux" ;;
    macos) os_pretty="macOS" ;;
  esac
  printf '%s %s' "$os_pretty" "$ARCH"
}

# ── GPU / CUDA detection ────────────────────────────────────────────────────
nvidia_gpu_present() {
  # Use command substitution + bash pattern match instead of `lspci | grep -q`
  # because `set -o pipefail` (active globally in this script) plus grep -q's
  # early exit causes lspci to die with SIGPIPE, which pipefail then reports
  # as a failed pipeline — hiding the GPU even when it is physically present.
  if command -v lspci >/dev/null 2>&1; then
    local lspci_out
    lspci_out="$(lspci 2>/dev/null || true)"
    case "$lspci_out" in
      *"NVIDIA Corporation"*|*"NVIDIA CORPORATION"*|*"nvidia corporation"*) return 0 ;;
    esac
  fi
  if command -v nvidia-smi >/dev/null 2>&1; then
    nvidia-smi -L >/dev/null 2>&1 && return 0
  fi
  return 1
}

nvidia_driver_ready() {
  command -v nvidia-smi >/dev/null 2>&1 || return 1
  nvidia-smi -L >/dev/null 2>&1
}

nvidia_gpu_name() {
  if command -v nvidia-smi >/dev/null 2>&1; then
    nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -n 1
  fi
}

detect_cuda_home() {
  if [[ -n "${CTOX_CUDA_HOME:-}" && -d "${CTOX_CUDA_HOME:-}" ]]; then
    printf '%s\n' "$CTOX_CUDA_HOME"; return
  fi
  # NOTE: cudarc (Rust CUDA bindings) only supports up to CUDA 12.x.
  # CUDA 13.x is intentionally excluded until cudarc adds support.
  local p
  for p in \
    /usr/local/cuda-12.9 /usr/local/cuda-12.8 /usr/local/cuda-12.6 /usr/local/cuda-12.5 \
    /usr/local/cuda-12.4 /usr/local/cuda-12.3 /usr/local/cuda-12.2 /usr/local/cuda-12.1 \
    /usr/local/cuda-12.0 /usr/local/cuda-12 \
    /usr/local/cuda-11.8 /usr/local/cuda-11.7 /usr/local/cuda-11.6 /usr/local/cuda-11.5 /usr/local/cuda-11.4
  do
    [[ -d "$p" ]] && { printf '%s\n' "$p"; return; }
  done
  [[ -d /usr/local/cuda ]] && { printf '%s\n' "/usr/local/cuda"; return; }
  local c
  for c in /usr/local/cuda-*; do
    [[ -x "$c/bin/nvcc" ]] && { printf '%s\n' "$c"; return; }
  done
  if command -v nvcc >/dev/null 2>&1; then
    printf '%s\n' "$(dirname "$(dirname "$(readlink -f "$(command -v nvcc)")")")"
  fi
}

cuda_include_dir() {
  local h="${1:-}" c
  for c in "$h/targets/x86_64-linux/include" "$h/targets/aarch64-linux/include" "$h/include" /usr/include; do
    [[ -d "$c" && -f "$c/cuda_runtime.h" ]] && { printf '%s\n' "$c"; return; }
  done
}

cuda_library_dir() {
  local h="${1:-}" c
  for c in "$h/targets/x86_64-linux/lib" "$h/targets/aarch64-linux/lib" "$h/lib/x86_64-linux-gnu" "$h/lib/aarch64-linux-gnu" "$h/lib64" "$h/lib" /usr/lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu; do
    [[ -d "$c" ]] && { printf '%s\n' "$c"; return; }
  done
}

cuda_toolchain_ready() {
  local h; h="$(detect_cuda_home || true)"
  [[ -n "$h" && -x "$h/bin/nvcc" ]] || return 1
  local inc; inc="$(cuda_include_dir "$h" || true)"
  [[ -n "$inc" ]] || return 1
  local lib; lib="$(cuda_library_dir "$h" || true)"
  [[ -n "$lib" ]] || return 1
  ls "$lib"/libcublas.so* >/dev/null 2>&1 || return 1
  return 0
}

detect_cuda_version() {
  local h="${1:-}"
  [[ -x "$h/bin/nvcc" ]] && { "$h/bin/nvcc" --version 2>/dev/null | sed -n 's/.*release \([0-9]*\.[0-9]*\).*/\1/p' | head -1; return; }
  local b; b="$(basename "$h")"
  [[ "$b" =~ ^cuda-([0-9]+\.[0-9]+)$ ]] && printf '%s\n' "${BASH_REMATCH[1]}"
}

detect_cuda_compute_cap() {
  [[ -n "${CTOX_CUDA_COMPUTE_CAP:-}" ]] && { printf '%s\n' "$CTOX_CUDA_COMPUTE_CAP"; return; }
  if command -v nvidia-smi >/dev/null 2>&1; then
    local cap; cap="$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>/dev/null | head -1 | tr -d '.[:space:]')"
    [[ "$cap" =~ ^[0-9][0-9]+$ ]] && printf '%s\n' "$cap"
  fi
}

detect_cudarc_cuda_version() {
  local v; v="$(detect_cuda_version "${1:-}" || true)"
  [[ -n "$v" ]] || return 0
  local maj min; IFS='.' read -r maj min <<< "$v"
  printf '%s0%s0\n' "$maj" "$min"
}

configure_cuda_env() {
  [[ -n "$CUDA_HOME_RESOLVED" ]] || return 0
  export CUDA_HOME="$CUDA_HOME_RESOLVED" CUDA_PATH="$CUDA_HOME_RESOLVED"
  export CUDA_ROOT="$CUDA_HOME_RESOLVED" CUDA_TOOLKIT_ROOT_DIR="$CUDA_HOME_RESOLVED"
  export CUDA_BIN_PATH="$CUDA_HOME_RESOLVED/bin"
  export PATH="$CUDA_HOME_RESOLVED/bin:$PATH"
  [[ -x "$CUDA_HOME_RESOLVED/bin/nvcc" ]] && export NVCC="$CUDA_HOME_RESOLVED/bin/nvcc" CUDACXX="$CUDA_HOME_RESOLVED/bin/nvcc"
  local inc; inc="$(cuda_include_dir "$CUDA_HOME_RESOLVED" || true)"
  if [[ -n "$inc" ]]; then
    export CUDA_INCLUDE_DIR="$inc"
    [[ "$inc" != "/usr/include" ]] && export CPATH="${inc}:${CPATH:-}" CPLUS_INCLUDE_PATH="${inc}:${CPLUS_INCLUDE_PATH:-}"
  fi
  local lib; lib="$(cuda_library_dir "$CUDA_HOME_RESOLVED" || true)"
  [[ -n "$lib" ]] && export LIBRARY_PATH="${lib}:${LIBRARY_PATH:-}" LD_LIBRARY_PATH="${lib}:${LD_LIBRARY_PATH:-}"
  local cv; cv="$(detect_cudarc_cuda_version "$CUDA_HOME_RESOLVED" || true)"
  [[ -n "$cv" ]] && export CUDARC_CUDA_VERSION="$cv"
  local cc; cc="$(detect_cuda_compute_cap || true)"
  [[ -n "$cc" ]] && export CUDA_COMPUTE_CAP="$cc"
}

# ── Flash-Attention feature picker ──────────────────────────────────────────
# Decide which FlashAttention variant fits the detected GPU:
#   * sm_90+  (H100/H200/Hopper)        → flash-attn-v3  (fastest)
#   * sm_80–89 (A100/A6000/RTX 30xx+)   → flash-attn     (v2)
#   * sm_75 and below (V100/T4/RTX 20xx)→ <none>         (kernels won't build)
# The caller decides whether to warn the user when we fall back to "none".
# Honours CTOX_FLASH_ATTN if explicitly set ("v3", "v2", "off") to let power
# users override the heuristic — e.g. forcing v2 on H100 during debugging.
pick_flash_attn_feature() {
  case "${CTOX_FLASH_ATTN:-auto}" in
    v3) printf 'flash-attn-v3\n'; return ;;
    v2) printf 'flash-attn\n';    return ;;
    off|none|disabled) return ;;
    auto) ;;
    *) ;;
  esac
  local cc; cc="$(detect_cuda_compute_cap || true)"
  # Empty compute cap = no nvidia-smi / no GPU visible at install time. Pick v2
  # as the safe modern default; Ampere+ (sm_80+) is the expected CTOX baseline.
  [[ -z "$cc" ]] && { printf 'flash-attn\n'; return; }
  if [[ "$cc" =~ ^[0-9]+$ ]]; then
    if [[ "$cc" -ge 90 ]]; then
      printf 'flash-attn-v3\n'
    elif [[ "$cc" -ge 80 ]]; then
      printf 'flash-attn\n'
    fi
    # else: pre-Ampere, omit — candle-flash-attn won't compile kernels for it.
  fi
}

# ── CUDA auto-install (Linux apt) ───────────────────────────────────────────
latest_apt_package_matching() {
  apt-cache pkgnames 2>/dev/null | grep -E "$1" | sort -V | tail -1
}

try_install_cuda_stack() {
  [[ "$PLATFORM" == "linux" ]] || return 1
  command -v apt-get >/dev/null 2>&1 || return 1
  can_sudo || return 1

  printf '\n  %b%bCUDA-Toolkit wird installiert...%b\n\n' "$C_BOLD" "$C_YELLOW" "$C_RESET"

  # Kernel headers are required by DKMS to build the nvidia module against the
  # running kernel. Without them the driver package installs but the .ko is
  # never produced, and modprobe/nvidia-smi fail silently afterwards.
  local kern; kern="$(uname -r)"
  run_sudo apt-get update -qq
  run_sudo apt-get install -y "linux-headers-${kern}" 2>/dev/null || true

  # First try: install from existing apt sources
  local packages=() driver cuda
  driver="$(latest_apt_package_matching '^nvidia-driver-[0-9]+-server-open$' || true)"
  [[ -z "$driver" ]] && driver="$(latest_apt_package_matching '^nvidia-driver-[0-9]+-server$' || true)"
  [[ -z "$driver" ]] && driver="$(latest_apt_package_matching '^nvidia-driver-[0-9]+$' || true)"
  [[ -n "$driver" ]] && packages+=("$driver")
  cuda="$(latest_apt_package_matching '^cuda-toolkit-[0-9]+-[0-9]+$' || true)"
  [[ -z "$cuda" ]] && cuda="nvidia-cuda-toolkit"
  packages+=("$cuda")

  [[ "${#packages[@]}" -gt 0 ]] && run_sudo apt-get install -y "${packages[@]}"

  # After apt install, the kernel module exists on disk (via DKMS) but may not
  # be loaded yet — try to modprobe it so nvidia-smi / cudarc can succeed
  # without requiring a reboot. nvidia-modprobe also creates /dev/nvidia*.
  run_sudo modprobe nvidia 2>/dev/null || true
  run_sudo modprobe nvidia-uvm 2>/dev/null || true
  command -v nvidia-modprobe >/dev/null 2>&1 && run_sudo nvidia-modprobe -c 0 -u 2>/dev/null || true

  nvidia_driver_ready && cuda_toolchain_ready && return 0

  # nvcc is the only hard dependency for the build. If it is present, surface
  # a clear reboot-required hint and return success so the source build can
  # proceed — the user will just need to reboot before running CTOX.
  if cuda_toolchain_ready; then
    printf '\n  %b%b⚠ NVIDIA-Treiber installiert, aber Kernel-Modul nicht geladen.%b\n' "$C_BOLD" "$C_YELLOW" "$C_RESET"
    printf '  %b  → Bitte das System neu starten, bevor CTOX gestartet wird.%b\n\n' "$C_YELLOW" "$C_RESET"
    return 0
  fi

  # If the toolchain itself is still missing, add NVIDIA's official CUDA repo
  # and try again (fixes systems with broken Ubuntu-packaged CUDA).
  setup_nvidia_cuda_repo
  return $?
}

# Add NVIDIA's official CUDA repository and install a modern CUDA toolkit.
# This fixes systems with broken Ubuntu-packaged CUDA (e.g. CUDA 12.0 + GCC 13).
setup_nvidia_cuda_repo() {
  [[ "$PLATFORM" == "linux" ]] || return 1
  can_sudo || return 1

  printf '  %bAdding NVIDIA CUDA repository...%b\n' "$C_YELLOW" "$C_RESET"

  # Detect Ubuntu version for the right repo
  local codename=""
  if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    case "${ID:-}-${VERSION_ID:-}" in
      ubuntu-22.04) codename="ubuntu2204" ;;
      ubuntu-24.04) codename="ubuntu2404" ;;
      ubuntu-24.10) codename="ubuntu2404" ;;  # use 24.04 repo
      debian-12)    codename="debian12" ;;
      *) ;;
    esac
  fi
  [[ -n "$codename" ]] || return 1

  # Install CUDA keyring package from NVIDIA
  local keyring_url="https://developer.download.nvidia.com/compute/cuda/repos/${codename}/x86_64/cuda-keyring_1.1-1_all.deb"
  local tmp_deb; tmp_deb="$(mktemp /tmp/cuda-keyring.XXXXX.deb)"
  curl -fsSL "$keyring_url" -o "$tmp_deb" || { rm -f "$tmp_deb"; return 1; }
  run_sudo dpkg -i "$tmp_deb" 2>/dev/null
  rm -f "$tmp_deb"

  # Update and install CUDA 12.6 toolkit (cudarc only supports up to CUDA 12.x)
  run_sudo apt-get update -qq
  local cuda_pkg="cuda-toolkit-12-6"
  if ! apt-cache policy "$cuda_pkg" 2>/dev/null | grep -q 'Candidate:'; then
    # Fallback: try latest 12.x
    cuda_pkg="$(apt-cache pkgnames 2>/dev/null | grep -E '^cuda-toolkit-12-[0-9]+$' | sort -V | tail -1 || true)"
  fi
  if [[ -n "$cuda_pkg" ]]; then
    printf '  %bInstalling %s...%b\n' "$C_YELLOW" "$cuda_pkg" "$C_RESET"
    run_sudo apt-get install -y "$cuda_pkg"
  fi

  # Re-detect CUDA home after installation
  CUDA_HOME_RESOLVED="$(detect_cuda_home || true)"
  configure_cuda_env

  nvidia_driver_ready && cuda_toolchain_ready
}

# ── sudo helper ──────────────────────────────────────────────────────────────
# If CTOX_SUDO_PASSWORD is set, use it for sudo -S (stdin).
# This enables sudo in non-interactive SSH sessions where the user is in
# the sudo group but no NOPASSWD is configured.
run_sudo() {
  if [[ -n "${CTOX_SUDO_PASSWORD:-}" ]]; then
    echo "$CTOX_SUDO_PASSWORD" | sudo -S "$@"
  else
    sudo "$@"
  fi
}

can_sudo() {
  command -v sudo >/dev/null 2>&1 || return 1
  # Try non-interactive sudo first (works if user has NOPASSWD in sudoers)
  sudo -n true 2>/dev/null && return 0
  # If we have a sudo password, test it
  if [[ -n "${CTOX_SUDO_PASSWORD:-}" ]]; then
    printf '%s\n' "$CTOX_SUDO_PASSWORD" | sudo -S true 2>/dev/null && return 0
  fi
  # If tty is available, interactive sudo will prompt
  tty -s 2>/dev/null && return 0
  # If SUDO_ASKPASS is set, sudo can use it
  [[ -n "${SUDO_ASKPASS:-}" ]] && return 0
  return 1
}

# ── Linux system prerequisites ────────────────────────────────────────────────
apt_package_installed() {
  dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q "install ok installed"
}

ensure_linux_discovery_prereqs() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v apt-get >/dev/null 2>&1 || return 0
  can_sudo || return 0
  local packages=()
  for pkg in ripgrep sqlite3 sysstat dnsutils iputils-ping openssl; do
    apt_package_installed "$pkg" || packages+=("$pkg")
  done
  [[ "${#packages[@]}" -eq 0 ]] && return 0
  run_sudo apt-get update -qq
  run_sudo apt-get install -y "${packages[@]}"
}

ensure_codex_linux_build_prereqs() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v apt-get >/dev/null 2>&1 || return 0
  can_sudo || return 0
  local packages=()
  for pkg in libcap-dev; do
    apt_package_installed "$pkg" || packages+=("$pkg")
  done
  [[ "${#packages[@]}" -eq 0 ]] && return 0
  run_sudo apt-get update -qq
  run_sudo apt-get install -y "${packages[@]}"
}

ensure_linux_browser_prereqs() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v apt-get >/dev/null 2>&1 || return 0
  can_sudo || return 0
  local packages=()
  for pkg in nodejs npm; do
    apt_package_installed "$pkg" || packages+=("$pkg")
  done
  [[ "${#packages[@]}" -eq 0 ]] && return 0
  run_sudo apt-get update -qq
  run_sudo apt-get install -y "${packages[@]}"
}

# ── Jami daemon ──────────────────────────────────────────────────────────────
resolve_jami_linux_repo_suffix() {
  [[ -r /etc/os-release ]] || return 1
  # shellcheck disable=SC1091
  . /etc/os-release
  local distro_id="${ID:-}" version_id="${VERSION_ID:-}" id_like="${ID_LIKE:-}"
  case "$distro_id" in
    ubuntu) case "$version_id" in 20.04|22.04|24.04|24.10|25.04) printf 'ubuntu_%s\n' "$version_id"; return 0;; esac;;
    debian) case "$version_id" in 11|12|13) printf 'debian_%s\n' "$version_id"; return 0;; esac;;
  esac
  case "$id_like" in
    *ubuntu*) case "$version_id" in 20.04|22.04|24.04|24.10|25.04) printf 'ubuntu_%s\n' "$version_id"; return 0;; esac;;
    *debian*) case "$version_id" in 11|12|13) printf 'debian_%s\n' "$version_id"; return 0;; esac;;
  esac
  return 1
}

ensure_linux_jami_installed() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v apt-get >/dev/null 2>&1 || return 0
  can_sudo || return 0
  local repo_suffix; repo_suffix="$(resolve_jami_linux_repo_suffix || true)"
  if [[ -z "$repo_suffix" ]]; then return 0; fi
  run_sudo apt-get install -y gnupg dirmngr ca-certificates curl --no-install-recommends 2>/dev/null
  local tmp; tmp="$(mktemp)"
  curl -fsSL https://dl.jami.net/public-key.gpg -o "$tmp"
  run_sudo install -m 0644 "$tmp" /usr/share/keyrings/jami-archive-keyring.gpg
  rm -f "$tmp"
  printf 'deb [signed-by=/usr/share/keyrings/jami-archive-keyring.gpg] https://dl.jami.net/stable/%s/ jami main\n' "$repo_suffix" | run_sudo tee /etc/apt/sources.list.d/jami.list >/dev/null
  run_sudo apt-get update -qq
  run_sudo apt-get install -y jami-daemon dbus-x11
}

jami_daemon_binary_present() {
  [[ -x /usr/libexec/jamid ]] || command -v jamid >/dev/null 2>&1 || command -v jami-daemon >/dev/null 2>&1
}

# ── Process cleanup ──────────────────────────────────────────────────────────
stop_ctox_services() {
  if [[ "$PLATFORM" != "linux" ]] || ! command -v systemctl >/dev/null 2>&1; then return 0; fi
  systemctl --user stop ctox.service >/dev/null 2>&1 || true
  systemctl --user stop cto-jami-daemon.service >/dev/null 2>&1 || true
}

kill_residual_processes() {
  command -v pkill >/dev/null 2>&1 || return 0
  pkill -x ctox >/dev/null 2>&1 || true
}

sync_skills_to_codex_home() {
  local source_root="$1"
  local codex_home="${CODEX_HOME:-$HOME/.codex}"
  local target="$codex_home/skills"
  local src_packs="$source_root/skills/packs"
  mkdir -p "$target"

  # System skills are embedded into the ctox binary via include_dir! and
  # registered into SQLite + materialized to $CODEX_HOME/skills/.system/ at
  # service start by install_system_skills() in src/harness/core/. We do
  # not copy them here — the Rust path is the source of truth.

  # Packs in the repo are organized into category subfolders (deploy/,
  # development/, content/, etc.) for clarity; in CODEX_HOME they remain
  # flat (one folder per skill). Walk the tree, find every SKILL.md, copy
  # the containing dir flat to $CODEX_HOME/skills/<skill>/.
  if [[ -d "$src_packs" ]]; then
    while IFS= read -r -d '' skill_md; do
      local skill_dir; skill_dir="$(dirname "$skill_md")"
      local name; name="$(basename "$skill_dir")"
      rm -rf "$target/$name"
      cp -R "$skill_dir" "$target/$name"
    done < <(find "$src_packs" -name SKILL.md -type f -print0)
  fi
}

# ── Speaches runtime (TTS/STT) ──────────────────────────────────────────────
ensure_uv_runtime() {
  command -v uv >/dev/null 2>&1 && return 0
  command -v curl >/dev/null 2>&1 || return 0
  curl -LsSf https://astral.sh/uv/install.sh 2>/dev/null | sh 2>/dev/null
  [[ -x "$HOME/.local/bin/uv" ]] && export PATH="$HOME/.local/bin:$PATH"
}

prepare_speaches_runtime() {
  local source_root="$1"
  local runtime_root="$source_root/tools/speaches-runtime"
  local venv_root="$runtime_root/.venv"
  local requirements_lock="$runtime_root/requirements.lock"
  [[ -f "$requirements_lock" ]] || return 0
  command -v uv >/dev/null 2>&1 || { ensure_uv_runtime; command -v uv >/dev/null 2>&1 || return 0; }
  mkdir -p "$runtime_root"
  local started; started="$(date +%s)"
  tui_module_start "Preparing TTS/STT runtime"
  printf '  %bdir:%b %s\n' "$C_GREY" "$C_RESET" "$runtime_root" >&2
  uv venv "$venv_root"
  uv pip install --python "$venv_root/bin/python" --requirement "$requirements_lock"
  tui_module_done "TTS/STT runtime" "$started"
}

# ── Browser / Playwright ─────────────────────────────────────────────────────
# Installs the Playwright workspace at the INSTALLED ctox's runtime reference
# dir (not the source tree), which is where ctox looks at runtime. Also
# triggers the browser-binary install so `ctox web search` works without
# requiring the user to run `ctox web browser-prepare` afterwards.
setup_browser_runtime() {
  local source_root="$1"
  local started; started="$(date +%s)"
  tui_module_start "Preparing browser / Playwright runtime"
  ensure_linux_browser_prereqs
  command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 && command -v npx >/dev/null 2>&1 || return 0
  # Prefer the managed-install binary so its runtime-root resolution lands at
  # the final location the user will run. Fall back to the source-tree binary
  # only if the managed symlink is not yet present.
  local ctox_bin=""
  if [[ -x "$BIN_DIR/ctox" ]]; then
    ctox_bin="$BIN_DIR/ctox"
  elif [[ -x "$INSTALL_ROOT/current/bin/ctox" ]]; then
    ctox_bin="$INSTALL_ROOT/current/bin/ctox"
  elif [[ -x "$source_root/bin/ctox" ]]; then
    ctox_bin="$source_root/bin/ctox"
  else
    return 0
  fi
  # `web browser-prepare --install-reference --install-browser` is the
  # documented one-shot: it installs package.json, runs npm install, and
  # fetches the Chromium browser binary that Playwright drives.
  printf '  %bctox:%b %s\n' "$C_GREY" "$C_RESET" "$ctox_bin" >&2
  "$ctox_bin" web browser-prepare --install-reference --install-browser || \
    "$ctox_bin" browser install-reference || true
  # Resolve the real reference dir via the doctor output so this works whether
  # the user set CTOX_WEB_BROWSER_REFERENCE_DIR or relies on the default.
  local browser_ref
  browser_ref="$("$ctox_bin" web google-doctor 2>/dev/null | awk -F'"' '/"reference_dir"/ {print $4; exit}')"
  [[ -z "$browser_ref" ]] && browser_ref="$source_root/runtime/browser/interactive-reference"
  if [[ -d "$browser_ref" ]]; then
    (cd "$browser_ref" && npm run doctor >/dev/null 2>&1) || true
    if "$ctox_bin" browser doctor 2>/dev/null | grep -q '"chromium_fallback_executable": null'; then
      if [[ "$PLATFORM" == "linux" ]]; then
        tui_note "Installing Chromium browser binary via Playwright"
        (cd "$browser_ref" && npx playwright install --with-deps chromium) || true
      else
        "$ctox_bin" browser install-reference --skip-npm-install --install-browser || true
      fi
    fi
  fi
  tui_module_done "browser / Playwright runtime" "$started"
}

build_google_fetch_helper() {
  local source_root="$1"
  local tool_dir="$source_root/tools/google-fetch"
  [[ -f "$tool_dir/Cargo.toml" ]] || return 0
  command -v cargo >/dev/null 2>&1 || return 0
  run_build_module "ctox Google fetch helper" "$tool_dir" cargo build --release --bin ctox-google-fetch
}

# ── systemd services ─────────────────────────────────────────────────────────
install_ctox_service() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v systemctl >/dev/null 2>&1 || return 0
  local wrapper_root="$1"
  local service_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  local install_root_line=""
  [[ -n "${CTOX_INSTALL_ROOT:-$INSTALL_ROOT}" ]] && install_root_line="Environment=CTOX_INSTALL_ROOT=$INSTALL_ROOT"
  mkdir -p "$service_dir" "$STATE_ROOT"

  cat > "$service_dir/ctox.service" <<SVCEOF
[Unit]
Description=CTOX Background Service
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=0

[Service]
Type=simple
WorkingDirectory=$wrapper_root
Environment=CTOX_ROOT=$wrapper_root
Environment=CTOX_STATE_ROOT=$STATE_ROOT
$install_root_line
ExecStart=$BIN_DIR/ctox service --foreground
Restart=always
RestartSec=5
KillMode=control-group
TimeoutStopSec=20

[Install]
WantedBy=default.target
SVCEOF

  systemctl --user daemon-reload
  systemctl --user enable ctox.service >/dev/null 2>&1 || true
  systemctl --user restart ctox.service >/dev/null 2>&1 || true
  # Enable lingering so service runs without login session
  command -v loginctl >/dev/null 2>&1 && can_sudo && \
    run_sudo loginctl enable-linger "$USER" >/dev/null 2>&1 || true
}

install_jami_service() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v systemctl >/dev/null 2>&1 || return 0
  jami_daemon_binary_present || return 0
  local wrapper_root="$1"
  local service_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  mkdir -p "$service_dir"

  cat > "$service_dir/cto-jami-daemon.service" <<SVCEOF
[Unit]
Description=CTOX Jami Daemon
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=0

[Service]
Type=simple
WorkingDirectory=$wrapper_root
EnvironmentFile=-$STATE_ROOT/jami_dbus_env
ExecStart=$BIN_DIR/ctox jami-daemon --foreground
Restart=always
RestartSec=5
KillMode=control-group
TimeoutStopSec=20

[Install]
WantedBy=default.target
SVCEOF

  systemctl --user daemon-reload
  systemctl --user enable cto-jami-daemon.service >/dev/null 2>&1 || true
  systemctl --user restart cto-jami-daemon.service >/dev/null 2>&1 || true
}

# ── Platform capabilities JSON ───────────────────────────────────────────────
write_platform_capabilities() {
  local state_root="$1"
  mkdir -p "$state_root"
  local cap_path="$state_root/platform_capabilities.json"
  local gpus="[]"

  if command -v nvidia-smi >/dev/null 2>&1; then
    local gpu_csv; gpu_csv="$(nvidia-smi --query-gpu=index,name,memory.total,compute_cap --format=csv,noheader,nounits 2>/dev/null || true)"
    if [[ -n "$gpu_csv" ]]; then
      gpus="["
      local first=1
      while IFS=',' read -r idx name mem cap; do
        idx="$(echo "$idx" | tr -d '[:space:]')"
        name="$(echo "$name" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
        mem="$(echo "$mem" | tr -d '[:space:]')"
        cap="$(echo "$cap" | tr -d '[:space:]')"
        [[ "$first" -eq 1 ]] && first=0 || gpus="$gpus,"
        gpus="$gpus{\"index\":$idx,\"name\":\"$name\",\"total_mb\":$mem,\"compute_capability\":\"$cap\"}"
      done <<< "$gpu_csv"
      gpus="$gpus]"
    fi
  fi

  local cuda_avail="false" nccl_avail="false" flash_avail="false"
  [[ "$ENGINE_FEATURES" == *cuda* ]] && cuda_avail="true"
  # Report flash_attn_available from the actual feature string, not from the
  # presence of "cuda" — pre-Ampere GPUs get cuda without flash-attn (see
  # pick_flash_attn_feature).
  [[ "$ENGINE_FEATURES" == *flash-attn* ]] && flash_avail="true"
  [[ "$ENGINE_FEATURES" == *nccl* ]] && nccl_avail="true"

  cat > "$cap_path" <<CAPEOF
{
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source": "install.sh",
  "cuda_available": $cuda_avail,
  "nccl_available": $nccl_avail,
  "flash_attn_available": $flash_avail,
  "gpus": $gpus
}
CAPEOF
}

# ── Runtime state layout ─────────────────────────────────────────────────────
ensure_runtime_state_layout() {
  local source_root="$1"
  mkdir -p "$STATE_ROOT"
  local runtime_link="$source_root/runtime"
  if [[ "$STATE_ROOT" == "$source_root/runtime" ]]; then
    mkdir -p "$source_root/runtime"
    return 0
  fi
  if [[ -L "$runtime_link" ]]; then return 0; fi
  [[ -e "$runtime_link" ]] && rm -rf "$runtime_link"
  ln -s "$STATE_ROOT" "$runtime_link"
}

# ── Wrapper script ───────────────────────────────────────────────────────────
write_wrapper_script() {
  local wrapper_root="$1"
  mkdir -p "$BIN_DIR"
  local launcher_binary="$BIN_DIR/ctox-real"

  # CRITICAL: remove any existing $BIN_DIR/ctox *before* writing. If
  # setup_managed_install left a symlink here pointing at the real Rust
  # binary under $wrapper_root/bin/ctox, `cat >` would follow the
  # symlink and overwrite the binary with this bash wrapper. The wrapper's
  # `exec "$wrapper_root/bin/ctox"` then execs itself in an
  # infinite loop, pegging a core at 100% CPU forever. We observed this
  # manifest as a ~1.5h hang during `setup_browser_runtime` on a clean VPS.
  rm -f "$BIN_DIR/ctox"

  cat > "$BIN_DIR/ctox" <<WRAPEOF
#!/usr/bin/env bash
set -euo pipefail
unset DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH DYLD_FRAMEWORK_PATH
export CTOX_ROOT="$wrapper_root"
export CTOX_STATE_ROOT="$STATE_ROOT"
export CTOX_INSTALL_ROOT="$INSTALL_ROOT"
exec "$launcher_binary" "\$@"
WRAPEOF
  chmod +x "$BIN_DIR/ctox"

  # Do not publish helper runtimes as public binaries. The supported install
  # surface is `ctox`; `ctox-desktop-host` is an optional remote-access helper,
  # and the GUI desktop app ships as a separate package.
}

write_managed_launch_wrapper() {
  local destination="$1"
  local wrapper_root="$2"
  local launcher_binary="$3"
  mkdir -p "$(dirname "$destination")"
  rm -f "$destination"
  cat > "$destination" <<WRAPEOF
#!/usr/bin/env bash
set -euo pipefail
unset DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH DYLD_FRAMEWORK_PATH
export CTOX_ROOT="$wrapper_root"
export CTOX_STATE_ROOT="$STATE_ROOT"
export CTOX_INSTALL_ROOT="$INSTALL_ROOT"
exec "$launcher_binary" "\$@"
WRAPEOF
  chmod +x "$destination"
}

populate_rebuild_release_layout() {
  local release_root="$1"
  local ctox_binary=""
  local ctox_desktop_host_binary=""

  ctox_binary="$(resolve_ctox_binary_path "$release_root" 2>/dev/null || true)"
  ctox_desktop_host_binary="$(resolve_ctox_desktop_host_binary_path "$release_root" 2>/dev/null || true)"

  mkdir -p "$release_root/bin" "$release_root/tools/model-runtime/bin" "$INSTALL_ROOT/bin"

  if [[ -n "$ctox_binary" && -x "$ctox_binary" ]]; then
    cp "$ctox_binary" "$release_root/bin/ctox-real"
    write_managed_launch_wrapper "$release_root/bin/ctox" "$release_root" "$release_root/bin/ctox-real"
    cp "$release_root/bin/ctox-real" "$BIN_DIR/ctox-real" 2>/dev/null || true
    write_managed_launch_wrapper "$INSTALL_ROOT/bin/ctox" "$release_root" "$BIN_DIR/ctox-real"
  fi

  if [[ -n "$ctox_desktop_host_binary" && -x "$ctox_desktop_host_binary" ]]; then
    cp "$ctox_desktop_host_binary" "$release_root/bin/ctox-desktop-host"
    cp "$release_root/bin/ctox-desktop-host" "$INSTALL_ROOT/bin/ctox-desktop-host" 2>/dev/null || true
  fi

  if [[ -x "$TOOLS_ROOT/model-runtime/bin/ctox-engine" ]]; then
    cp "$TOOLS_ROOT/model-runtime/bin/ctox-engine" "$release_root/tools/model-runtime/bin/ctox-engine" 2>/dev/null || true
  elif [[ -x "$release_root/tools/model-runtime/target/release/ctox-engine" ]]; then
    cp "$release_root/tools/model-runtime/target/release/ctox-engine" "$release_root/tools/model-runtime/bin/ctox-engine" 2>/dev/null || true
  fi
}

# ── Rust toolchain ───────────────────────────────────────────────────────────
ensure_rust_toolchain() {
  if command -v cargo >/dev/null 2>&1 || [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    return 0
  fi
  if [[ "$PLATFORM" == "linux" ]] && command -v apt-get >/dev/null 2>&1 && can_sudo; then
    local needed=()
    for pkg in build-essential pkg-config libssl-dev ca-certificates libdbus-1-dev libfontconfig1-dev; do
      dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null | grep -q "install ok installed" || needed+=("$pkg")
    done
    [[ "${#needed[@]}" -gt 0 ]] && { run_sudo apt-get update -qq; run_sudo apt-get install -y "${needed[@]}"; }
  fi
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal 2>/dev/null
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env" 2>/dev/null || true
}

resolve_cargo() {
  [[ -x "$HOME/.cargo/bin/cargo" ]] && { printf '%s\n' "$HOME/.cargo/bin/cargo"; return; }
  command -v cargo
}

# ── CUDA build prerequisites ─────────────────────────────────────────────────
ensure_cuda_build_prereqs() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  command -v apt-get >/dev/null 2>&1 || return 0
  can_sudo || return 0
  [[ "$ENGINE_FEATURES" == *cuda* ]] || return 0

  # Determine the CUDA version suffix from CUDA_HOME (e.g. "12-6" from /usr/local/cuda-12.6)
  local cuda_ver_suffix=""
  if [[ -n "${CUDA_HOME_RESOLVED:-}" ]]; then
    local ver; ver="$(basename "$CUDA_HOME_RESOLVED" | sed 's/^cuda-//' | tr '.' '-')"
    # Only use if it looks like a version (e.g. "12-6", not "cuda" or "usr")
    [[ "$ver" =~ ^[0-9]+-[0-9]+$ ]] && cuda_ver_suffix="$ver"
  fi

  local packages="" pkg
  if [[ -n "$cuda_ver_suffix" ]]; then
    # Install dev packages matching the detected CUDA version
    for pkg in \
      "cuda-driver-dev-${cuda_ver_suffix}" \
      "cuda-cudart-dev-${cuda_ver_suffix}" \
      "cuda-nvcc-${cuda_ver_suffix}" \
      "cuda-nvrtc-dev-${cuda_ver_suffix}" \
      "libcublas-dev-${cuda_ver_suffix}" \
      "libcurand-dev-${cuda_ver_suffix}"
    do
      apt-cache policy "$pkg" 2>/dev/null | grep -q 'Candidate:' && packages="$packages $pkg"
    done
  else
    # Fallback: find latest 12.x packages (never 13.x — cudarc doesn't support it)
    local pattern
    for pattern in \
      '^cuda-driver-dev-12-[0-9]+$' \
      '^cuda-cudart-dev-12-[0-9]+$' \
      '^cuda-nvcc-12-[0-9]+$' \
      '^cuda-nvrtc-dev-12-[0-9]+$' \
      '^libcublas-dev-12-[0-9]+$' \
      '^libcurand-dev-12-[0-9]+$'
    do
      pkg="$(latest_apt_package_matching "$pattern" || true)"
      [[ -n "$pkg" ]] && packages="$packages $pkg"
    done
  fi

  if [[ -n "$packages" ]]; then
    run_sudo apt-get update -qq
    # shellcheck disable=SC2086
    run_sudo apt-get install -y $packages
  fi
}

ensure_nccl_runtime() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  [[ "$ENGINE_FEATURES" == *cuda* ]] || return 0
  command -v apt-get >/dev/null 2>&1 || return 0
  can_sudo || return 0

  # Check if NCCL is already present
  if command -v ldconfig >/dev/null 2>&1 && ldconfig -p 2>/dev/null | grep -q 'libnccl'; then
    return 0
  fi

  # Check if packages are available
  if ! apt-cache policy libnccl2 2>/dev/null | grep -q 'Candidate:'; then
    return 0
  fi

  run_sudo apt-get update -qq
  run_sudo apt-get install -y libnccl2 libnccl-dev

  # Re-detect nccl feature if just installed
  if command -v ldconfig >/dev/null 2>&1 && ldconfig -p 2>/dev/null | grep -q 'libnccl'; then
    [[ "$ENGINE_FEATURES" == *nccl* ]] || ENGINE_FEATURES="$ENGINE_FEATURES nccl"
  fi
}

cuda_smoke_test() {
  local cuda_home="${1:-}"
  [[ -n "$cuda_home" && -x "$cuda_home/bin/nvcc" ]] || return 1

  local inc; inc="$(cuda_include_dir "$cuda_home" || true)"
  [[ -n "$inc" ]] || return 1
  local lib; lib="$(cuda_library_dir "$cuda_home" || true)"

  local tmp; tmp="$(mktemp -d)"
  cat > "$tmp/smoke.cu" <<'CUDASRC'
#include <cuda_runtime.h>
__global__ void smoke_kernel() {}
int main() {
  smoke_kernel<<<1, 1>>>();
  return cudaDeviceSynchronize();
}
CUDASRC

  local ok=0
  (
    export CUDA_HOME="$cuda_home" CUDA_PATH="$cuda_home"
    export CUDACXX="$cuda_home/bin/nvcc" NVCC="$cuda_home/bin/nvcc"
    export PATH="$cuda_home/bin:$PATH"
    [[ "$inc" != "/usr/include" ]] && export CPATH="${inc}:${CPATH:-}" CPLUS_INCLUDE_PATH="${inc}:${CPLUS_INCLUDE_PATH:-}"
    [[ -n "$lib" ]] && export LIBRARY_PATH="${lib}:${LIBRARY_PATH:-}" LD_LIBRARY_PATH="${lib}:${LD_LIBRARY_PATH:-}"
    "$cuda_home/bin/nvcc" -c "$tmp/smoke.cu" -o "$tmp/smoke.o" >/dev/null 2>&1
  ) && ok=1

  rm -rf "$tmp"
  [[ "$ok" -eq 1 ]]
}

# ── Build ────────────────────────────────────────────────────────────────────
build_ctox() {
  local source_root="$1"
  local cargo; cargo="$(resolve_cargo)"

  # 1. Build main CTOX binary
  run_build_module "ctox CLI" "$source_root" "$cargo" build --release --bin ctox
  mkdir -p "$source_root/bin"
  cp "$source_root/target/release/ctox" "$source_root/bin/" 2>/dev/null || true

  if [[ -f "$source_root/desktop/Cargo.toml" ]]; then
    run_build_module "ctox desktop host" "$source_root" "$cargo" build --release --manifest-path desktop/Cargo.toml --bin ctox-desktop-host
    cp "$source_root/desktop/target/release/ctox-desktop-host" "$source_root/bin/" 2>/dev/null || true
  fi

  # 2. If CUDA features requested, prepare build environment
  if [[ "$ENGINE_FEATURES" == *cuda* ]]; then
    # nvcc needs writable temp space. Use /tmp (the standard location).
    # We previously tried a local .nvcc_tmp dir, but TMPDIR is also used
    # by rustc — setting it to a non-standard path broke cargo compilation.

    # Install CUDA dev headers and libraries for kernel compilation
    ensure_cuda_build_prereqs

    # Install NCCL for multi-GPU support
    ensure_nccl_runtime

    # Smoke test: verify nvcc can actually compile a kernel
    if [[ -n "$CUDA_HOME_RESOLVED" ]]; then
      if ! cuda_smoke_test "$CUDA_HOME_RESOLVED"; then
        printf '\n  %b%bCUDA smoke test failed at %s — attempting to install NVIDIA CUDA Toolkit...%b\n' "$C_BOLD" "$C_YELLOW" "$CUDA_HOME_RESOLVED" "$C_RESET"
        # System CUDA is broken (e.g. CUDA 12.0 + GCC 13 incompatibility).
        # Try to install NVIDIA's official CUDA toolkit from their repo.
        if setup_nvidia_cuda_repo 2>&1; then
          # Re-detect and reconfigure
          CUDA_HOME_RESOLVED="$(detect_cuda_home || true)"
          configure_cuda_env
          # Try smoke test again with new toolkit
          if [[ -n "$CUDA_HOME_RESOLVED" ]] && cuda_smoke_test "$CUDA_HOME_RESOLVED"; then
            printf '  %b%bCUDA toolkit repaired: %s%b\n' "$C_BOLD" "$C_GREEN" "$CUDA_HOME_RESOLVED" "$C_RESET"
          else
            printf '\n  %b%bCUDA smoke test still failing after toolkit install.%b\n' "$C_BOLD" "$C_RED" "$C_RESET" >&2
            printf '  %bCUDA_HOME: %s%b\n' "$C_RED" "${CUDA_HOME_RESOLVED:-not found}" "$C_RESET" >&2
            printf '  %bA reboot may be required after driver installation.%b\n\n' "$C_RED" "$C_RESET" >&2
            return 1
          fi
        else
          printf '  %b%bCould not install NVIDIA CUDA Toolkit automatically.%b\n' "$C_BOLD" "$C_RED" "$C_RESET" >&2
          printf '  %bInstall manually: https://developer.nvidia.com/cuda-downloads%b\n\n' "$C_RED" "$C_RESET" >&2
          return 1
        fi
      fi
    fi
  fi

  # 3. Build the internal inference runtime. CPU-only hosts still need a
  # usable ctox-engine payload, so an empty ENGINE_FEATURES must not skip the
  # build entirely.
  if [[ -f "$source_root/tools/model-runtime/Cargo.toml" ]]; then
    local cargo_features=""
    local engine_label="${ENGINE_FEATURES:-cpu-only}"
    local -a engine_build_cmd=(
      "$cargo" build --release --package ctox-engine-cli --bin ctox-engine
    )

    if [[ -n "$ENGINE_FEATURES" ]]; then
      for feat in $ENGINE_FEATURES; do
        [[ -n "$cargo_features" ]] && cargo_features="$cargo_features,"
        cargo_features="${cargo_features}${feat}"
      done
      engine_build_cmd+=(--features "$cargo_features")
    fi

    # Build the internal inference runtime binary (not the workspace default)
    run_build_module "ctox model engine (${engine_label})" "$source_root/tools/model-runtime" "${engine_build_cmd[@]}"

    # Write feature stamp for runtime verification
    local stamp_dir="$source_root/tools/model-runtime/target/release"
    local install_engine_dir="$TOOLS_ROOT/model-runtime/bin"
    mkdir -p "$stamp_dir" "$install_engine_dir" "$TOOLS_ROOT/model-runtime"
    printf 'features=%s;cudarc=%s\n' \
      "$engine_label" \
      "${CUDARC_CUDA_VERSION:-none}" \
      > "$stamp_dir/ctox-engine.features"
    cp "$source_root/tools/model-runtime/target/release/ctox-engine" "$install_engine_dir/" 2>/dev/null || true
    cp "$stamp_dir/ctox-engine.features" "$TOOLS_ROOT/model-runtime/ctox-engine.features" 2>/dev/null || true

    # Clean up any leftover .nvcc_tmp from previous installer versions
    rm -rf "$source_root/.nvcc_tmp" 2>/dev/null || true
  fi

  # 4. Do not build or publish a secondary agent-runtime CLI. CTOX consumes
  # the integrated agent-runtime source tree in-process via direct session.
  if [[ -f "$source_root/src/harness/Cargo.toml" ]]; then
    printf '  %b%bintegrated agent-runtime kept in-process; no standalone runtime CLI built%b\n' \
      "$C_BOLD" "$C_GREY" "$C_RESET" >&2
  fi
}

# Prefer the built binary's embedded version, then git tags, then Cargo.toml.
resolve_ctox_binary_path() {
  local source_root="$1"
  local candidate
  for candidate in \
    "$source_root/target/release/ctox" \
    "$source_root/target/debug/ctox" \
    "$source_root/bin/ctox"
  do
    [[ -x "$candidate" ]] && { printf '%s\n' "$candidate"; return 0; }
  done
  return 1
}

resolve_ctox_desktop_host_binary_path() {
  local source_root="$1"
  local candidate
  for candidate in \
    "$source_root/desktop/target/release/ctox-desktop-host" \
    "$source_root/desktop/target/debug/ctox-desktop-host" \
    "$source_root/bin/ctox-desktop-host"
  do
    [[ -x "$candidate" ]] && { printf '%s\n' "$candidate"; return 0; }
  done
  return 1
}

resolve_source_version() {
  local source_root="$1"

  if command -v git >/dev/null 2>&1; then
    local described
    described="$(git -C "$source_root" describe --tags --dirty --match 'v[0-9]*' 2>/dev/null || true)"
    described="${described#v}"
    [[ -n "$described" ]] && { printf '%s\n' "$described"; return 0; }
  fi

  local binary
  binary="$(resolve_ctox_binary_path "$source_root" 2>/dev/null || true)"
  if [[ -n "$binary" && -x "$binary" ]]; then
    local embedded
    embedded="$("$binary" version 2>/dev/null | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
    [[ -n "$embedded" ]] && { printf '%s\n' "$embedded"; return 0; }
  fi

  grep '^version' "$source_root/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

# ── Managed installation layout ─────────────────────────────────────────────
setup_managed_install() {
  local source_root="$1"
  mkdir -p "$INSTALL_ROOT" "$STATE_ROOT" "$CACHE_ROOT" "$BIN_DIR" "$TOOLS_ROOT" "$DEPENDENCIES_ROOT"

  local version; version="$(resolve_source_version "$source_root")"
  local release_name="v${version}"
  local release_dir="$INSTALL_ROOT/releases/$release_name"

  [[ -d "$release_dir" ]] && rm -rf "$release_dir"
  mkdir -p "$release_dir"

  rsync -a --exclude='target' --exclude='runtime' --exclude='.git' "$source_root/" "$release_dir/"

  mkdir -p "$release_dir/bin"
  local ctox_binary=""
  ctox_binary="$(resolve_ctox_binary_path "$source_root" 2>/dev/null || true)"
  [[ -n "$ctox_binary" ]] && cp "$ctox_binary" "$release_dir/bin/ctox-real" 2>/dev/null || true
  local ctox_desktop_host_binary=""
  ctox_desktop_host_binary="$(resolve_ctox_desktop_host_binary_path "$source_root" 2>/dev/null || true)"
  [[ -n "$ctox_desktop_host_binary" ]] && cp "$ctox_desktop_host_binary" "$release_dir/bin/ctox-desktop-host" 2>/dev/null || true
  mkdir -p "$INSTALL_ROOT/bin"
  if [[ -x "$release_dir/bin/ctox-real" ]]; then
    cp "$release_dir/bin/ctox-real" "$BIN_DIR/ctox-real" 2>/dev/null || true
    write_managed_launch_wrapper "$release_dir/bin/ctox" "$release_dir" "$BIN_DIR/ctox-real"
    write_managed_launch_wrapper "$INSTALL_ROOT/bin/ctox" "$release_dir" "$BIN_DIR/ctox-real"
  fi
  [[ -x "$release_dir/bin/ctox-desktop-host" ]] && cp "$release_dir/bin/ctox-desktop-host" "$INSTALL_ROOT/bin/ctox-desktop-host" 2>/dev/null || true
  ln -sfn "$release_dir" "$INSTALL_ROOT/current"
  [[ ! -e "$release_dir/runtime" ]] && ln -sfn "$STATE_ROOT" "$release_dir/runtime"

  cat > "$INSTALL_ROOT/install_manifest.json" <<MANIFEST
{
  "schema_version": 1,
  "install_root": "$INSTALL_ROOT",
  "state_root": "$STATE_ROOT",
  "current_release": "$release_name",
  "previous_release": null,
  "release_channel": {
    "kind": "github",
    "repo": "metric-space-ai/ctox",
    "api_base": "https://api.github.com",
    "token_env": null
  },
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
MANIFEST

  printf '%s\n' "$ENGINE_FEATURES" > "$STATE_ROOT/engine_features"
}

# ── Jami DBus env file ────────────────────────────────────────────────────────
write_jami_dbus_env() {
  [[ "$PLATFORM" == "linux" ]] || return 0
  local state_root="$1"
  local dbus_env_path="$state_root/jami_dbus_env"
  local uid; uid="$(id -u)"
  local bus_path="/run/user/${uid}/bus"

  # Only write the file when a user bus socket actually exists
  if [[ -S "$bus_path" ]]; then
    printf 'DBUS_SESSION_BUS_ADDRESS=unix:path=%s\n' "$bus_path" > "$dbus_env_path"
  fi
}

# ── Runtime config SQLite bootstrap ──────────────────────────────────────────
sql_escape() {
  printf "%s" "$1" | sed "s/'/''/g"
}

write_runtime_sqlite_config() {
  local state_root="$1"
  mkdir -p "$state_root"
  local db_path="$state_root/ctox.sqlite3"
  local engine_binary="$TOOLS_ROOT/model-runtime/bin/ctox-engine"
  local engine_log="$state_root/logs/model-runtime/ctox-engine.log"
  local model="${SELECTED_MODEL:-$DEFAULT_MODEL}"

  # Model-specific defaults
  local port="1234" arch="gpt_oss" max_seq="131072" paged_attn="auto"
  local tp_backend="disabled" isq="" pa_cache_type="f8e4m3" pa_mem_frac="0.80"
  local disable_nccl="1" world_size=""

  case "$model" in
    openai/gpt-oss-20b) ;;
    google/gemma-4-26B-A4B-it)
      port="1234"; arch=""; max_seq="131072"; isq=""; pa_mem_frac="0.80" ;;
    google/gemma-4-E4B-it)
      port="1234"; arch=""; max_seq="131072"; isq="" ;;
    Qwen/Qwen3.6-35B-A3B)
      port="1235"; arch=""; max_seq="131072"; isq="Q4K"; pa_cache_type="turboquant3"; pa_mem_frac="0.80" ;;
    Qwen/Qwen3.5-9B|Qwen/Qwen3.5-4B)
      port="1235"; arch=""; max_seq="65536"; isq="Q4K" ;;
  esac

  local proxy_port="12434"

  # Auxiliary models require a local inference runtime.  On hosts without a
  # GPU (detected_gpu=none) leave them empty so CTOX does not attempt to
  # spawn backends that can never start.
  local emb_model="" emb_port="1237" emb_isq="Q4K"
  local stt_model="" stt_port="1238" stt_isq="Q4K"
  local tts_model="" tts_port="1239" tts_isq="Q4K"
  if [[ "${CTOX_DETECTED_GPU:-none}" != "none" ]]; then
    emb_model="Qwen/Qwen3-Embedding-0.6B"
    stt_model="Systran/faster-whisper-small [CPU]"
    tts_model="speaches-ai/piper-en_US-lessac-medium [CPU EN]"
  fi

  sqlite3 "$db_path" <<SQL
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS runtime_env_kv (
  env_key TEXT PRIMARY KEY,
  env_value TEXT NOT NULL
);
BEGIN IMMEDIATE;
DELETE FROM runtime_env_kv;
INSERT INTO runtime_env_kv(env_key, env_value) VALUES
('CTOX_ENGINE_MODEL', '$(sql_escape "${CTOX_ENGINE_MODEL:-$model}")'),
('CTOX_ENGINE_PORT', '$(sql_escape "${CTOX_ENGINE_PORT:-$port}")'),
('CTOX_ENGINE_ARCH', '$(sql_escape "${CTOX_ENGINE_ARCH:-$arch}")'),
('CTOX_ENGINE_MAX_SEQS', '$(sql_escape "${CTOX_ENGINE_MAX_SEQS:-1}")'),
('CTOX_ENGINE_MAX_BATCH_SIZE', '$(sql_escape "${CTOX_ENGINE_MAX_BATCH_SIZE:-1}")'),
('CTOX_ENGINE_MAX_SEQ_LEN', '$(sql_escape "${CTOX_ENGINE_MAX_SEQ_LEN:-$max_seq}")'),
('CTOX_ENGINE_PAGED_ATTN', '$(sql_escape "${CTOX_ENGINE_PAGED_ATTN:-$paged_attn}")'),
('CTOX_ENGINE_TENSOR_PARALLEL_BACKEND', '$(sql_escape "${CTOX_ENGINE_TENSOR_PARALLEL_BACKEND:-$tp_backend}")'),
('CTOX_ENGINE_ISQ', '$(sql_escape "${CTOX_ENGINE_ISQ:-$isq}")'),
('CTOX_ENGINE_PA_CACHE_TYPE', '$(sql_escape "${CTOX_ENGINE_PA_CACHE_TYPE:-$pa_cache_type}")'),
('CTOX_ENGINE_PA_MEMORY_FRACTION', '$(sql_escape "${CTOX_ENGINE_PA_MEMORY_FRACTION:-$pa_mem_frac}")'),
('CTOX_ENGINE_PA_CONTEXT_LEN', '$(sql_escape "${CTOX_ENGINE_PA_CONTEXT_LEN:-}")'),
('CTOX_ENGINE_CUDA_VISIBLE_DEVICES', '$(sql_escape "${CTOX_ENGINE_CUDA_VISIBLE_DEVICES:-}")'),
('CTOX_ENGINE_DISABLE_NCCL', '$(sql_escape "${CTOX_ENGINE_DISABLE_NCCL:-$disable_nccl}")'),
('CTOX_ENGINE_MN_LOCAL_WORLD_SIZE', '$(sql_escape "${CTOX_ENGINE_MN_LOCAL_WORLD_SIZE:-$world_size}")'),
('CTOX_ENGINE_TOPOLOGY', '$(sql_escape "${CTOX_ENGINE_TOPOLOGY:-}")'),
('CTOX_ENGINE_NUM_DEVICE_LAYERS', '$(sql_escape "${CTOX_ENGINE_NUM_DEVICE_LAYERS:-}")'),
('CTOX_CHAT_MODEL', '$(sql_escape "${CTOX_CHAT_MODEL:-$model}")'),
('CTOX_CHAT_MODEL_MAX_CONTEXT', '$(sql_escape "${CTOX_CHAT_MODEL_MAX_CONTEXT:-131072}")'),
('CTOX_CHAT_COMPACTION_THRESHOLD_PERCENT', '$(sql_escape "${CTOX_CHAT_COMPACTION_THRESHOLD_PERCENT:-75}")'),
('CTOX_ACTIVE_MODEL', '$(sql_escape "${CTOX_ACTIVE_MODEL:-$model}")'),
('CTOX_PROXY_HOST', '$(sql_escape "${CTOX_PROXY_HOST:-127.0.0.1}")'),
('CTOX_PROXY_PORT', '$(sql_escape "${CTOX_PROXY_PORT:-$proxy_port}")'),
('CTOX_UPSTREAM_BASE_URL', '$(sql_escape "${CTOX_UPSTREAM_BASE_URL:-http://127.0.0.1:$port}")'),
('CTOX_INSTALL_ROOT', '$(sql_escape "${CTOX_INSTALL_ROOT:-$INSTALL_ROOT}")'),
('CTOX_STATE_ROOT', '$(sql_escape "${CTOX_STATE_ROOT:-$state_root}")'),
('CTOX_CACHE_ROOT', '$(sql_escape "${CTOX_CACHE_ROOT:-$CACHE_ROOT}")'),
('CTOX_BIN_DIR', '$(sql_escape "${CTOX_BIN_DIR:-$BIN_DIR}")'),
('CTOX_TOOLS_ROOT', '$(sql_escape "${CTOX_TOOLS_ROOT:-$TOOLS_ROOT}")'),
('CTOX_DEPENDENCIES_ROOT', '$(sql_escape "${CTOX_DEPENDENCIES_ROOT:-$DEPENDENCIES_ROOT}")'),
('CTOX_ENGINE_BINARY', '$(sql_escape "${CTOX_ENGINE_BINARY:-$engine_binary}")'),
('CTOX_ENGINE_LOG', '$(sql_escape "${CTOX_ENGINE_LOG:-$engine_log}")'),
('CTOX_EMBEDDING_MODEL', '$(sql_escape "${CTOX_EMBEDDING_MODEL:-$emb_model}")'),
('CTOX_EMBEDDING_PORT', '$(sql_escape "${CTOX_EMBEDDING_PORT:-$emb_port}")'),
('CTOX_EMBEDDING_ISQ', '$(sql_escape "${CTOX_EMBEDDING_ISQ:-$emb_isq}")'),
('CTOX_STT_MODEL', '$(sql_escape "${CTOX_STT_MODEL:-$stt_model}")'),
('CTOX_STT_PORT', '$(sql_escape "${CTOX_STT_PORT:-$stt_port}")'),
('CTOX_STT_ISQ', '$(sql_escape "${CTOX_STT_ISQ:-$stt_isq}")'),
('CTOX_TTS_MODEL', '$(sql_escape "${CTOX_TTS_MODEL:-$tts_model}")'),
('CTOX_TTS_PORT', '$(sql_escape "${CTOX_TTS_PORT:-$tts_port}")'),
('CTOX_TTS_ISQ', '$(sql_escape "${CTOX_TTS_ISQ:-$tts_isq}")'),
('CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES', '$(sql_escape "${CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES:-}")'),
('CTOX_EMBEDDING_CUDA_VISIBLE_DEVICES', '$(sql_escape "${CTOX_EMBEDDING_CUDA_VISIBLE_DEVICES:-}")'),
('CTOX_STT_CUDA_VISIBLE_DEVICES', '$(sql_escape "${CTOX_STT_CUDA_VISIBLE_DEVICES:-}")'),
('CTOX_TTS_CUDA_VISIBLE_DEVICES', '$(sql_escape "${CTOX_TTS_CUDA_VISIBLE_DEVICES:-}")'),
('CTOX_CHAT_SHARE_AUXILIARY_GPUS', '$(sql_escape "${CTOX_CHAT_SHARE_AUXILIARY_GPUS:-1}")'),
('CTOX_AUXILIARY_GPU_LAYER_RESERVATION_MAP', '$(sql_escape "${CTOX_AUXILIARY_GPU_LAYER_RESERVATION_MAP:-}")'),
('CTOX_EMBEDDING_GPU_LAYER_RESERVATION', '$(sql_escape "${CTOX_EMBEDDING_GPU_LAYER_RESERVATION:-0.30}")'),
('CTOX_STT_GPU_LAYER_RESERVATION', '$(sql_escape "${CTOX_STT_GPU_LAYER_RESERVATION:-0.55}")'),
('CTOX_TTS_GPU_LAYER_RESERVATION', '$(sql_escape "${CTOX_TTS_GPU_LAYER_RESERVATION:-0.35}")'),
('CTOX_WEB_SEARCH_ENABLED', 'true'),
('CTOX_WEB_SEARCH_PROVIDER', 'auto'),
('CTOX_WEB_SEARCH_TIMEOUT_MS', '10000'),
('CTOX_WEB_SEARCH_TOP_K', '5'),
('CTOX_WEB_SEARCH_MAX_TOP_K', '8'),
('CTOX_WEB_SEARCH_CACHE_TTL_SECS', '86400'),
('CTOX_WEB_SEARCH_PAGE_CACHE_TTL_SECS', '259200');
COMMIT;
SQL
}

# ── Rebuild mode (called by `ctox update apply`) ────────────────────────────
run_rebuild() {
  local root="$1"
  mkdir -p "$root"
  root="$(cd "$root" && pwd)"  # absolute path
  detect_platform

  if [[ -z "${CTOX_ENGINE_FEATURES:-}" && -f "${CTOX_STATE_ROOT:-$root/runtime}/engine_features" ]]; then
    ENGINE_FEATURES="$(cat "${CTOX_STATE_ROOT:-$root/runtime}/engine_features")"
  else
    ENGINE_FEATURES="$(detect_engine_features_auto || true)"
  fi

  CUDA_HOME_RESOLVED="$(detect_cuda_home || true)"
  configure_cuda_env
  build_ctox "$root"
  populate_rebuild_release_layout "$root"

  STATE_ROOT="${CTOX_STATE_ROOT:-$root/runtime}"
  TOOLS_ROOT="${CTOX_TOOLS_ROOT:-$STATE_ROOT/tools}"
  DEPENDENCIES_ROOT="${CTOX_DEPENDENCIES_ROOT:-$STATE_ROOT/dependencies}"

  # Keep the web stack operational after source upgrades, not only after first
  # install. These are idempotent and leave existing runtime config untouched.
  ensure_web_runtime_defaults "$STATE_ROOT"
  setup_browser_runtime "$root" || true
  build_google_fetch_helper "$root" || true

  # Ensure ctox is available as a command everywhere
  write_wrapper_script "$root"

  # Ensure BIN_DIR is in PATH for future shells
  local shell_rc=""
  case "${SHELL:-}" in
    */zsh)  shell_rc="$HOME/.zshrc" ;;
    */bash) shell_rc="$HOME/.bashrc" ;;
    */fish) shell_rc="$HOME/.config/fish/config.fish" ;;
  esac
  if [[ -n "$shell_rc" ]] && ! grep -q "$BIN_DIR" "$shell_rc" 2>/dev/null; then
    printf '\nexport PATH="%s:$PATH"\n' "$BIN_DIR" >> "$shell_rc"
  fi
}

ensure_web_runtime_defaults() {
  local state_root="$1"
  command -v sqlite3 >/dev/null 2>&1 || return 0
  mkdir -p "$state_root"
  local db_path="$state_root/ctox.sqlite3"
  sqlite3 "$db_path" <<SQL
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS runtime_env_kv (
  env_key TEXT PRIMARY KEY,
  env_value TEXT NOT NULL
);
INSERT OR IGNORE INTO runtime_env_kv(env_key, env_value) VALUES
('CTOX_WEB_SEARCH_ENABLED', 'true'),
('CTOX_WEB_SEARCH_PROVIDER', 'auto'),
('CTOX_WEB_SEARCH_TIMEOUT_MS', '10000'),
('CTOX_WEB_SEARCH_TOP_K', '5'),
('CTOX_WEB_SEARCH_MAX_TOP_K', '8'),
('CTOX_WEB_SEARCH_CACHE_TTL_SECS', '86400'),
('CTOX_WEB_SEARCH_PAGE_CACHE_TTL_SECS', '259200');
SQL
}

# Auto-detect without interactivity (for rebuild / non-interactive)
detect_engine_features_auto() {
  [[ -n "${CTOX_ENGINE_FEATURES:-}" ]] && { printf '%s\n' "$CTOX_ENGINE_FEATURES"; return; }
  if [[ "$PLATFORM" == "macos" ]]; then printf '%s\n' "metal accelerate"; return; fi
  if ! cuda_toolchain_ready; then printf '%s\n' ""; return; fi
  local f="cuda"
  local fa; fa="$(pick_flash_attn_feature || true)"
  [[ -n "$fa" ]] && f="$f $fa"
  if command -v ldconfig >/dev/null 2>&1; then
    ldconfig -p 2>/dev/null | grep -q 'libnccl' && f="$f nccl"
    ldconfig -p 2>/dev/null | grep -q 'libcudnn' && f="$f cudnn"
  fi
  printf '%s\n' "$f"
}

# ── Parse CLI arguments ─────────────────────────────────────────────────────
parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --rebuild)
        shift
        run_rebuild "${1:-.}"
        exit 0
        ;;
      --backend=*)
        BACKEND_FLAG="${1#*=}"
        ;;
      --backend)
        shift
        BACKEND_FLAG="${1:-}"
        ;;
      --branch=*)
        CTOX_BRANCH="${1#*=}"
        ;;
      --install-root=*)
        INSTALL_ROOT="${1#*=}"
        ;;
      --state-root=*)
        STATE_ROOT="${1#*=}"
        ;;
      --cache-root=*)
        CACHE_ROOT="${1#*=}"
        ;;
      --bin-dir=*)
        BIN_DIR="${1#*=}"
        ;;
      --tools-root=*)
        TOOLS_ROOT="${1#*=}"
        TOOLS_ROOT_EXPLICIT=1
        ;;
      --dependencies-root=*)
        DEPENDENCIES_ROOT="${1#*=}"
        DEPENDENCIES_ROOT_EXPLICIT=1
        ;;
      --repo=*)
        CTOX_REPO="${1#*=}"
        ;;
      --features=*)
        export CTOX_ENGINE_FEATURES="${1#*=}"
        ;;
      --model=*)
        MODEL_FLAG="${1#*=}"
        ;;
      --model)
        shift
        MODEL_FLAG="${1:-}"
        ;;
      --help|-h)
        printf 'Usage: install.sh [OPTIONS]\n\n'
        printf 'Options:\n'
        printf '  --backend=<cuda|metal|cpu>  Set compute backend (skip interactive selection)\n'
        printf '  --model=<model>             Set default model (default: google/gemma-4-E4B-it)\n'
        printf '  --features=<features>       Override engine features (comma or space separated)\n'
        printf '  --branch=<branch>           Git branch to install from (default: main)\n'
        printf '  --repo=<url>                Git repository URL (default: metric-space-ai/ctox)\n'
        printf '  --install-root=<path>       Installation directory (default: ~/.local/lib/ctox)\n'
        printf '  --state-root=<path>         State directory (default: ~/.local/state/ctox)\n'
        printf '  --cache-root=<path>         Cache directory (default: ~/.cache/ctox)\n'
        printf '  --bin-dir=<path>            Binary symlink directory (default: ~/.local/bin)\n'
        printf '  --tools-root=<path>         Canonical install root for helper tools (default: <state>/tools)\n'
        printf '  --dependencies-root=<path>  Canonical install root for dependencies (default: <state>/dependencies)\n'
        printf '  --rebuild                   Rebuild in-place (used by ctox update)\n'
        printf '  --help                      Show this help\n\n'
        printf 'Environment:\n'
        printf '  CTOX_BACKEND                Same as --backend\n'
        printf '  CTOX_ENGINE_FEATURES        Override engine features (space-separated)\n'
        printf '  CTOX_CUDA_HOME              Override CUDA home directory\n'
        printf '  CTOX_CUDA_COMPUTE_CAP       Override CUDA compute capability\n'
        printf '  CTOX_INSTALL_ROOT           Same as --install-root\n'
        printf '  CTOX_STATE_ROOT             Same as --state-root\n'
        printf '  CTOX_CACHE_ROOT             Same as --cache-root\n'
        printf '  CTOX_BIN_DIR                Same as --bin-dir\n'
        printf '  CTOX_TOOLS_ROOT             Canonical install root for CTOX-managed helper tools\n'
        printf '  CTOX_DEPENDENCIES_ROOT      Canonical install root for downloaded dependencies/assets\n'
        printf '  CTOX_REPO                   Same as --repo\n'
        printf '  CTOX_BRANCH                 Same as --branch\n\n'
        exit 0
        ;;
      *)
        ;;
    esac
    shift
  done
}

# ── Main install flow ────────────────────────────────────────────────────────
main() {
  parse_args "$@"
  if [[ "$TOOLS_ROOT_EXPLICIT" -eq 0 ]]; then
    TOOLS_ROOT="$STATE_ROOT/tools"
  fi
  if [[ "$DEPENDENCIES_ROOT_EXPLICIT" -eq 0 ]]; then
    DEPENDENCIES_ROOT="$STATE_ROOT/dependencies"
  fi

  # Determine if online install or from existing checkout
  if [[ -f "$(dirname "${BASH_SOURCE[0]:-$0}")/Cargo.toml" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
    IS_ONLINE_INSTALL=0
  else
    IS_ONLINE_INSTALL=1
  fi

  tui_banner

  # ── Register steps ──
  tui_register_step "System erkennen"
  tui_register_step "Hardware erkennen"
  tui_register_step "Compute-Backend konfigurieren"
  tui_register_step "Modell konfigurieren"
  tui_register_step "Systemvoraussetzungen"
  tui_register_step "Rust-Toolchain"
  tui_register_step "Quellcode bereitstellen"
  tui_register_step "Skills vorbereiten"
  tui_register_step "CTOX + Engine + Agent kompilieren"
  tui_register_step "Installation einrichten"
  tui_register_step "Laufzeit-Dienste konfigurieren"
  tui_register_step "Sprachverarbeitung + Browser"
  tui_register_step "Abschluss"
  tui_redraw

  # ── Step 0: Platform ──
  tui_start_step 0
  detect_platform
  tui_complete_step 0 "$(platform_summary)"

  # ── Step 1: Hardware ──
  tui_start_step 1
  local detected_gpu="none"
  local gpu_name=""
  local cuda_ready="no"

  if [[ "$PLATFORM" == "macos" ]]; then
    detected_gpu="metal"
    gpu_name="Apple Silicon / Metal"
  elif nvidia_gpu_present; then
    detected_gpu="nvidia"
    gpu_name="$(nvidia_gpu_name || echo 'NVIDIA GPU')"
    nvidia_driver_ready && cuda_toolchain_ready && cuda_ready="yes"
  fi

  case "$detected_gpu" in
    nvidia) tui_complete_step 1 "NVIDIA $gpu_name" ;;
    metal)  tui_complete_step 1 "$gpu_name" ;;
    *)      tui_complete_step 1 "Keine GPU erkannt" ;;
  esac

  # ── Step 2: Backend selection ──
  tui_start_step 2

  if [[ -n "$BACKEND_FLAG" ]]; then
    case "$BACKEND_FLAG" in
      cuda)
        if [[ "$detected_gpu" != "nvidia" ]]; then
          tui_fail_step 2 "CUDA angefordert, aber keine NVIDIA GPU"
          tui_fatal "Kann CUDA nicht ohne NVIDIA GPU aktivieren."
        fi
        if [[ "$cuda_ready" != "yes" ]]; then
          if ! try_install_cuda_stack; then
            tui_fail_step 2 "CUDA-Installation fehlgeschlagen"
            tui_fatal "CUDA-Toolkit konnte nicht installiert werden."
          fi
        fi
        ENGINE_FEATURES="cuda"
        local fa; fa="$(pick_flash_attn_feature || true)"
        if [[ -n "$fa" ]]; then
          ENGINE_FEATURES="$ENGINE_FEATURES $fa"
        else
          printf '  %b%bFlashAttention deaktiviert — GPU compute cap < sm_80 oder manuell abgeschaltet.%b\n' \
            "$C_BOLD" "$C_YELLOW" "$C_RESET"
        fi
        if command -v ldconfig >/dev/null 2>&1; then
          ldconfig -p 2>/dev/null | grep -q 'libnccl' && ENGINE_FEATURES="$ENGINE_FEATURES nccl"
          ldconfig -p 2>/dev/null | grep -q 'libcudnn' && ENGINE_FEATURES="$ENGINE_FEATURES cudnn"
        fi
        ;;
      metal)
        ENGINE_FEATURES="metal accelerate"
        ;;
      cpu)
        ENGINE_FEATURES=""
        ;;
      *)
        tui_fatal "Unbekanntes Backend: $BACKEND_FLAG (cuda, metal, cpu)"
        ;;
    esac
  elif [[ -n "${CTOX_ENGINE_FEATURES:-}" ]]; then
    ENGINE_FEATURES="$CTOX_ENGINE_FEATURES"
  else
    tui_select_backend "$detected_gpu" "$cuda_ready" "$gpu_name"
    if [[ "$ENGINE_FEATURES" == *cuda* && "$cuda_ready" != "yes" ]]; then
      if ! try_install_cuda_stack; then
        tui_fail_step 2 "CUDA-Installation fehlgeschlagen"
        tui_fatal "CUDA-Toolkit konnte nicht installiert werden. Neustart erforderlich?"
      fi
    fi
  fi

  CUDA_HOME_RESOLVED="$(detect_cuda_home || true)"
  configure_cuda_env

  local backend_desc="CPU-only (Candle)"
  local has_gpu="no"
  if [[ "$ENGINE_FEATURES" == *cuda* ]]; then
    local cv; cv="$(detect_cuda_version "$CUDA_HOME_RESOLVED" || true)"
    local cc; cc="$(detect_cuda_compute_cap || true)"
    backend_desc="CUDA ${cv:-?} (SM ${cc:-?})"
    has_gpu="yes"
  elif [[ "$ENGINE_FEATURES" == *metal* ]]; then
    backend_desc="Metal + Accelerate"
    has_gpu="yes"
  fi
  tui_complete_step 2 "$backend_desc"

  # ── Step 3: Model selection ──
  tui_start_step 3

  tui_select_model "$has_gpu"

  # All local inference now runs through the integrated Candle engine.
  local model_serving="Candle"
  if [[ "$has_gpu" != "yes" && "$SELECTED_MODEL" != *gemma-4-E4B* && "$SELECTED_MODEL" != *gemma-4-E2B* ]]; then
    SELECTED_MODEL="$DEFAULT_MODEL"
  fi

  local model_short; model_short="$(basename "$SELECTED_MODEL")"
  tui_complete_step 3 "${model_short} (${model_serving})"

  # ── Step 4: System prerequisites ──
  tui_start_step 4
  local prereq_details=""
  ensure_linux_discovery_prereqs 2>/dev/null && prereq_details="discovery"
  ensure_linux_jami_installed 2>/dev/null && prereq_details="${prereq_details:+$prereq_details, }jami"
  tui_complete_step 4 "${prereq_details:-ok}"

  # ── Step 5: Rust ──
  tui_start_step 5
  ensure_rust_toolchain
  local rv; rv="$("$(resolve_cargo)" --version 2>/dev/null | awk '{print $2}')"
  tui_complete_step 5 "v${rv:-?}"

  # ── Step 6: Source ──
  tui_start_step 6
  local source_root
  if [[ "$IS_ONLINE_INSTALL" -eq 1 ]]; then
    source_root="$CACHE_ROOT/src"
    if [[ -d "$source_root/.git" ]]; then
      (cd "$source_root" && git fetch origin "$CTOX_BRANCH" && git checkout "origin/$CTOX_BRANCH") >/dev/null 2>&1
      tui_complete_step 6 "aktualisiert von $CTOX_BRANCH"
    else
      rm -rf "$source_root"
      git clone --depth 1 --branch "$CTOX_BRANCH" "$CTOX_REPO" "$source_root" 2>/dev/null
      tui_complete_step 6 "geklont ($CTOX_BRANCH)"
    fi
  else
    source_root="$SCRIPT_DIR"
    tui_complete_step 6 "lokaler Quellcode"
  fi

  # ── Step 7: Skills sync ──
  tui_start_step 7
  tui_complete_step 7 "System-Skills aus Repo-Quelle"

  # ── Step 8: Build (CTOX + Engine + Agent Runtime) ──
  tui_start_step 8

  # Stop old processes before build to avoid file locks
  stop_ctox_services
  kill_residual_processes

  build_ctox "$source_root"
  local feat_short="${ENGINE_FEATURES:-cpu}"
  [[ -z "$ENGINE_FEATURES" ]] && feat_short="cpu"
  tui_complete_step 8 "$feat_short"

  # ── Step 9: Install layout ──
  tui_start_step 9
  ensure_runtime_state_layout "$source_root"
  setup_managed_install "$source_root"
  local active_root="$INSTALL_ROOT/current"
  write_wrapper_script "$active_root"
  sync_skills_to_codex_home "$source_root"
  write_platform_capabilities "$STATE_ROOT"
  CTOX_DETECTED_GPU="$detected_gpu" write_runtime_sqlite_config "$STATE_ROOT"
  tui_complete_step 9 "$INSTALL_ROOT"

  # ── Step 10: Services ──
  tui_start_step 10
  install_ctox_service "$active_root"
  install_jami_service "$active_root"
  tui_complete_step 10 "systemd"

  # ── Step 11: Speaches + Browser ──
  tui_start_step 11
  local runtime_details=""
  if prepare_speaches_runtime "$source_root"; then
    runtime_details="TTS/STT"
  fi
  if setup_browser_runtime "$source_root"; then
    runtime_details="${runtime_details:+$runtime_details, }Browser"
  fi
  if build_google_fetch_helper "$source_root"; then
    runtime_details="${runtime_details:+$runtime_details, }Google-Fetch"
  fi
  tui_complete_step 11 "${runtime_details:-bereitgestellt}"

  # ── Step 12: Finalize ──
  tui_start_step 12

  # Ensure BIN_DIR is in PATH
  local shell_rc="" shell_rc_hint=""
  case "${SHELL:-}" in
    */zsh)  shell_rc="$HOME/.zshrc" ;;
    */bash) shell_rc="$HOME/.bashrc" ;;
    */fish) shell_rc="$HOME/.config/fish/config.fish" ;;
  esac
  if [[ -n "$shell_rc" ]] && ! grep -q "$BIN_DIR" "$shell_rc" 2>/dev/null; then
    printf '\nexport PATH="%s:$PATH"\n' "$BIN_DIR" >> "$shell_rc"
    shell_rc_hint="$shell_rc"
  fi

  # Write Jami DBus env file so the Jami adapter can reach the daemon
  write_jami_dbus_env "$STATE_ROOT"

  # Set update channel
  "$BIN_DIR/ctox" update channel set-github --repo metric-space-ai/ctox 2>/dev/null || true

  tui_complete_step 12 "PATH + Update-Channel"

  tui_success "$shell_rc_hint"
}

main "$@"
