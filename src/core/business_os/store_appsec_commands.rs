// Origin: CTOX
// License: Apache-2.0

use super::session::session_user_id;
use super::store::{BusinessCommand, BusinessOsSession};
use anyhow::Context;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn handle_appsec_business_command(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    if command.command_type == "ctox.appsec.app.audit" {
        return handle_appsec_app_audit_command(root, &command.payload);
    }
    let mut args = Vec::new();
    let release_review = command.command_type == "ctox.appsec.release.review";
    if command.command_type != "ctox.appsec.state.sync" {
        push_appsec_state_dir_arg(root, &command.payload, &mut args)?;
    }
    match command.command_type.as_str() {
        "ctox.appsec.state.sync" => {
            args.extend(["state", "sync"].map(str::to_string));
            push_appsec_state_dir_arg(root, &command.payload, &mut args)?;
            args.push("--json".to_string());
        }
        "ctox.appsec.review" => {
            args.extend(["review", "--json"].map(str::to_string));
        }
        "ctox.appsec.release.review" => {
            args.extend(
                ["report", "--format", "json", "--gate", "go-live", "--json"].map(str::to_string),
            );
        }
        "ctox.appsec.tools.doctor" => {
            args.extend(["tools", "doctor"].map(str::to_string));
            if let Some(profile) = appsec_payload_string(&command.payload, "profile") {
                args.extend(["--profile".to_string(), profile]);
            }
            if command
                .payload
                .get("probe_versions")
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--probe-versions".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.graph.build" => {
            args.extend(["graph", "build", "--json"].map(str::to_string));
        }
        "ctox.appsec.investigation.plan" => {
            let id = appsec_payload_string(&command.payload, "investigation_id")
                .or_else(|| appsec_payload_string(&command.payload, "candidate_id"))
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context("ctox.appsec.investigation.plan payload.investigation_id or payload.candidate_id is required")?;
            args.extend(["investigate", "plan", "--id"].map(str::to_string));
            args.push(id);
            for (key, flag) in [
                ("hypothesis", "--hypothesis"),
                ("expected_signal", "--expected-signal"),
                ("falsification_criterion", "--falsification-criterion"),
                ("tool", "--tool"),
                ("url", "--url"),
                ("host", "--host"),
                ("timeout", "--timeout"),
                ("approval_id", "--approval-id"),
            ] {
                push_optional_appsec_string_arg(&command.payload, &mut args, key, flag);
            }
            if let Some(target) = appsec_payload_string(&command.payload, "target") {
                let target = workspace_bound_path(root, &target, "target")?;
                args.extend(["--target".to_string(), target.display().to_string()]);
            }
            if let Some(wordlist) = appsec_payload_string(&command.payload, "wordlist") {
                let wordlist = workspace_bound_path(root, &wordlist, "wordlist")?;
                args.extend(["--wordlist".to_string(), wordlist.display().to_string()]);
            }
            for raw_arg in appsec_payload_string_list(&command.payload, "raw_args") {
                args.extend(["--raw-arg".to_string(), raw_arg]);
            }
            if command.payload.get("active").and_then(Value::as_bool) == Some(true) {
                args.push("--active".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.investigation.execute" => {
            let id = appsec_payload_string(&command.payload, "investigation_id")
                .or_else(|| appsec_payload_string(&command.payload, "candidate_id"))
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context(
                    "ctox.appsec.investigation.execute payload.investigation_id is required",
                )?;
            args.extend(["investigate", "execute", "--id"].map(str::to_string));
            args.push(id);
            if command
                .payload
                .get("confirm_active")
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--confirm-active".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.investigation.resolve" => {
            let id = appsec_payload_string(&command.payload, "investigation_id")
                .or_else(|| appsec_payload_string(&command.payload, "candidate_id"))
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context(
                    "ctox.appsec.investigation.resolve payload.investigation_id is required",
                )?;
            let outcome = appsec_payload_string(&command.payload, "outcome")
                .context("ctox.appsec.investigation.resolve payload.outcome is required")?;
            let reason = appsec_payload_string(&command.payload, "reason")
                .context("ctox.appsec.investigation.resolve payload.reason is required")?;
            let artifact = appsec_payload_string(&command.payload, "artifact")
                .context("ctox.appsec.investigation.resolve payload.artifact is required")?;
            let artifact = workspace_bound_path(root, &artifact, "artifact")?;
            args.extend(["investigate", "resolve", "--id"].map(str::to_string));
            args.extend([
                id,
                "--outcome".to_string(),
                outcome,
                "--reason".to_string(),
                reason,
                "--artifact".to_string(),
                artifact.display().to_string(),
            ]);
            push_optional_appsec_string_arg(
                &command.payload,
                &mut args,
                "duplicate_of",
                "--duplicate-of",
            );
            args.push("--json".to_string());
        }
        "ctox.appsec.investigation.refute" => {
            let id = appsec_payload_string(&command.payload, "investigation_id")
                .or_else(|| appsec_payload_string(&command.payload, "candidate_id"))
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context("ctox.appsec.investigation.refute payload.investigation_id is required")?;
            let artifact = appsec_payload_string(&command.payload, "artifact")
                .context("ctox.appsec.investigation.refute payload.artifact is required")?;
            let artifact = workspace_bound_path(root, &artifact, "artifact")?;
            args.extend(["investigate", "refute", "--id"].map(str::to_string));
            args.extend([
                id,
                "--artifact".to_string(),
                artifact.display().to_string(),
                "--json".to_string(),
            ]);
        }
        "ctox.appsec.replay.baseline" | "ctox.appsec.replay.investigations" => {
            let assessment_id = appsec_payload_string(&command.payload, "assessment_id")
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context("ctox.appsec.replay payload.assessment_id is required")?;
            args.extend(["replay", "--assessment"].map(str::to_string));
            args.push(assessment_id);
            args.push(
                if command.command_type == "ctox.appsec.replay.baseline" {
                    "--baseline"
                } else {
                    "--investigations"
                }
                .to_string(),
            );
            args.push("--json".to_string());
        }
        "ctox.appsec.assessment.create" => {
            push_appsec_assessment_definition_args(root, &command.payload, &mut args, "draft")?;
            args.push("--json".to_string());
        }
        "ctox.appsec.assessment.archive" => {
            push_appsec_assessment_definition_args(root, &command.payload, &mut args, "archived")?;
            args.push("--json".to_string());
        }
        "ctox.appsec.assessment.run" => {
            args.push("assess".to_string());
            let profile = appsec_payload_string(&command.payload, "profile")
                .unwrap_or_else(|| "full".to_string());
            anyhow::ensure!(
                matches!(profile.as_str(), "quick" | "standard" | "deep" | "full"),
                "ctox.appsec.assessment.run payload.profile must be quick, standard, deep, or full"
            );
            args.extend(["--profile".to_string(), profile]);

            let mut target_count = 0usize;
            if let Some(url) = appsec_payload_string(&command.payload, "url") {
                args.extend(["--url".to_string(), url]);
                target_count += 1;
            }
            if let Some(source_path) = appsec_payload_string(&command.payload, "source_path")
                .or_else(|| appsec_payload_string(&command.payload, "source"))
            {
                let source_path = workspace_bound_path(root, &source_path, "source_path")?;
                args.extend(["--target".to_string(), source_path.display().to_string()]);
                target_count += 1;
            }
            anyhow::ensure!(
                target_count > 0,
                "ctox.appsec.assessment.run requires payload.url or payload.source_path"
            );

            if let Some(subjects) = appsec_payload_string(&command.payload, "authz_subjects") {
                let subjects = workspace_bound_path(root, &subjects, "authz_subjects")?;
                args.extend([
                    "--authz-subjects".to_string(),
                    subjects.display().to_string(),
                ]);
            }
            if command
                .payload
                .get("authz_enabled")
                .and_then(Value::as_bool)
                == Some(false)
            {
                args.push("--no-authz".to_string());
            }

            let active = command.payload.get("active").and_then(Value::as_bool) == Some(true);
            if active {
                let approval_id = appsec_payload_string(&command.payload, "approval_id").context(
                    "ctox.appsec.assessment.run active testing requires payload.approval_id",
                )?;
                args.extend([
                    "--active".to_string(),
                    "--confirm-active".to_string(),
                    "--approval-id".to_string(),
                    approval_id,
                ]);
                if let Some(wordlist) = appsec_payload_string(&command.payload, "wordlist") {
                    let wordlist = workspace_bound_path(root, &wordlist, "wordlist")?;
                    args.extend(["--wordlist".to_string(), wordlist.display().to_string()]);
                }
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.audit.run" => {
            args.extend(["audit", "run"].map(str::to_string));
            let profile = appsec_payload_string(&command.payload, "profile")
                .unwrap_or_else(|| "standard".to_string());
            anyhow::ensure!(
                matches!(profile.as_str(), "standard" | "full"),
                "ctox.appsec.audit.run payload.profile must be standard or full"
            );
            args.extend(["--profile".to_string(), profile]);

            let mut target_count = 0usize;
            if let Some(url) = appsec_payload_string(&command.payload, "url") {
                args.extend(["--url".to_string(), url]);
                target_count += 1;
            }
            if let Some(source_path) = appsec_payload_string(&command.payload, "source_path")
                .or_else(|| appsec_payload_string(&command.payload, "source"))
            {
                let source_path = workspace_bound_path(root, &source_path, "source_path")?;
                args.extend(["--source".to_string(), source_path.display().to_string()]);
                target_count += 1;
            }
            anyhow::ensure!(
                target_count > 0,
                "ctox.appsec.audit.run requires payload.url or payload.source"
            );

            let active = command.payload.get("active").and_then(Value::as_bool) == Some(true);
            if active {
                let approval_id = appsec_payload_string(&command.payload, "approval_id")
                    .context("ctox.appsec.audit.run active testing requires payload.approval_id")?;
                args.extend([
                    "--active".to_string(),
                    "--approval-id".to_string(),
                    approval_id,
                ]);
                if let Some(wordlist) = appsec_payload_string(&command.payload, "wordlist") {
                    let wordlist = workspace_bound_path(root, &wordlist, "wordlist")?;
                    args.extend(["--wordlist".to_string(), wordlist.display().to_string()]);
                }
            }
            if let Some(timeout) = appsec_payload_u64(&command.payload, "timeout_seconds") {
                args.extend(["--timeout".to_string(), timeout.to_string()]);
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.exploit.list" => {
            return appsec_exploit_list(root, &command.payload);
        }
        "ctox.appsec.exploit.get" => {
            return appsec_exploit_get(root, &command.payload);
        }
        "ctox.appsec.exploit.verify" => {
            return appsec_exploit_verify(root, &command.payload);
        }
        "ctox.appsec.lab.create" => {
            args.extend(["lab", "create"].map(str::to_string));
            if let Some(out) = appsec_payload_string(&command.payload, "out") {
                let out_path = workspace_bound_path(root, &out, "out")?;
                args.extend(["--out".to_string(), out_path.display().to_string()]);
            }
            if command
                .payload
                .get("allow_incomplete")
                .or_else(|| command.payload.get("allow-incomplete"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--allow-incomplete".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.lab.run" => {
            let url = appsec_payload_string(&command.payload, "url")
                .or_else(|| appsec_payload_string(&command.payload, "target"))
                .context("ctox.appsec.lab.run payload.url is required")?;
            args.extend([
                "lab".to_string(),
                "run".to_string(),
                "--url".to_string(),
                url,
            ]);
            push_optional_appsec_string_arg(&command.payload, &mut args, "profile", "--profile");
            if command
                .payload
                .get("rebuild_coverage")
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--rebuild-coverage".to_string());
            }
            if command.payload.get("report").and_then(Value::as_bool) == Some(false) {
                args.push("--no-report".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.report.export" => {
            let format = appsec_payload_string(&command.payload, "format")
                .unwrap_or_else(|| "markdown".to_string());
            anyhow::ensure!(
                matches!(format.as_str(), "markdown" | "md" | "json"),
                "ctox.appsec.report.export payload.format must be markdown, md, or json"
            );
            args.extend(["report".to_string(), "--format".to_string(), format]);
            if let Some(out) = appsec_payload_string(&command.payload, "out") {
                let out_path = workspace_bound_path(root, &out, "out")?;
                args.extend(["--out".to_string(), out_path.display().to_string()]);
            }
            if command
                .payload
                .get("allow_incomplete")
                .or_else(|| command.payload.get("allow-incomplete"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--allow-incomplete".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.authz.plan" => {
            let target = appsec_payload_string(&command.payload, "target")
                .or_else(|| appsec_payload_string(&command.payload, "url"))
                .context("ctox.appsec.authz.plan payload.target is required")?;
            args.extend([
                "authz".to_string(),
                "plan".to_string(),
                "--target".to_string(),
                target,
            ]);
            if let Some(source_id) = appsec_payload_string(&command.payload, "source_id") {
                args.extend(["--source-id".to_string(), source_id]);
            }
            if let Some(subjects) = appsec_payload_string(&command.payload, "subjects") {
                let subjects_path = workspace_bound_path(root, &subjects, "subjects")?;
                args.extend([
                    "--subjects".to_string(),
                    subjects_path.display().to_string(),
                ]);
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.authz.credential_proof_template" => {
            let subjects = appsec_payload_string(&command.payload, "subjects")
                .or_else(|| appsec_payload_string(&command.payload, "subjects_file"))
                .or_else(|| appsec_payload_string(&command.payload, "subjects-file"))
                .context(
                    "ctox.appsec.authz.credential_proof_template payload.subjects is required",
                )?;
            let subjects_path = workspace_bound_path(root, &subjects, "subjects")?;
            args.extend([
                "authz".to_string(),
                "credential-proof-template".to_string(),
                "--subjects".to_string(),
                subjects_path.display().to_string(),
            ]);
            if let Some(out) = appsec_payload_string(&command.payload, "out")
                .or_else(|| appsec_payload_string(&command.payload, "output"))
            {
                let out_path = workspace_bound_path(root, &out, "out")?;
                args.extend(["--out".to_string(), out_path.display().to_string()]);
            }
            if command.payload.get("force").and_then(Value::as_bool) == Some(true) {
                args.push("--force".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.authz.credential_proof_from_evidence" => {
            let run = appsec_payload_string(&command.payload, "run")
                .or_else(|| appsec_payload_string(&command.payload, "run_artifact"))
                .or_else(|| appsec_payload_string(&command.payload, "run-artifact"))
                .context(
                    "ctox.appsec.authz.credential_proof_from_evidence payload.run is required",
                )?;
            let evidence_dir = appsec_payload_string(&command.payload, "evidence_dir")
                .or_else(|| appsec_payload_string(&command.payload, "evidence-dir"))
                .context("ctox.appsec.authz.credential_proof_from_evidence payload.evidence_dir is required")?;
            let run_path = workspace_bound_path(root, &run, "run")?;
            let evidence_dir = workspace_bound_path(root, &evidence_dir, "evidence_dir")?;
            args.extend([
                "authz".to_string(),
                "credential-proof-from-evidence".to_string(),
                "--run".to_string(),
                run_path.display().to_string(),
                "--evidence-dir".to_string(),
                evidence_dir.display().to_string(),
            ]);
            if let Some(base_proof) = appsec_payload_string(&command.payload, "base_proof")
                .or_else(|| appsec_payload_string(&command.payload, "base-proof"))
                .or_else(|| appsec_payload_string(&command.payload, "credential_proof"))
                .or_else(|| appsec_payload_string(&command.payload, "credential-proof"))
            {
                let proof_path = workspace_bound_path(root, &base_proof, "base_proof")?;
                args.extend(["--base-proof".to_string(), proof_path.display().to_string()]);
            }
            if let Some(out) = appsec_payload_string(&command.payload, "out")
                .or_else(|| appsec_payload_string(&command.payload, "output"))
            {
                let out_path = workspace_bound_path(root, &out, "out")?;
                args.extend(["--out".to_string(), out_path.display().to_string()]);
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.authz.status" => {
            args.extend(["authz", "status", "--json"].map(str::to_string));
        }
        "ctox.appsec.authz.preflight" => {
            let target = appsec_payload_string(&command.payload, "target")
                .or_else(|| appsec_payload_string(&command.payload, "url"))
                .context("ctox.appsec.authz.preflight payload.target is required")?;
            args.extend([
                "authz".to_string(),
                "preflight".to_string(),
                "--target".to_string(),
                target,
            ]);
            if let Some(source_id) = appsec_payload_string(&command.payload, "source_id") {
                args.extend(["--source-id".to_string(), source_id]);
            }
            if let Some(subjects) = appsec_payload_string(&command.payload, "subjects")
                .or_else(|| appsec_payload_string(&command.payload, "subjects_file"))
                .or_else(|| appsec_payload_string(&command.payload, "subjects-file"))
            {
                let subjects_path = workspace_bound_path(root, &subjects, "subjects")?;
                args.extend([
                    "--subjects".to_string(),
                    subjects_path.display().to_string(),
                ]);
            }
            if let Some(run) = appsec_payload_string(&command.payload, "run")
                .or_else(|| appsec_payload_string(&command.payload, "run_artifact"))
                .or_else(|| appsec_payload_string(&command.payload, "run-artifact"))
            {
                let run_path = workspace_bound_path(root, &run, "run")?;
                args.extend(["--run".to_string(), run_path.display().to_string()]);
            }
            if let Some(evidence_dir) = appsec_payload_string(&command.payload, "evidence_dir")
                .or_else(|| appsec_payload_string(&command.payload, "evidence-dir"))
            {
                let evidence_dir = workspace_bound_path(root, &evidence_dir, "evidence_dir")?;
                args.extend([
                    "--evidence-dir".to_string(),
                    evidence_dir.display().to_string(),
                ]);
            }
            if let Some(credential_proof) =
                appsec_payload_string(&command.payload, "credential_proof")
                    .or_else(|| appsec_payload_string(&command.payload, "credential-proof"))
            {
                let proof_path = workspace_bound_path(root, &credential_proof, "credential_proof")?;
                args.extend([
                    "--credential-proof".to_string(),
                    proof_path.display().to_string(),
                ]);
            }
            if command
                .payload
                .get("require_credentials")
                .or_else(|| command.payload.get("require-credentials"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--require-credentials".to_string());
            }
            if command
                .payload
                .get("require_login_proof")
                .or_else(|| command.payload.get("require-login-proof"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--require-login-proof".to_string());
            }
            if command
                .payload
                .get("require_evidence")
                .or_else(|| command.payload.get("require-evidence"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--require-evidence".to_string());
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.authz.run" => {
            let target = appsec_payload_string(&command.payload, "target")
                .or_else(|| appsec_payload_string(&command.payload, "url"))
                .context("ctox.appsec.authz.run payload.target is required")?;
            let subjects = appsec_payload_string(&command.payload, "subjects")
                .or_else(|| appsec_payload_string(&command.payload, "subjects_file"))
                .or_else(|| appsec_payload_string(&command.payload, "subjects-file"))
                .context("ctox.appsec.authz.run payload.subjects is required")?;
            let subjects_path = workspace_bound_path(root, &subjects, "subjects")?;
            args.extend([
                "authz".to_string(),
                "run".to_string(),
                "--target".to_string(),
                target,
                "--subjects".to_string(),
                subjects_path.display().to_string(),
            ]);
            if let Some(source_id) = appsec_payload_string(&command.payload, "source_id") {
                args.extend(["--source-id".to_string(), source_id]);
            }
            if let Some(credential_proof) =
                appsec_payload_string(&command.payload, "credential_proof")
                    .or_else(|| appsec_payload_string(&command.payload, "credential-proof"))
            {
                let proof_path = workspace_bound_path(root, &credential_proof, "credential_proof")?;
                args.extend([
                    "--credential-proof".to_string(),
                    proof_path.display().to_string(),
                ]);
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.authz.build_matrix" => {
            let run = appsec_payload_string(&command.payload, "run")
                .or_else(|| appsec_payload_string(&command.payload, "run_artifact"))
                .or_else(|| appsec_payload_string(&command.payload, "run-artifact"))
                .context("ctox.appsec.authz.build_matrix payload.run is required")?;
            let evidence_dir = appsec_payload_string(&command.payload, "evidence_dir")
                .or_else(|| appsec_payload_string(&command.payload, "evidence-dir"))
                .context("ctox.appsec.authz.build_matrix payload.evidence_dir is required")?;
            let run_path = workspace_bound_path(root, &run, "run")?;
            let evidence_dir = workspace_bound_path(root, &evidence_dir, "evidence_dir")?;
            args.extend([
                "authz".to_string(),
                "build-matrix".to_string(),
                "--run".to_string(),
                run_path.display().to_string(),
                "--evidence-dir".to_string(),
                evidence_dir.display().to_string(),
            ]);
            if command
                .payload
                .get("import")
                .or_else(|| command.payload.get("import_matrix"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--import".to_string());
            }
            if command
                .payload
                .get("no_mark_coverage")
                .or_else(|| command.payload.get("no-mark-coverage"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--no-mark-coverage".to_string());
            }
            if let Some(out) = appsec_payload_string(&command.payload, "out") {
                let out_path = workspace_bound_path(root, &out, "out")?;
                args.extend(["--out".to_string(), out_path.display().to_string()]);
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.pipeline.rework" => {
            args.extend(["pipeline", "rework"].map(str::to_string));
            if let Some(stage_id) = appsec_payload_string(&command.payload, "stage_id")
                .or_else(|| appsec_payload_string(&command.payload, "stage-id"))
            {
                args.extend(["--stage-id".to_string(), stage_id]);
            } else if let Some(phase) = appsec_payload_string(&command.payload, "phase") {
                args.extend(["--phase".to_string(), phase]);
            } else {
                anyhow::bail!(
                    "ctox.appsec.pipeline.rework payload.stage_id or payload.phase is required"
                );
            }
            push_optional_appsec_string_arg(&command.payload, &mut args, "target", "--target");
            push_optional_appsec_string_arg(&command.payload, &mut args, "status", "--status");
            let reason = appsec_payload_string(&command.payload, "reason")
                .or_else(|| appsec_payload_string(&command.payload, "note"))
                .context("ctox.appsec.pipeline.rework payload.reason is required")?;
            args.extend(["--reason".to_string(), reason]);
            let operator = session_user_id(session)
                .map(str::to_string)
                .unwrap_or_else(|| "business-os-operator".to_string());
            args.extend(["--operator".to_string(), operator]);
            let mut artifacts = appsec_payload_string_list(&command.payload, "artifacts");
            if let Some(artifact) = appsec_payload_string(&command.payload, "artifact") {
                artifacts.push(artifact);
            }
            anyhow::ensure!(
                !artifacts.is_empty(),
                "ctox.appsec.pipeline.rework payload.artifact or payload.artifacts is required"
            );
            for artifact in artifacts {
                let artifact_path = workspace_bound_path(root, &artifact, "artifact")?;
                args.extend([
                    "--artifact".to_string(),
                    artifact_path.display().to_string(),
                ]);
            }
            args.push("--json".to_string());
        }
        "ctox.appsec.approval.request" => {
            args.extend(["approval", "request"].map(str::to_string));
            push_appsec_approval_target_args(&command.payload, &mut args)?;
            let tools = command
                .payload
                .get("tools")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|items| !items.is_empty())
                .or_else(|| appsec_payload_string(&command.payload, "tool").map(|tool| vec![tool]))
                .unwrap_or_else(|| vec!["*".to_string()]);
            for tool in tools {
                args.extend(["--tool".to_string(), tool]);
            }
            push_optional_appsec_string_arg(&command.payload, &mut args, "profile", "--profile");
            push_optional_appsec_string_arg(&command.payload, &mut args, "reason", "--reason");
            push_optional_appsec_string_arg(
                &command.payload,
                &mut args,
                "expires_at",
                "--expires-at",
            );
            push_optional_appsec_string_arg(
                &command.payload,
                &mut args,
                "review_mode",
                "--review-mode",
            );
            if let Some(required_approvers) =
                appsec_payload_u64(&command.payload, "required_approvers")
            {
                args.extend([
                    "--required-approvers".to_string(),
                    required_approvers.to_string(),
                ]);
            }
            if let Some(max_rate) =
                appsec_payload_u64(&command.payload, "max_request_rate_per_second")
            {
                args.extend(["--max-rate".to_string(), max_rate.to_string()]);
            }
            if let Some(max_duration) = appsec_payload_u64(&command.payload, "max_duration_seconds")
            {
                args.extend(["--max-duration".to_string(), max_duration.to_string()]);
            }
            if command
                .payload
                .get("destructive_actions_allowed")
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--allow-destructive".to_string());
            }
            if command
                .payload
                .get("production_safe_profile")
                .and_then(Value::as_bool)
                == Some(false)
            {
                args.push("--production-unsafe".to_string());
            }
            let requested_by = session_user_id(session)
                .map(str::to_string)
                .unwrap_or_else(|| "business-os-operator".to_string());
            args.extend(["--requested-by".to_string(), requested_by]);
            args.push("--json".to_string());
        }
        "ctox.appsec.approval.grant" => {
            let approval_id = appsec_payload_string(&command.payload, "approval_id")
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context("ctox.appsec.approval.grant payload.approval_id is required")?;
            args.extend([
                "approval".to_string(),
                "grant".to_string(),
                "--id".to_string(),
                approval_id,
            ]);
            let approver = session_user_id(session)
                .map(str::to_string)
                .unwrap_or_else(|| "business-os-operator".to_string());
            args.extend(["--approver".to_string(), approver]);
            push_optional_appsec_string_arg(&command.payload, &mut args, "reason", "--reason");
            push_optional_appsec_string_arg(
                &command.payload,
                &mut args,
                "expires_at",
                "--expires-at",
            );
            if command
                .payload
                .get("high_impact_ack")
                .and_then(Value::as_bool)
                == Some(true)
            {
                args.push("--high-impact-ack".to_string());
            }
            push_optional_appsec_string_arg(
                &command.payload,
                &mut args,
                "review_note",
                "--review-note",
            );
            args.push("--json".to_string());
        }
        "ctox.appsec.approval.revoke" => {
            let approval_id = appsec_payload_string(&command.payload, "approval_id")
                .or_else(|| appsec_payload_string(&command.payload, "id"))
                .context("ctox.appsec.approval.revoke payload.approval_id is required")?;
            args.extend([
                "approval".to_string(),
                "revoke".to_string(),
                "--id".to_string(),
                approval_id,
            ]);
            push_optional_appsec_string_arg(&command.payload, &mut args, "reason", "--reason");
            args.push("--json".to_string());
        }
        other => anyhow::bail!("unsupported AppSec Business OS command type: {other}"),
    }
    let outcome = crate::run_projected_appsec_command(root, &args)?;
    if release_review {
        return appsec_release_review_result(outcome);
    }
    Ok(outcome)
}

fn appsec_release_review_result(report: Value) -> anyhow::Result<Value> {
    let completion_review = report
        .get("completion_review")
        .cloned()
        .context("native AppSec release review is missing completion evidence")?;
    let release_decision = report.get("release_decision").cloned();
    anyhow::ensure!(
        completion_review.is_object(),
        "native AppSec release review completion evidence is not an object"
    );
    let blockers = completion_review
        .get("blockers")
        .and_then(Value::as_array)
        .cloned()
        .context("native AppSec release review completion blockers are unreadable")?;
    let blocker_count = completion_review
        .get("blocker_count")
        .and_then(Value::as_u64)
        .context("native AppSec release review completion blocker count is unreadable")?;
    let completion_closable = completion_review
        .get("closable")
        .and_then(Value::as_bool)
        .context("native AppSec release review completion decision is unreadable")?;
    anyhow::ensure!(
        blocker_count == blockers.len() as u64,
        "native AppSec release review completion blocker count does not match its blocker list"
    );
    anyhow::ensure!(
        completion_closable == blockers.is_empty(),
        "native AppSec release review completion decision contradicts its blockers"
    );
    let report_complete = report
        .get("report_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let release_approved = report.get("ok").and_then(Value::as_bool) == Some(true)
        && report_complete
        && completion_closable
        && blocker_count == 0
        && blockers.is_empty()
        && release_decision
            .as_ref()
            .and_then(|value| value.get("decision"))
            .and_then(Value::as_str)
            == Some("ready")
        && release_decision
            .as_ref()
            .and_then(|value| value.get("go_live_approved"))
            .and_then(Value::as_bool)
            == Some(true);
    let reason = release_decision
        .as_ref()
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str)
        .or_else(|| report.get("error").and_then(Value::as_str))
        .unwrap_or("required release evidence is incomplete")
        .to_string();
    Ok(serde_json::json!({
        "version": "ctox.business_os.appsec_release_review.v1",
        "ok": true,
        "command": "ctox.appsec.release.review",
        "status": if release_approved { "ready" } else { "blocked" },
        "release_approved": release_approved,
        "report_complete": report_complete,
        "reason": reason,
        "blocker_count": blocker_count,
        "blockers": blockers,
        "release_decision": release_decision,
        "completion_review": completion_review,
        "report": report,
    }))
}

fn handle_appsec_app_audit_command(root: &Path, payload: &Value) -> anyhow::Result<Value> {
    let module_id = appsec_payload_string(payload, "module_id")
        .context("ctox.appsec.app.audit payload.module_id is required")?;
    let profile = appsec_payload_string(payload, "profile").unwrap_or_else(|| "release".into());
    anyhow::ensure!(
        matches!(profile.as_str(), "quick" | "release" | "full"),
        "ctox.appsec.app.audit payload.profile must be quick, release, or full"
    );
    let mode = appsec_payload_string(payload, "mode").unwrap_or_else(|| "installed".into());
    anyhow::ensure!(
        matches!(mode.as_str(), "installed" | "source"),
        "ctox.appsec.app.audit payload.mode must be installed or source"
    );

    // Resolve the source server-side from module_id + mode. Business OS
    // callers cannot inject a filesystem target or turn the shell hash route
    // into an HTTP scanner target.
    let mut args = vec![format!("--{mode}"), "--profile".into(), profile];
    for (key, flag) in [
        ("shell_url", "--url"),
        ("deployed_url", "--deployed-url"),
        ("approval_id", "--approval-id"),
    ] {
        if let Some(value) = appsec_payload_string(payload, key) {
            args.extend([flag.to_string(), value]);
        }
    }
    if payload.get("active").and_then(Value::as_bool) == Some(true) {
        args.push("--active".into());
    }
    crate::service::business_os_app_testing::run_business_os_app_audit(root, &module_id, &args)
}

fn push_appsec_state_dir_arg(
    root: &Path,
    payload: &Value,
    args: &mut Vec<String>,
) -> anyhow::Result<()> {
    if let Some(state_dir) = appsec_payload_string(payload, "state_dir") {
        let state_dir = workspace_bound_path(root, &state_dir, "state_dir")?;
        args.extend(["--state-dir".to_string(), state_dir.display().to_string()]);
    }
    Ok(())
}

fn appsec_exploit_state_dir(root: &Path, payload: &Value) -> anyhow::Result<PathBuf> {
    if let Some(state_dir) = appsec_payload_string(payload, "state_dir") {
        return workspace_bound_path(root, &state_dir, "state_dir");
    }
    Ok(root.join("runtime/appsec/default"))
}

/// DataRead: list the verified exploit index entries of the bound state dir.
fn appsec_exploit_list(root: &Path, payload: &Value) -> anyhow::Result<Value> {
    let state_dir = appsec_exploit_state_dir(root, payload)?;
    let index_path = state_dir.join("exploits").join("index.json");
    let index = if index_path.is_file() {
        let raw = fs::read_to_string(&index_path)
            .with_context(|| format!("failed to read {}", index_path.display()))?;
        serde_json::from_str::<Value>(&raw)
            .with_context(|| format!("invalid JSON in {}", index_path.display()))?
    } else {
        return Ok(serde_json::json!({
            "ok": true,
            "command": "ctox.appsec.exploit.list",
            "state_dir": state_dir.display().to_string(),
            "exploits": [],
            "note": "no exploit index present for this state dir; run ctox.appsec.audit.run first",
        }));
    };
    let exploits = index
        .get("exploits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "finding_id": entry.get("finding_id").cloned().unwrap_or(Value::Null),
                "title": entry.get("title").cloned().unwrap_or(Value::Null),
                "severity": entry.get("severity").cloned().unwrap_or(Value::Null),
                "cwe": entry.get("cwe").cloned().unwrap_or(Value::Null),
                "cvss_score": entry.get("cvss_score").cloned().unwrap_or(Value::Null),
                "target": entry.get("target").cloned().unwrap_or(Value::Null),
                "verification_status": entry.pointer("/verification/status").cloned().unwrap_or(Value::Null),
                "origin": entry.get("origin").cloned().unwrap_or(Value::Null),
                "craft": entry.get("craft").cloned().unwrap_or(Value::Null),
                "script_name": entry
                    .get("script")
                    .and_then(Value::as_str)
                    .and_then(|script| Path::new(script).file_name().map(|name| name.to_string_lossy().to_string())),
                "script_sha256": entry.get("script_sha256").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "ok": true,
        "command": "ctox.appsec.exploit.list",
        "state_dir": state_dir.display().to_string(),
        "version": index.get("version").cloned().unwrap_or(Value::Null),
        "generated_at": index.get("generated_at").cloned().unwrap_or(Value::Null),
        "exploits": exploits,
    }))
}

/// DataRead: return the text of ONE exploit script from the bound state
/// dir's exploits/ directory. Path safety: plain `.py` file names only, the
/// canonicalized path must stay inside the canonicalized exploits directory.
fn appsec_exploit_get(root: &Path, payload: &Value) -> anyhow::Result<Value> {
    let state_dir = appsec_exploit_state_dir(root, payload)?;
    let name = appsec_payload_string(payload, "name")
        .or_else(|| appsec_payload_string(payload, "script_name"))
        .or_else(|| appsec_payload_string(payload, "script"))
        .context("ctox.appsec.exploit.get payload.name is required")?;
    let name_path = Path::new(&name);
    anyhow::ensure!(
        name_path.components().count() == 1
            && matches!(name_path.components().next(), Some(Component::Normal(_)))
            && name.ends_with(".py"),
        "ctox.appsec.exploit.get payload.name must be a plain .py file name inside the exploits directory"
    );
    let exploits_dir = state_dir.join("exploits");
    let canonical_dir = fs::canonicalize(&exploits_dir).with_context(|| {
        format!(
            "no exploits directory for this state dir: {}",
            exploits_dir.display()
        )
    })?;
    let canonical_path = fs::canonicalize(exploits_dir.join(&name))
        .with_context(|| format!("exploit script not found: {name}"))?;
    anyhow::ensure!(
        canonical_path.starts_with(&canonical_dir),
        "ctox.appsec.exploit.get payload.name must stay inside the exploits directory"
    );
    let bytes = fs::read(&canonical_path)
        .with_context(|| format!("failed to read {}", canonical_path.display()))?;
    let content = String::from_utf8(bytes)
        .with_context(|| format!("exploit script is not UTF-8 text: {name}"))?;
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    Ok(serde_json::json!({
        "ok": true,
        "command": "ctox.appsec.exploit.get",
        "state_dir": state_dir.display().to_string(),
        "name": name,
        "sha256": format!("{:x}", hasher.finalize()),
        "size_bytes": content.len(),
        "content": content,
    }))
}

/// Build the shared CLI args for one `exploit verify` run. The caller has
/// already validated `expect` and the confirm/approval gate.
fn appsec_exploit_verify_cli_args(
    root: &Path,
    payload: &Value,
    finding_id: &str,
) -> anyhow::Result<Vec<String>> {
    let mut args = Vec::new();
    push_appsec_state_dir_arg(root, payload, &mut args)?;
    args.extend(["exploit", "verify", "--id", finding_id, "--execute"].map(str::to_string));
    if let Some(expect) = appsec_payload_string(payload, "expect") {
        args.extend(["--expect".to_string(), expect]);
    }
    if payload.get("confirm_active").and_then(Value::as_bool) == Some(true) {
        args.push("--confirm-active".to_string());
    }
    if payload.get("confirm_non_get").and_then(Value::as_bool) == Some(true) {
        args.push("--confirm-non-get".to_string());
    }
    if let Some(timeout) = appsec_payload_u64(payload, "timeout_seconds") {
        args.extend(["--timeout".to_string(), timeout.to_string()]);
    }
    args.push("--json".to_string());
    Ok(args)
}

/// Map one projected `exploit verify` CLI result onto the compact business
/// entry shape. Returns the verification status plus the JSON entry.
fn appsec_exploit_verify_entry(
    finding_id: &str,
    script_name: &str,
    output: &Value,
) -> (String, Value) {
    let status = output
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("execution-failed")
        .to_string();
    let exit_code = output
        .pointer("/verification_record/exit_code")
        .cloned()
        .unwrap_or(Value::Null);
    let mut entry = serde_json::json!({
        "finding_id": finding_id,
        "script_name": script_name,
        "status": status,
        "exit_code": exit_code,
    });
    if let Some(expectation) = output.get("expectation").filter(|value| !value.is_null()) {
        entry["expectation"] = serde_json::json!({
            "expected": expectation.get("expected").cloned().unwrap_or(Value::Null),
            "met": expectation.get("met").cloned().unwrap_or(Value::Null),
        });
    }
    if let Some(error) = output.get("error").and_then(Value::as_str) {
        entry["error"] = Value::String(error.to_string());
    }
    (status, entry)
}

/// DataWrite: re-execute ONE exploit (`payload.name` = script_name from
/// ctox.appsec.exploit.list) or ALL exploits of the bound state dir's index
/// and report the verification status (still vulnerable vs fixed). Execution
/// goes through the projected command machinery like ctox.appsec.audit.run.
fn appsec_exploit_verify(root: &Path, payload: &Value) -> anyhow::Result<Value> {
    // Validate everything before any CLI execution.
    if let Some(expect) = appsec_payload_string(payload, "expect") {
        anyhow::ensure!(
            matches!(expect.as_str(), "vulnerable" | "fixed"),
            "ctox.appsec.exploit.verify payload.expect must be vulnerable or fixed"
        );
    }
    let confirm_active = payload.get("confirm_active").and_then(Value::as_bool) == Some(true);
    let confirm_non_get = payload.get("confirm_non_get").and_then(Value::as_bool) == Some(true);
    if confirm_active || confirm_non_get {
        appsec_payload_string(payload, "approval_id").context(
            "ctox.appsec.exploit.verify confirm_active/confirm_non_get requires payload.approval_id",
        )?;
    }

    let state_dir = appsec_exploit_state_dir(root, payload)?;
    let index_path = state_dir.join("exploits").join("index.json");
    let index = if index_path.is_file() {
        let raw = fs::read_to_string(&index_path)
            .with_context(|| format!("failed to read {}", index_path.display()))?;
        serde_json::from_str::<Value>(&raw)
            .with_context(|| format!("invalid JSON in {}", index_path.display()))?
    } else {
        Value::Null
    };
    let entries = index
        .get("exploits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let script_name_of = |entry: &Value| {
        entry
            .get("script")
            .and_then(Value::as_str)
            .and_then(|script| {
                Path::new(script)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
    };

    if let Some(name) = appsec_payload_string(payload, "name") {
        let name_path = Path::new(&name);
        anyhow::ensure!(
            name_path.components().count() == 1
                && matches!(name_path.components().next(), Some(Component::Normal(_)))
                && name.ends_with(".py"),
            "ctox.appsec.exploit.verify payload.name must be a plain .py file name inside the exploits directory"
        );
        let entry = entries
            .iter()
            .find(|entry| script_name_of(entry).as_deref() == Some(name.as_str()))
            .ok_or_else(|| {
                anyhow::anyhow!("ctox.appsec.exploit.verify unknown exploit script name: {name}")
            })?;
        let finding_id = entry
            .get("finding_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "ctox.appsec.exploit.verify index entry for {name} has no finding_id"
                )
            })?;
        let args = appsec_exploit_verify_cli_args(root, payload, finding_id)?;
        let output = crate::run_projected_appsec_command(root, &args)?;
        if output.get("status").and_then(Value::as_str).is_none() {
            let error = output
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("exploit verify did not produce a verification record");
            anyhow::bail!("ctox.appsec.exploit.verify failed for {name}: {error}");
        }
        let (status, entry) = appsec_exploit_verify_entry(finding_id, &name, &output);
        // ok means the proof script executed and produced a usable verdict;
        // the verdict itself (still vulnerable vs fixed) lives in status.
        let executed = matches!(
            status.as_str(),
            "still-reproduces" | "fixed-or-not-reproducible" | "setup-or-inconclusive"
        );
        let mut result = serde_json::json!({
            "ok": executed,
            "command": "ctox.appsec.exploit.verify",
            "state_dir": state_dir.display().to_string(),
            "finding_id": finding_id,
            "script_name": name,
            "status": status,
            "exit_code": entry.get("exit_code").cloned().unwrap_or(Value::Null),
            "duration_ms": output
                .pointer("/verification_record/duration_ms")
                .cloned()
                .unwrap_or(Value::Null),
        });
        if let Some(expectation) = entry.get("expectation") {
            result["expectation"] = expectation.clone();
        }
        return Ok(result);
    }

    // No name: verify every exploit of the index.
    if entries.is_empty() {
        return Ok(serde_json::json!({
            "ok": true,
            "command": "ctox.appsec.exploit.verify",
            "state_dir": state_dir.display().to_string(),
            "results": [],
            "summary": {
                "verified": 0,
                "still_reproduces": 0,
                "fixed": 0,
                "inconclusive": 0,
                "failed": 0,
            },
            "note": "no exploit index present for this state dir; run ctox.appsec.audit.run first",
        }));
    }
    let mut results = Vec::new();
    let mut still_reproduces = 0u64;
    let mut fixed = 0u64;
    let mut inconclusive = 0u64;
    let mut failed = 0u64;
    for entry in &entries {
        let finding_id = entry
            .get("finding_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let script_name = script_name_of(entry).unwrap_or_default();
        let verified_entry = if finding_id.is_empty() {
            let mut item = serde_json::json!({
                "finding_id": Value::Null,
                "script_name": script_name,
                "status": "execution-failed",
                "exit_code": Value::Null,
            });
            item["error"] = Value::String("index entry has no finding_id".to_string());
            item
        } else {
            match appsec_exploit_verify_cli_args(root, payload, &finding_id)
                .and_then(|args| crate::run_projected_appsec_command(root, &args))
            {
                Ok(output) => appsec_exploit_verify_entry(&finding_id, &script_name, &output).1,
                Err(err) => {
                    let mut item = serde_json::json!({
                        "finding_id": finding_id,
                        "script_name": script_name,
                        "status": "execution-failed",
                        "exit_code": Value::Null,
                    });
                    item["error"] = Value::String(format!("{err:#}"));
                    item
                }
            }
        };
        match verified_entry.get("status").and_then(Value::as_str) {
            Some("still-reproduces") => still_reproduces += 1,
            Some("fixed-or-not-reproducible") => fixed += 1,
            Some("setup-or-inconclusive") => inconclusive += 1,
            _ => failed += 1,
        }
        results.push(verified_entry);
    }
    let verified = results.len() as u64;
    Ok(serde_json::json!({
        "ok": true,
        "command": "ctox.appsec.exploit.verify",
        "state_dir": state_dir.display().to_string(),
        "results": results,
        "summary": {
            "verified": verified,
            "still_reproduces": still_reproduces,
            "fixed": fixed,
            "inconclusive": inconclusive,
            "failed": failed,
        },
    }))
}

fn push_appsec_assessment_definition_args(
    root: &Path,
    payload: &Value,
    args: &mut Vec<String>,
    status: &str,
) -> anyhow::Result<()> {
    args.push("init".to_string());
    let profile = appsec_payload_string(payload, "profile").unwrap_or_else(|| "full".to_string());
    anyhow::ensure!(
        matches!(profile.as_str(), "quick" | "standard" | "deep" | "full"),
        "AppSec assessment payload.profile must be quick, standard, deep, or full"
    );
    args.extend(["--profile".to_string(), profile]);
    args.extend(["--status".to_string(), status.to_string()]);
    if let Some(name) = appsec_payload_string(payload, "name") {
        args.extend(["--name".to_string(), name]);
    }

    let mut target_count = 0usize;
    if let Some(url) =
        appsec_payload_string(payload, "url").or_else(|| appsec_payload_string(payload, "target"))
    {
        args.extend(["--url".to_string(), url]);
        target_count += 1;
    }
    if let Some(source_path) = appsec_payload_string(payload, "source_path")
        .or_else(|| appsec_payload_string(payload, "source"))
    {
        let source_path = workspace_bound_path(root, &source_path, "source_path")?;
        args.extend(["--target".to_string(), source_path.display().to_string()]);
        target_count += 1;
    }
    anyhow::ensure!(
        target_count > 0,
        "AppSec assessment definition requires payload.url or payload.source_path"
    );

    if let Some(subjects) = appsec_payload_string(payload, "authz_subjects") {
        let subjects = workspace_bound_path(root, &subjects, "authz_subjects")?;
        args.extend([
            "--authz-subjects".to_string(),
            subjects.display().to_string(),
        ]);
    }
    if payload.get("active").and_then(Value::as_bool) == Some(true) {
        args.push("--active".to_string());
    }
    if let Some(approval_id) = appsec_payload_string(payload, "approval_id") {
        args.extend(["--approval-id".to_string(), approval_id]);
    }
    if let Some(wordlist) = appsec_payload_string(payload, "wordlist") {
        let wordlist = workspace_bound_path(root, &wordlist, "wordlist")?;
        args.extend(["--wordlist".to_string(), wordlist.display().to_string()]);
    }
    Ok(())
}

fn push_appsec_approval_target_args(payload: &Value, args: &mut Vec<String>) -> anyhow::Result<()> {
    if let Some(url) = appsec_payload_string(payload, "url") {
        args.extend(["--url".to_string(), url]);
        return Ok(());
    }
    if let Some(host) = appsec_payload_string(payload, "host") {
        args.extend(["--host".to_string(), host]);
        return Ok(());
    }
    if let Some(target) = appsec_payload_string(payload, "target") {
        args.extend(["--target".to_string(), target]);
        return Ok(());
    }
    anyhow::bail!("AppSec approval request requires payload.url, payload.host, or payload.target")
}

fn push_optional_appsec_string_arg(payload: &Value, args: &mut Vec<String>, key: &str, flag: &str) {
    if let Some(value) = appsec_payload_string(payload, key) {
        args.extend([flag.to_string(), value]);
    }
}

fn appsec_payload_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn appsec_payload_string_list(payload: &Value, key: &str) -> Vec<String> {
    match payload.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        }
        _ => Vec::new(),
    }
}

fn appsec_payload_u64(payload: &Value, key: &str) -> Option<u64> {
    payload
        .get(key)
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}

fn workspace_bound_path(root: &Path, value: &str, field: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(value.trim());
    anyhow::ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::ParentDir)),
        "{field} must not contain parent-directory components"
    );
    let resolved = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    anyhow::ensure!(
        resolved.starts_with(root),
        "{field} must stay inside the CTOX workspace"
    );
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::chef_session;
    use super::super::store::{
        appsec_business_command_requires_data_write, BusinessCommand, CommandOrigin,
        APPSEC_MODULE_ID,
    };
    use super::{appsec_release_review_result, handle_appsec_business_command};
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[test]
    fn native_release_review_approves_only_complete_go_live_evidence() -> anyhow::Result<()> {
        assert!(appsec_business_command_requires_data_write(
            "ctox.appsec.release.review"
        ));
        let result = appsec_release_review_result(serde_json::json!({
            "ok": true,
            "report_complete": true,
            "completion_review": {
                "closable": true,
                "blocker_count": 0,
                "blockers": []
            },
            "release_decision": {
                "decision": "ready",
                "go_live_approved": true,
                "reason": "all required evidence is current"
            }
        }))?;
        assert_eq!(result.get("status").and_then(Value::as_str), Some("ready"));
        assert_eq!(
            result.get("release_approved").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(result.get("blocker_count").and_then(Value::as_u64), Some(0));
        Ok(())
    }

    #[test]
    fn native_release_review_reports_stale_evidence_as_a_blocked_decision() -> anyhow::Result<()> {
        let result = appsec_release_review_result(serde_json::json!({
            "ok": false,
            "report_complete": false,
            "error": "completion evidence changed after finish",
            "completion_review": {
                "closable": false,
                "blocker_count": 1,
                "blockers": [{"code": "stale-finish-evidence"}]
            }
        }))?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("blocked")
        );
        assert_eq!(
            result.get("release_approved").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(result.get("blocker_count").and_then(Value::as_u64), Some(1));
        assert_eq!(
            result.pointer("/blockers/0/code").and_then(Value::as_str),
            Some("stale-finish-evidence")
        );
        Ok(())
    }

    #[test]
    fn native_release_review_does_not_trust_ready_over_completion_blockers() -> anyhow::Result<()> {
        let result = appsec_release_review_result(serde_json::json!({
            "ok": true,
            "report_complete": true,
            "completion_review": {
                "closable": false,
                "blocker_count": 1,
                "blockers": [{"code": "delta-audit-pending"}]
            },
            "release_decision": {
                "decision": "ready",
                "go_live_approved": true,
                "reason": "inconsistent upstream decision"
            }
        }))?;
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("blocked")
        );
        assert_eq!(
            result.get("release_approved").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(result.get("blocker_count").and_then(Value::as_u64), Some(1));
        Ok(())
    }

    #[test]
    fn native_release_review_rejects_inconsistent_completion_counts() {
        let error = appsec_release_review_result(serde_json::json!({
            "ok": true,
            "report_complete": true,
            "completion_review": {
                "closable": true,
                "blocker_count": 1,
                "blockers": []
            },
            "release_decision": {
                "decision": "ready",
                "go_live_approved": true
            }
        }))
        .expect_err("contradictory completion evidence must fail closed");
        assert!(error.to_string().contains("blocker count"), "{error:#}");
    }

    #[test]
    fn native_release_review_rejects_unreadable_gate_output() {
        let error = appsec_release_review_result(serde_json::json!({"ok": false}))
            .expect_err("missing gate evidence must fail closed");
        assert!(
            error.to_string().contains("missing completion evidence"),
            "{error:#}"
        );
    }

    #[test]
    fn app_audit_command_is_write_gated_and_rejects_unbounded_payloads() -> anyhow::Result<()> {
        assert!(appsec_business_command_requires_data_write(
            "ctox.appsec.app.audit"
        ));
        let root = tempdir()?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_app_audit".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.app.audit".into(),
            record_id: None,
            payload: serde_json::json!({"source_path": "/tmp/untrusted"}),
            client_context: Value::Null,
        };
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("module id is mandatory");
        assert!(error.to_string().contains("payload.module_id"), "{error:#}");

        command.payload = serde_json::json!({
            "module_id": "sample-app",
            "profile": "release",
            "active": true
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("active audit needs an isolated deployment and approval");
        assert!(error.to_string().contains("--deployed-url"), "{error:#}");
        Ok(())
    }

    #[test]
    fn appsec_assessment_run_is_a_write_command_with_bounded_active_gates() -> anyhow::Result<()> {
        assert!(appsec_business_command_requires_data_write(
            "ctox.appsec.assessment.run"
        ));
        assert!(appsec_business_command_requires_data_write(
            "ctox.appsec.assessment.archive"
        ));
        let root = tempdir()?;
        fs::create_dir_all(root.path().join("project"))?;
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_assessment_run".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.assessment.run".into(),
            record_id: Some("runtime/appsec/test".into()),
            payload: serde_json::json!({
                "state_dir": "runtime/appsec/test",
                "url": "https://example.test",
                "source_path": "project",
                "profile": "full",
                "active": true
            }),
            client_context: Value::Null,
        };
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("active assessment must require a durable approval id");
        assert!(error.to_string().contains("payload.approval_id"));
        Ok(())
    }

    #[test]
    fn appsec_assessment_definition_creates_and_archives_a_managed_test() -> anyhow::Result<()> {
        let root = tempdir()?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_assessment_create".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.assessment.create".into(),
            record_id: Some("runtime/appsec/tests/customer-portal".into()),
            payload: serde_json::json!({
                "state_dir": "runtime/appsec/tests/customer-portal",
                "name": "Customer portal",
                "url": "https://example.test",
                "profile": "deep"
            }),
            client_context: Value::Null,
        };
        let created = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(
            created.get("name").and_then(Value::as_str),
            Some("Customer portal")
        );
        assert_eq!(created.get("status").and_then(Value::as_str), Some("draft"));
        assert!(created
            .pointer("/ctox_durable_projection/business_os_projection/projected_count")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= 1));

        command.command_type = "ctox.appsec.assessment.archive".into();
        command.id = Some("cmd_appsec_assessment_archive".into());
        let archived = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(
            archived.get("status").and_then(Value::as_str),
            Some("archived")
        );
        Ok(())
    }

    #[test]
    fn appsec_assessment_run_rejects_unbounded_source_paths_and_unknown_profiles(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let outside = tempdir()?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_assessment_run_invalid".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.assessment.run".into(),
            record_id: Some("runtime/appsec/test".into()),
            payload: serde_json::json!({
                "state_dir": "runtime/appsec/test",
                "source_path": outside.path(),
                "profile": "full"
            }),
            client_context: Value::Null,
        };
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("source workspace must stay inside the CTOX workspace");
        assert!(error.to_string().contains("source_path must stay inside"));

        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/test",
            "url": "https://example.test",
            "profile": "unbounded"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("unknown profile must be rejected before execution");
        assert!(error.to_string().contains("payload.profile"));
        Ok(())
    }

    #[test]
    fn appsec_investigation_and_replay_commands_are_typed_write_actions() -> anyhow::Result<()> {
        let root = tempdir()?;
        for (command_type, expected_error) in [
            (
                "ctox.appsec.investigation.execute",
                "payload.investigation_id",
            ),
            (
                "ctox.appsec.investigation.refute",
                "payload.investigation_id",
            ),
            ("ctox.appsec.replay.baseline", "payload.assessment_id"),
            ("ctox.appsec.replay.investigations", "payload.assessment_id"),
        ] {
            assert!(appsec_business_command_requires_data_write(command_type));
            let command = BusinessCommand {
                origin: CommandOrigin::TrustedLocal,
                id: Some(format!("cmd_{}", command_type.replace('.', "_"))),
                module: APPSEC_MODULE_ID.into(),
                command_type: command_type.into(),
                record_id: None,
                payload: serde_json::json!({"state_dir": "runtime/appsec/test"}),
                client_context: Value::Null,
            };
            let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
                .expect_err("typed command must reject its missing identifier");
            assert!(
                error.to_string().contains(expected_error),
                "{command_type}: {error:#}"
            );
        }
        Ok(())
    }

    #[test]
    fn appsec_audit_run_is_a_write_command_with_bounded_active_gates() -> anyhow::Result<()> {
        assert!(appsec_business_command_requires_data_write(
            "ctox.appsec.audit.run"
        ));
        assert!(!appsec_business_command_requires_data_write(
            "ctox.appsec.exploit.list"
        ));
        assert!(!appsec_business_command_requires_data_write(
            "ctox.appsec.exploit.get"
        ));
        let root = tempdir()?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_audit_run".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.audit.run".into(),
            record_id: Some("runtime/appsec/test".into()),
            payload: serde_json::json!({
                "state_dir": "runtime/appsec/test",
                "url": "https://example.test",
                "profile": "standard",
                "active": true
            }),
            client_context: Value::Null,
        };
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("active audit run must require a durable approval id");
        assert!(error.to_string().contains("payload.approval_id"));

        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/test",
            "url": "https://example.test",
            "profile": "deep"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("unsupported audit run profile must be rejected before execution");
        assert!(error.to_string().contains("payload.profile"));

        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/test",
            "profile": "standard"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("audit run without any target must be rejected");
        assert!(error.to_string().contains("payload.url"));

        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/test",
            "source": "../outside-repo"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("source path traversal must be rejected");
        assert!(error.to_string().contains("parent-directory"));
        Ok(())
    }

    #[test]
    fn appsec_exploit_list_and_get_read_only_state_dir_exploits() -> anyhow::Result<()> {
        let root = tempdir()?;
        let state = root.path().join("runtime/appsec/test");
        let exploits_dir = state.join("exploits");
        fs::create_dir_all(&exploits_dir)?;
        let script = "#!/usr/bin/env python3\nprint('bounded proof')\n";
        fs::write(exploits_dir.join("exploit_F-001.py"), script)?;
        fs::write(
            exploits_dir.join("index.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.exploit_index.v1",
                "generated_at": "1784756437937",
                "exploits": [{
                    "finding_id": "F-001",
                    "title": "Reflected XSS",
                    "severity": "high",
                    "cwe": "CWE-79",
                    "cvss_score": 7.1,
                    "target": "https://example.test",
                    "script": exploits_dir.join("exploit_F-001.py").display().to_string(),
                    "script_sha256": "abc",
                    "verification": {"status": "still-reproduces"},
                    "origin": "crafted",
                    "craft": {
                        "session_id": "session-1",
                        "session": "/state/crafting/session-1/session.json",
                        "iterations": 1,
                        "requests_used": 3,
                    },
                }],
            }))?,
        )?;

        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_exploit_list".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.exploit.list".into(),
            record_id: None,
            payload: serde_json::json!({"state_dir": "runtime/appsec/test"}),
            client_context: Value::Null,
        };
        let list = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(list.get("ok").and_then(Value::as_bool), Some(true));
        let entries = list
            .get("exploits")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].get("finding_id").and_then(Value::as_str),
            Some("F-001")
        );
        assert_eq!(
            entries[0].get("script_name").and_then(Value::as_str),
            Some("exploit_F-001.py")
        );
        assert_eq!(
            entries[0]
                .get("verification_status")
                .and_then(Value::as_str),
            Some("still-reproduces")
        );
        assert_eq!(
            entries[0].get("origin").and_then(Value::as_str),
            Some("crafted")
        );
        assert_eq!(
            entries[0]
                .pointer("/craft/session_id")
                .and_then(Value::as_str),
            Some("session-1")
        );
        assert_eq!(
            entries[0]
                .pointer("/craft/iterations")
                .and_then(Value::as_u64),
            Some(1)
        );

        command.command_type = "ctox.appsec.exploit.get".into();
        command.id = Some("cmd_appsec_exploit_get".into());
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/test",
            "name": "exploit_F-001.py"
        });
        let got = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(got.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(got.get("content").and_then(Value::as_str), Some(script));
        let expected_sha = {
            let mut hasher = Sha256::new();
            hasher.update(script.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        assert_eq!(
            got.get("sha256").and_then(Value::as_str),
            Some(expected_sha.as_str())
        );

        // Traversal and non-.py names are rejected before any file read.
        for bad_name in ["../secret.py", "..", "nested/exploit.py", "notes.txt"] {
            command.payload = serde_json::json!({
                "state_dir": "runtime/appsec/test",
                "name": bad_name
            });
            let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
                .expect_err("unsafe exploit name must be rejected");
            assert!(
                error.to_string().contains("plain .py file name"),
                "{bad_name}: {error:#}"
            );
        }
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/test",
            "name": "exploit_F-999.py"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("missing exploit script must fail");
        assert!(error.to_string().contains("not found"));

        // An empty state dir yields an honest empty list, not an error.
        command.command_type = "ctox.appsec.exploit.list".into();
        command.payload = serde_json::json!({"state_dir": "runtime/appsec/empty"});
        let empty = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(empty.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            empty
                .get("exploits")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(0)
        );
        Ok(())
    }

    /// Bounded proof script accepted by the pentest bundle validator. The
    /// doc/comment lines carry the required marker needles. `reproduces`
    /// drives the verdict; with --expect the script maps to the
    /// expectation_exit convention (0 = expectation met, 2 = mismatch),
    /// without it 2 = still-reproduces, 0 = fixed.
    fn appsec_verify_reproduce_script(reproduces: bool) -> String {
        let python_literal = if reproduces { "True" } else { "False" };
        format!(
            "#!/usr/bin/env python3\n\"\"\"Bounded proof replay.\n\nExit codes: 0=fixed or denied, 1=setup/inconclusive, 2=impact confirmed.\nSCRIPT_MODE=generic_http_replay; CONFIRM_NON_GET=1 gates state-changing probes.\n\"\"\"\nimport sys\n\n\ndef main() -> int:\n    expect = None\n    argv = sys.argv[1:]\n    if \"--expect\" in argv:\n        expect = argv[argv.index(\"--expect\") + 1]\n    reproduces = {python_literal}\n    if expect is not None:\n        met = (expect == \"vulnerable\") == reproduces\n        return 0 if met else 2\n    # return 2 while the impact reproduces; return 0 once fixed.\n    return 2 if reproduces else 0\n\n\nif __name__ == \"__main__\":\n    sys.exit(main())\n"
        )
    }

    /// Write a minimal but valid pentest state dir for exploit verify tests:
    /// findings.json, one issue bundle per finding whose proof script knows
    /// whether the impact still reproduces, plus the exploit index used by
    /// the business command.
    fn write_appsec_verify_state(
        root: &Path,
        state_name: &str,
        specs: &[(&str, &str, bool)],
    ) -> anyhow::Result<PathBuf> {
        let state = root.join("runtime/appsec").join(state_name);
        let exploits_dir = state.join("exploits");
        fs::create_dir_all(&exploits_dir)?;
        let mut findings = Vec::new();
        let mut index_entries = Vec::new();
        for (finding_id, title, reproduces) in specs {
            findings.push(serde_json::json!({
                "id": finding_id,
                "title": title,
                "severity": "high",
                "category": "xss",
                "status": "open",
                "target": "https://example.test",
                "endpoint": "https://example.test/search?q=x",
                "method": "GET",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            }));
            let bundle_name = format!(
                "{}-{}",
                finding_id.to_ascii_lowercase(),
                title.to_ascii_lowercase().replace(' ', "-")
            );
            let bundle_dir = state.join("reports/issue-bundles").join(&bundle_name);
            fs::create_dir_all(&bundle_dir)?;
            fs::write(
                bundle_dir.join("finding.json"),
                serde_json::to_string_pretty(
                    &serde_json::json!({"id": finding_id, "title": title}),
                )?,
            )?;
            fs::write(
                bundle_dir.join("evidence-manifest.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "version": "ctox.appsec_pentest.finding_evidence_manifest.v1",
                    "finding_id": finding_id,
                    "evidence": [],
                }))?,
            )?;
            fs::write(
                bundle_dir.join("github-issue.md"),
                format!("# {finding_id}\n\n## Proof Script\n\nRun reproduce.py.\n\n## Fix Verification\n\n- pentest exploit verify --id {finding_id} --execute --expect vulnerable\n- pentest exploit verify --id {finding_id} --execute --expect fixed\n"),
            )?;
            fs::write(bundle_dir.join("README.md"), "bundle\n")?;
            fs::write(bundle_dir.join("requirements.txt"), "")?;
            fs::write(
                bundle_dir.join("reproduce.py"),
                appsec_verify_reproduce_script(*reproduces),
            )?;
            let script_name = format!("exploit_{finding_id}.py");
            fs::write(
                exploits_dir.join(&script_name),
                appsec_verify_reproduce_script(*reproduces),
            )?;
            index_entries.push(serde_json::json!({
                "finding_id": finding_id,
                "title": title,
                "severity": "high",
                "script": exploits_dir.join(&script_name).display().to_string(),
                "script_sha256": "abc",
            }));
        }
        fs::write(
            state.join("findings.json"),
            serde_json::to_string_pretty(&Value::Array(findings))?,
        )?;
        fs::write(
            exploits_dir.join("index.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.exploit_index.v1",
                "generated_at": "1784756437937",
                "exploits": index_entries,
            }))?,
        )?;
        Ok(state)
    }

    #[test]
    fn appsec_exploit_verify_is_a_write_command_with_expect_and_confirm_gates() -> anyhow::Result<()>
    {
        assert!(appsec_business_command_requires_data_write(
            "ctox.appsec.exploit.verify"
        ));

        let root = tempdir()?;
        write_appsec_verify_state(
            root.path(),
            "verify-gates",
            &[("F-001", "Reflected XSS", true)],
        )?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_exploit_verify".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.exploit.verify".into(),
            record_id: None,
            payload: serde_json::json!({"state_dir": "runtime/appsec/verify-gates"}),
            client_context: Value::Null,
        };

        // Invalid expect values are rejected before any CLI execution.
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/verify-gates",
            "expect": "pwned"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("invalid expect must fail before execution");
        assert!(error.to_string().contains("payload.expect"), "{error:#}");

        // Active/non-GET confirms require an approval id, same gate as audit.run.
        for confirm in ["confirm_active", "confirm_non_get"] {
            command.payload = serde_json::json!({
                "state_dir": "runtime/appsec/verify-gates",
                confirm: true
            });
            let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
                .expect_err("confirm without approval must fail");
            assert!(
                error.to_string().contains("payload.approval_id"),
                "{confirm}: {error:#}"
            );
        }

        // Traversal and non-.py names are rejected before any CLI execution.
        for bad_name in ["../secret.py", "..", "nested/exploit.py", "notes.txt"] {
            command.payload = serde_json::json!({
                "state_dir": "runtime/appsec/verify-gates",
                "name": bad_name
            });
            let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
                .expect_err("unsafe exploit name must be rejected");
            assert!(
                error.to_string().contains("plain .py file name"),
                "{bad_name}: {error:#}"
            );
        }

        // Unknown script names fail honestly against the index.
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/verify-gates",
            "name": "exploit_F-999.py"
        });
        let error = handle_appsec_business_command(root.path(), &chef_session(), &command)
            .expect_err("unknown exploit name must fail");
        assert!(
            error.to_string().contains("unknown exploit script name"),
            "{error:#}"
        );
        Ok(())
    }

    #[test]
    fn appsec_exploit_verify_runs_one_script_by_name() -> anyhow::Result<()> {
        let root = tempdir()?;
        let state = write_appsec_verify_state(
            root.path(),
            "verify-one",
            &[("F-001", "Reflected XSS", true)],
        )?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_exploit_verify".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.exploit.verify".into(),
            record_id: None,
            payload: serde_json::json!({
                "state_dir": "runtime/appsec/verify-one",
                "name": "exploit_F-001.py"
            }),
            client_context: Value::Null,
        };

        // The proof still reproduces: exit 2 maps to still-reproduces and the
        // run counts as executed even though the CLI marks it not-ok.
        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("finding_id").and_then(Value::as_str),
            Some("F-001")
        );
        assert_eq!(
            result.get("script_name").and_then(Value::as_str),
            Some("exploit_F-001.py")
        );
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("still-reproduces")
        );
        assert_eq!(result.get("exit_code").and_then(Value::as_i64), Some(2));
        assert!(result.get("duration_ms").and_then(Value::as_u64).is_some());
        assert!(result.get("expectation").is_none());

        // With expect vulnerable the expectation block reports expected/met.
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/verify-one",
            "name": "exploit_F-001.py",
            "expect": "vulnerable"
        });
        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result
                .pointer("/expectation/expected")
                .and_then(Value::as_str),
            Some("vulnerable")
        );
        assert_eq!(
            result.pointer("/expectation/met").and_then(Value::as_bool),
            Some(true)
        );

        // Confirms pass through once an approval id is present.
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/verify-one",
            "name": "exploit_F-001.py",
            "confirm_active": true,
            "approval_id": "apr-test-1"
        });
        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("still-reproduces")
        );

        // After the fix the same script reports fixed-or-not-reproducible.
        fs::write(
            state.join("reports/issue-bundles/f-001-reflected-xss/reproduce.py"),
            appsec_verify_reproduce_script(false),
        )?;
        command.payload = serde_json::json!({
            "state_dir": "runtime/appsec/verify-one",
            "name": "exploit_F-001.py",
            "expect": "fixed"
        });
        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("fixed-or-not-reproducible")
        );
        assert_eq!(result.get("exit_code").and_then(Value::as_i64), Some(0));
        assert_eq!(
            result.pointer("/expectation/met").and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn appsec_exploit_verify_all_aggregates_index_results() -> anyhow::Result<()> {
        let root = tempdir()?;
        write_appsec_verify_state(
            root.path(),
            "verify-all",
            &[
                ("F-001", "Reflected XSS", true),
                ("F-002", "SQL Injection", false),
            ],
        )?;
        let mut command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_appsec_exploit_verify".into()),
            module: APPSEC_MODULE_ID.into(),
            command_type: "ctox.appsec.exploit.verify".into(),
            record_id: None,
            payload: serde_json::json!({"state_dir": "runtime/appsec/verify-all"}),
            client_context: Value::Null,
        };

        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].get("finding_id").and_then(Value::as_str),
            Some("F-001")
        );
        assert_eq!(
            results[0].get("script_name").and_then(Value::as_str),
            Some("exploit_F-001.py")
        );
        assert_eq!(
            results[0].get("status").and_then(Value::as_str),
            Some("still-reproduces")
        );
        assert_eq!(results[0].get("exit_code").and_then(Value::as_i64), Some(2));
        assert_eq!(
            results[1].get("status").and_then(Value::as_str),
            Some("fixed-or-not-reproducible")
        );
        assert_eq!(results[1].get("exit_code").and_then(Value::as_i64), Some(0));
        let summary = result.get("summary").cloned().unwrap_or(Value::Null);
        assert_eq!(summary.get("verified").and_then(Value::as_u64), Some(2));
        assert_eq!(
            summary.get("still_reproduces").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(summary.get("fixed").and_then(Value::as_u64), Some(1));
        assert_eq!(summary.get("inconclusive").and_then(Value::as_u64), Some(0));
        assert_eq!(summary.get("failed").and_then(Value::as_u64), Some(0));

        // An index entry whose finding is gone degrades to a failed entry
        // instead of aborting the whole run.
        let state = root.path().join("runtime/appsec/verify-all");
        let exploits_dir = state.join("exploits");
        fs::write(
            exploits_dir.join("exploit_F-999.py"),
            appsec_verify_reproduce_script(true),
        )?;
        fs::write(
            exploits_dir.join("index.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.exploit_index.v1",
                "generated_at": "1784756437937",
                "exploits": [
                    {
                        "finding_id": "F-001",
                        "title": "Reflected XSS",
                        "script": exploits_dir.join("exploit_F-001.py").display().to_string(),
                    },
                    {
                        "finding_id": "F-002",
                        "title": "SQL Injection",
                        "script": exploits_dir.join("exploit_F-002.py").display().to_string(),
                    },
                    {
                        "finding_id": "F-999",
                        "title": "Ghost",
                        "script": exploits_dir.join("exploit_F-999.py").display().to_string(),
                    },
                ],
            }))?,
        )?;
        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(results.len(), 3);
        assert_eq!(
            results[2].get("status").and_then(Value::as_str),
            Some("execution-failed")
        );
        assert!(results[2].get("error").and_then(Value::as_str).is_some());
        let summary = result.get("summary").cloned().unwrap_or(Value::Null);
        assert_eq!(summary.get("verified").and_then(Value::as_u64), Some(3));
        assert_eq!(summary.get("failed").and_then(Value::as_u64), Some(1));

        // A state dir without any index verifies nothing but stays honest.
        command.payload = serde_json::json!({"state_dir": "runtime/appsec/verify-none"});
        let result = handle_appsec_business_command(root.path(), &chef_session(), &command)?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result
                .get("results")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            result.pointer("/summary/verified").and_then(Value::as_u64),
            Some(0)
        );
        assert!(result.get("note").and_then(Value::as_str).is_some());
        Ok(())
    }
}
