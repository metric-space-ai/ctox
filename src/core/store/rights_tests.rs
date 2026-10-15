use super::*;
use rusqlite::Connection;
use serde_json::json;
#[path = "change_log.rs"]
mod change_log;

fn rule() -> Right {
    Right {
        role: "user".into(),
        collection: "leads".into(),
        row_filter: scope::RowFilter::Eq {
            path: "/owner".into(),
            value: scope::FilterValue::ActorId,
        },
        field_mask: scope::FieldMask::Fields {
            paths: ["/name".into()].into(),
        },
        ops: [Operation::Read, Operation::Write].into(),
    }
}
#[test]
fn rights_share_p3_seq_and_pages_and_rollback_with_their_transaction() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    change_log::initialize(&tx).unwrap();
    initialize(&tx).unwrap();
    assert_eq!(replace(&tx, 1, &[rule()]).unwrap(), 2);
    let snapshot = read(&tx).unwrap();
    assert_eq!(snapshot.rights_ver, 2);
    assert_eq!(snapshot.rules.len(), 1);
    let page = change_log::read_page(&tx, "_rights", 1, 0, 100).unwrap();
    assert_eq!(page.docs.len(), 2);
    assert!(page.docs.iter().all(|d| d.body.is_some()));
    assert_eq!(page.head_seq, 2);
    let seq = page.head_seq;
    assert!(replace(&tx, 1, &[]).is_err());
    assert_eq!(change_log::head(&tx).unwrap().head_seq, seq);
    tx.commit().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(replace(&tx, 2, &[]).unwrap(), 3);
    let page = change_log::read_page(&tx, "_rights", 1, seq, 100).unwrap();
    assert!(page
        .docs
        .iter()
        .any(|d| d.op == change_log::Operation::Delete));
    assert!(read(&tx).unwrap().rules.is_empty());
    tx.rollback().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(read(&tx).unwrap().rights_ver, 2);
    assert_eq!(change_log::head(&tx).unwrap().head_seq, seq);
}
#[test]
fn rights_replacement_is_atomic_even_if_caller_catches_failure() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    change_log::initialize(&tx).unwrap();
    initialize(&tx).unwrap();
    replace(&tx, 1, &[rule()]).unwrap();
    let mut duplicate = rule();
    duplicate.row_filter = scope::RowFilter::All;
    assert!(replace(&tx, 2, &[rule(), duplicate]).is_err());
    assert_eq!(read(&tx).unwrap().rights_ver, 2);
    assert_eq!(read(&tx).unwrap().rules, vec![rule()]);
}
#[test]
fn current_rights_gate_outbox_owner_and_hidden_field_mutations() {
    let snapshot = RightsSnapshot {
        rights_ver: 2,
        rules: vec![rule()],
    };
    let before = json!({"owner":"alice","name":"A","secret":"S"});
    let after = json!({"owner":"alice","name":"B","secret":"S"});
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), Some(&after))
        .is_ok());
    for bad in [
        json!({"owner":"alice","name":"B","secret":"leak"}),
        json!({"owner":"bob","name":"B","secret":"S"}),
        json!({"owner":"alice","name":"B"}),
    ] {
        assert!(snapshot
            .authorize_write("user", "alice", "leads", Some(&before), Some(&bad))
            .is_err());
    }
    assert!(snapshot
        .authorize_write("admin", "alice", "leads", Some(&before), Some(&after))
        .is_err());
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), None)
        .is_err());
    assert!(snapshot
        .resolve("user", "alice")
        .unwrap()
        .projection("leads")
        .is_some());
    assert!(snapshot
        .resolve("admin", "alice")
        .unwrap()
        .projections
        .is_empty());
}

#[test]
fn write_cannot_replace_a_masked_parent_object_with_a_scalar() {
    let mut right = rule();
    right.field_mask = scope::FieldMask::Fields {
        paths: ["/profile/name".into()].into(),
    };
    let snapshot = RightsSnapshot {
        rights_ver: 2,
        rules: vec![right],
    };
    let before = json!({"owner":"alice","profile":{"name":"A"}});
    let bad = json!({"owner":"alice","profile":"forbidden"});
    let good = json!({"owner":"alice","profile":{"name":"B"}});
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), Some(&bad))
        .is_err());
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), Some(&good))
        .is_ok());
}
