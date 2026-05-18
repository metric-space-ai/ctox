// Origin: CTOX
// License: Apache-2.0

//! Hardware-fact probe for the Apple-Silicon host this engine targets.
//!
//! Stage 1 — read-only, no Metal SDK needed. Shells out to system tools
//! that ship with macOS (`sysctl`, `system_profiler`, `sw_vers`),
//! parses the relevant fields, prints a stable JSON object, and exits.
//! The output is the canonical input to the "Capture hardware facts"
//! step of the optimization skill (see method-playbook §4 + §1).
//!
//! Stage 2 will add (`metal` feature-gated) real GPU-side probes —
//! sustained stream bandwidth via a memcpy MSL kernel, SIMDgroup-matrix
//! throughput, and `MTLDevice supportsFamily:` membership for
//! `MTLGPUFamilyApple9` / `MTLGPUFamilyMetal4`. Stage-1 prints the
//! advertised capabilities and explicitly tags them
//! `"source": "system_profiler"` so the autotuner doesn't confuse them
//! with measured numbers.
//!
//! Stage 1 deliberately has no Metal/Cocoa dep so a fresh checkout
//! probes correctly without Xcode being on PATH.

use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Serialize)]
struct ProbeReport {
    chip: Option<String>,
    cpu_cores_total: Option<u32>,
    gpu_cores: Option<u32>,
    metal_support: Option<String>,
    unified_memory_bytes: Option<u64>,
    macos_product_name: Option<String>,
    macos_product_version: Option<String>,
    macos_build_version: Option<String>,
    /// Always `"system_profiler"` in stage 1 — these are advertised
    /// values, not measured ceilings.
    source: &'static str,
    /// Stage-1 puts a stable hint here so the autotuner / handbook can
    /// treat the report deterministically until stage 2 measurements
    /// land.
    notes: &'static str,
}

fn main() -> Result<()> {
    let chip = sysctl_string("machdep.cpu.brand_string").ok();
    let cpu_cores_total = sysctl_u64("hw.ncpu").ok().map(|n| n as u32);
    let unified_memory_bytes = sysctl_u64("hw.memsize").ok();

    let displays = command_output("system_profiler", &["SPDisplaysDataType"]).ok();
    let gpu_cores = displays.as_deref().and_then(parse_gpu_cores);
    let metal_support = displays.as_deref().and_then(parse_metal_support);

    let macos_product_name = command_output("sw_vers", &["-productName"])
        .ok()
        .map(trim_string);
    let macos_product_version = command_output("sw_vers", &["-productVersion"])
        .ok()
        .map(trim_string);
    let macos_build_version = command_output("sw_vers", &["-buildVersion"])
        .ok()
        .map(trim_string);

    let report = ProbeReport {
        chip,
        cpu_cores_total,
        gpu_cores,
        metal_support,
        unified_memory_bytes,
        macos_product_name,
        macos_product_version,
        macos_build_version,
        source: "system_profiler",
        notes: "stage-1 advertised caps only; stage-2 adds measured \
                stream bandwidth + SIMDgroup-matrix throughput",
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("serialize ProbeReport")?
    );
    Ok(())
}

fn sysctl_string(name: &str) -> Result<String> {
    let raw = command_output("sysctl", &["-n", name])?;
    Ok(trim_string(raw))
}

fn sysctl_u64(name: &str) -> Result<u64> {
    let raw = sysctl_string(name)?;
    raw.parse::<u64>()
        .with_context(|| format!("parse {name}={raw}"))
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("spawn `{program} {}`", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "`{program} {}` exited with status {}",
            args.join(" "),
            output.status
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn trim_string(raw: String) -> String {
    raw.trim().to_string()
}

fn parse_gpu_cores(displays: &str) -> Option<u32> {
    // system_profiler prints e.g. "      Total Number of Cores: 10"
    // under the GPU section. Match the first such line.
    for line in displays.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Total Number of Cores:") {
            if let Ok(n) = rest.trim().parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

fn parse_metal_support(displays: &str) -> Option<String> {
    for line in displays.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Metal Support:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}
