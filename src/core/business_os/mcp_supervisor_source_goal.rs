// Origin: CTOX
// License: AGPL-3.0-only
//! Lossless, bounded Source pages. The caller's original controller and current
//! DataRead policy are revalidated by tools::respond before EVERY cache access.
use super::{wire, HashMap, Value};
use anyhow::{ensure, Context};
use sha2::{Digest, Sha256};
use wire::WireValidate;

const SCHEMA: &str = "ctox.workjet.supervisor.confirmed_goal_page.v1";
const MAX_DOCUMENT_BYTES: usize =
    crate::business_os::project_chats::jour_fixe_confirmed_goal::MAX_METADATA_BYTES + 4096;
const MAX_FRAGMENT_BYTES: usize = 24 * 1024;
const MAX_SNAPSHOTS: usize = 8;

struct Snapshot {
    document: String,
    digest: String,
    project: String,
    thread: String,
    goal_state: Value,
    captured_at_ms: i64,
    deadline_ms: i64,
    complete: bool,
}
impl Snapshot {
    fn capture(current: Value, deadline_ms: i64, now_ms: i64) -> anyhow::Result<Self> {
        let project = current["project_id"]
            .as_str()
            .context("native goal project missing")?
            .to_owned();
        let thread = current["supervisor_thread_id"]
            .as_str()
            .context("native goal thread missing")?
            .to_owned();
        let goal = &current["confirmed_goal"];
        let goal_state = if goal.is_null() {
            Value::Null
        } else {
            ensure!(
                goal["goal"].is_object() && goal["status"].is_string(),
                "native goal revision or status missing"
            );
            serde_json::json!({"goal":goal["goal"],"status":goal["status"]})
        };
        let document = serde_json::to_string(&current)?;
        ensure!(
            document.len() <= MAX_DOCUMENT_BYTES,
            "native goal snapshot exceeds document budget"
        );
        // Compact serde JSON escapes C0 characters. Escaping this JSON string
        // into the page costs at most 2x; 24 KiB fragments leave room for the
        // typed page and original Source envelope below the 64 KiB bridge.
        let digest = format!("{:x}", Sha256::digest(document.as_bytes()));
        Ok(Self {
            document,
            digest,
            project,
            thread,
            goal_state,
            captured_at_ms: now_ms.max(0),
            deadline_ms,
            complete: false,
        })
    }
    fn response(&self, id: &str, state: &str, offset: usize) -> anyhow::Result<Value> {
        ensure!(
            offset <= self.document.len() && self.document.is_char_boundary(offset),
            "native goal cursor is not a JSON byte boundary"
        );
        let mut end = (offset + MAX_FRAGMENT_BYTES).min(self.document.len());
        while !self.document.is_char_boundary(end) {
            end -= 1;
        }
        let page = state == "page";
        let fragment = if page {
            &self.document[offset..end]
        } else {
            ""
        };
        let complete = page && end == self.document.len();
        let next = (page && !complete).then(|| format!("{id}:{end}"));
        let value = serde_json::json!({"schema":SCHEMA,"state":state,
            "project_id":self.project,"supervisor_thread_id":self.thread,
            "snapshot_id":id,"document_sha256":self.digest,
            "document_bytes":self.document.len(),"byte_offset":offset,
            "byte_length":fragment.len(),"json_fragment":fragment,
            "captured_at_ms":self.captured_at_ms,"document_complete":complete,
            "next_cursor":next});
        serde_json::from_value::<wire::SourceGoalReadPage>(value.clone())?
            .validate()
            .map_err(anyhow::Error::msg)?;
        // Validate the entire envelope, not just its inner result.
        let envelope = serde_json::json!({"version":1,"state":"tool_result",
            "operation_id":uuid::Uuid::nil().to_string(),"native_tool":"confirmed_goal_read",
            "result":value,"execution_ready":false});
        ensure!(
            serde_json::to_vec(&envelope)?.len() <= 64 * 1024,
            "native goal page exceeds Source envelope budget"
        );
        Ok(value)
    }
}

#[derive(Default)]
pub(super) struct Snapshots {
    entries: HashMap<(String, String), Snapshot>,
}
impl Snapshots {
    pub(super) fn page(
        &mut self,
        controller: &str,
        operation: &str,
        cursor: Option<&str>,
        current: Value,
        deadline_ms: i64,
        now_ms: i64,
    ) -> anyhow::Result<Value> {
        let canonical_operation = uuid::Uuid::parse_str(operation)?.to_string();
        ensure!(
            operation == canonical_operation && deadline_ms > now_ms,
            "native goal operation is noncanonical or expired"
        );
        self.entries.retain(|_, v| v.deadline_ms > now_ms);
        let (id, offset) = match cursor {
            None => (canonical_operation, 0),
            Some(cursor) => {
                let (id, offset) = cursor
                    .split_once(':')
                    .context("native goal cursor malformed")?;
                ensure!(
                    uuid::Uuid::parse_str(id)?.to_string() == id,
                    "native goal cursor identity is noncanonical"
                );
                let parsed = offset.parse::<usize>()?;
                ensure!(
                    parsed.to_string() == offset && parsed <= MAX_DOCUMENT_BYTES,
                    "native goal cursor offset exceeds budget"
                );
                (id.to_owned(), parsed)
            }
        };
        let key = (controller.to_owned(), id.clone());
        let captured = Snapshot::capture(current, deadline_ms, now_ms)?;
        if let Some(snapshot) = self.entries.get_mut(&key) {
            ensure!(
                snapshot.project == captured.project && snapshot.thread == captured.thread,
                "native goal snapshot scope changed"
            );
            if snapshot.goal_state != captured.goal_state {
                // Owner replaced, paused or completed the goal. No page of the
                // old target is returned; this is NOT retirement of the Source.
                snapshot.complete = true;
                return snapshot.response(&id, "snapshot_changed", 0);
            }
            let result = snapshot.response(&id, "page", offset)?;
            snapshot.complete |= result["document_complete"] == true;
            return Ok(result);
        }
        if cursor.is_some() {
            return captured.response(&id, "snapshot_unavailable", 0);
        }
        if self.entries.len() >= MAX_SNAPSHOTS {
            // Only completed or invalidated snapshots may be displaced. An active read never
            // loses a page to another read while its controller is still valid.
            if let Some(old) = self
                .entries
                .iter()
                .filter(|(_, v)| v.complete)
                .min_by_key(|(_, v)| v.captured_at_ms)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&old);
            } else {
                return captured.response(&id, "capacity_unavailable", 0);
            }
        }
        let result = captured.response(&id, "page", 0)?;
        let mut captured = captured;
        captured.complete = result["document_complete"] == true;
        self.entries.insert(key, captured);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn current(text: &str) -> Value {
        json!({"contract":"ctox.workjet.jour_fixe.v1","project_id":"project",
            "supervisor_thread_id":"supervisor",
            "confirmed_goal":{"goal":{"goal_id":"goal","revision":1},"status":"active",
                "items":[{"title":text,"acceptance":text}],"steps":[{"status":"pending"}]}})
    }
    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    #[test]
    fn native_source_consumes_the_same_bounded_page_fixture_as_the_browser() -> anyhow::Result<()> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../rxdb/tests/fixtures/workjet-supervisor-source-v1.json"
        ))?;
        for case in fixture["valid_cases"].as_array().unwrap() {
            wire::validate_fixture(case["type"].as_str().unwrap(), case["value"].clone())
                .map_err(anyhow::Error::msg)?;
        }
        for case in fixture["invalid_cases"].as_array().unwrap() {
            assert!(
                wire::validate_fixture(case["type"].as_str().unwrap(), case["value"].clone())
                    .is_err(),
                "{}",
                case["reason"]
            );
        }
        Ok(())
    }

    #[test]
    fn large_goal_is_lossless_utf8_and_every_complete_envelope_fits_the_bridge(
    ) -> anyhow::Result<()> {
        let text = "é🧭\\\" \n\t".repeat(12_000);
        let original = current(&text);
        let expected = serde_json::to_string(&original)?;
        assert!(expected.len() > 64 * 1024);
        let mut reads = Snapshots::default();
        let operation = id();
        let mut page = reads.page("controller", &operation, None, original.clone(), 1000, 1)?;
        let digest = page["document_sha256"].clone();
        let mut document = String::new();
        let mut pages = 0;
        loop {
            assert_eq!(page["state"], "page");
            assert_eq!(page["document_sha256"], digest);
            assert_eq!(page["document_bytes"], expected.len());
            assert_eq!(page["byte_offset"], document.len());
            let fragment = page["json_fragment"].as_str().unwrap();
            assert_eq!(page["byte_length"], fragment.len());
            assert!(fragment.len() <= MAX_FRAGMENT_BYTES);
            let wrapped = json!({"version":1,"state":"tool_result","operation_id":id(),
                "native_tool":"confirmed_goal_read","result":page,"execution_ready":false});
            assert!(serde_json::to_vec(&wrapped)?.len() <= 64 * 1024);
            document.push_str(fragment);
            pages += 1;
            if page["document_complete"] == true {
                break;
            }
            let cursor = page["next_cursor"].as_str().unwrap();
            // New SDK call UUID, original immutable native snapshot.
            page = reads.page("controller", &id(), Some(cursor), original.clone(), 1000, 2)?;
        }
        assert!(pages > 2);
        assert!(page["next_cursor"].is_null());
        assert_eq!(document, expected);
        assert_eq!(serde_json::from_str::<Value>(&document)?, original);
        assert_eq!(digest, format!("{:x}", Sha256::digest(document.as_bytes())));
        Ok(())
    }

    #[test]
    fn progress_is_as_of_capture_but_goal_replacement_or_status_change_starts_a_new_read(
    ) -> anyhow::Result<()> {
        let original = current(&"a".repeat(70_000));
        let mut reads = Snapshots::default();
        let operation = id();
        let first = reads.page("controller", &operation, None, original.clone(), 1000, 1)?;
        let cursor = first["next_cursor"].as_str().unwrap();
        let mut progress = original.clone();
        progress["confirmed_goal"]["steps"][0]["status"] = json!("completed");
        let next = reads.page("controller", &id(), Some(cursor), progress.clone(), 1000, 2)?;
        assert_eq!(next["state"], "page");
        assert_eq!(next["document_sha256"], first["document_sha256"]);
        assert_eq!(
            reads.page("controller", &operation, None, progress.clone(), 1000, 2)?,
            first
        );
        for (field, value) in [
            ("status", json!("paused")),
            ("goal", json!({"goal_id":"new-goal","revision":2})),
        ] {
            let mut changed = progress.clone();
            changed["confirmed_goal"][field] = value;
            let result = reads.page("controller", &id(), Some(cursor), changed.clone(), 1000, 3)?;
            assert_eq!(result["state"], "snapshot_changed");
            assert_eq!(result["json_fragment"], "");
            assert_eq!(result["document_complete"], false);
            assert_eq!(
                reads.page("controller", &id(), None, changed, 1000, 3)?["state"],
                "page"
            );
        }
        Ok(())
    }

    #[test]
    fn cursors_do_not_cross_controllers_or_expiry_and_malformed_offsets_are_rejected(
    ) -> anyhow::Result<()> {
        let value = current(&"🧭".repeat(30_000));
        let mut reads = Snapshots::default();
        let operation = id();
        let first = reads.page("original", &operation, None, value.clone(), 100, 1)?;
        let cursor = first["next_cursor"].as_str().unwrap();
        assert_eq!(
            reads.page("foreign", &id(), Some(cursor), value.clone(), 100, 2)?["state"],
            "snapshot_unavailable"
        );
        for malformed in [
            "other:0".to_owned(),
            format!("{operation}:00"),
            format!("{operation}:{}", MAX_DOCUMENT_BYTES + 1),
            format!("{operation}:18446744073709551616"),
        ] {
            assert!(reads
                .page("original", &id(), Some(&malformed), value.clone(), 100, 2)
                .is_err());
        }
        let snapshot = &reads.entries[&("original".to_owned(), operation.clone())];
        let middle = snapshot.document.find('🧭').unwrap() + 1;
        assert!(reads
            .page(
                "original",
                &id(),
                Some(&format!("{operation}:{middle}")),
                value.clone(),
                100,
                2
            )
            .is_err());
        assert_eq!(
            reads.page("original", &id(), Some(cursor), value, 200, 101)?["state"],
            "snapshot_unavailable"
        );
        Ok(())
    }

    #[test]
    fn bounded_cache_never_evicts_an_active_page_and_reuses_completed_or_expired_entries(
    ) -> anyhow::Result<()> {
        let value = current(&"a".repeat(70_000));
        let mut reads = Snapshots::default();
        for _ in 0..MAX_SNAPSHOTS {
            assert_eq!(
                reads.page("controller", &id(), None, value.clone(), 1000, 1)?["state"],
                "page"
            );
        }
        assert_eq!(
            reads.page("controller", &id(), None, value.clone(), 1000, 2)?["state"],
            "capacity_unavailable"
        );
        assert_eq!(reads.entries.len(), MAX_SNAPSHOTS);
        assert!(
            reads
                .entries
                .values()
                .map(|v| v.document.len())
                .sum::<usize>()
                <= MAX_SNAPSHOTS * MAX_DOCUMENT_BYTES
        );
        let (key, snapshot) = reads.entries.iter_mut().next().unwrap();
        let key = key.clone();
        snapshot.complete = true;
        assert_eq!(
            reads.page("controller", &id(), None, value.clone(), 1000, 2)?["state"],
            "page"
        );
        assert!(!reads.entries.contains_key(&key));
        assert_eq!(
            reads.page("controller", &id(), None, value, 2000, 1001)?["state"],
            "page"
        );
        assert_eq!(reads.entries.len(), 1);
        Ok(())
    }

    #[test]
    fn absent_goal_is_a_complete_json_document_not_a_claim_of_work_completion() -> anyhow::Result<()>
    {
        let value = json!({"contract":"ctox.workjet.jour_fixe.v1","project_id":"project",
            "supervisor_thread_id":"supervisor","confirmed_goal":null});
        let mut reads = Snapshots::default();
        let page = reads.page("controller", &id(), None, value.clone(), 1000, 1)?;
        assert_eq!(page["state"], "page");
        assert_eq!(page["document_complete"], true);
        assert_eq!(
            serde_json::from_str::<Value>(page["json_fragment"].as_str().unwrap())?,
            value
        );
        assert!(!page.as_object().unwrap().contains_key("goal_complete"));
        assert!(Snapshot::capture(current(&"x".repeat(MAX_DOCUMENT_BYTES)), 1000, 1).is_err());
        Ok(())
    }
}
