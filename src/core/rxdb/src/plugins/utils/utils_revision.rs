//! Document revision string handling.
//!
//! A revision is the string `"<height>-<hash>"`, e.g. `"3-abcd"`.

use crate::rx_error::{new_rx_error, RxError, RxResult};

// ref: rxdb/src/plugins/utils/utils-revision.ts:5-19
/// Parses the full revision.
/// Do NOT use this if you only need the revision height,
/// then use [`get_height_of_revision`] instead which is faster.
pub fn parse_revision(revision: &str) -> RxResult<ParsedRevision> {
    let split: Vec<&str> = revision.splitn(2, '-').collect();
    if split.len() != 2 || revision.matches('-').count() != 1 {
        return Err(malformatted(revision));
    }
    let height = split[0]
        .parse::<u64>()
        .map_err(|_| malformatted(revision))?;
    Ok(ParsedRevision {
        height,
        hash: split[1].to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct ParsedRevision {
    pub height: u64,
    pub hash: String,
}

// ref: rxdb/src/plugins/utils/utils-revision.ts:21-37
/// @hotPath Performance is very important here
/// because we need to parse the revision height very often.
/// Do not use `parseInt(revision.split('-')[0], 10)` because
/// only fetching the start-number chars is faster.
pub fn get_height_of_revision(revision: &str) -> RxResult<u64> {
    let mut use_chars = String::new();
    for ch in revision.chars() {
        if ch == '-' {
            return use_chars.parse::<u64>().map_err(|_| malformatted(revision));
        }
        use_chars.push(ch);
    }
    Err(malformatted(revision))
}

// ref: rxdb/src/plugins/utils/utils-revision.ts:40-49
/// Creates the next write revision for a given document.
/// `previous_rev` is the `_rev` of the previous version of the document, if any.
pub fn create_revision(
    database_instance_token: &str,
    previous_rev: Option<&str>,
) -> RxResult<String> {
    let new_height = match previous_rev {
        None => 1,
        Some(r) => match get_height_of_revision(r) {
            Ok(height) => height + 1,
            Err(_) => 1,
        },
    };
    Ok(format!("{new_height}-{database_instance_token}"))
}

fn malformatted(revision: &str) -> RxError {
    new_rx_error(
        "UTL2",
        Some(serde_json::json!({ "message": format!("malformatted revision: {revision}") })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_revision_repairs_legacy_malformed_previous_revision() {
        let revision = create_revision("token", Some("rev_legacy_uuid"))
            .expect("revision creation should tolerate legacy malformed revisions");

        assert_eq!(revision, "1-token");
    }
}
