use super::*;
use crate::{registry::Registry, writer::WriterOptions, DbId};

#[tokio::test]
async fn lane4_cas_resume_integrity_and_atomic_file_publication() {
    let dir = tempfile::tempdir().unwrap();
    let cas = Cas::open(dir.path().join("cas")).unwrap();
    let data = b"durable prefix and unacknowledged suffix";
    let reference = format!("{:x}", Sha256::digest(data));
    let scope = "a".repeat(64);
    let mut receiver = cas.receive(&scope, &reference, data.len() as u64).unwrap();
    receiver.append(0, &data[..8]).unwrap();
    assert!(
        cas.receive(&scope, &reference, data.len() as u64).is_err(),
        "concurrent receiver must not share staging inode"
    );
    drop(receiver);
    let mut receiver = cas.receive(&scope, &reference, data.len() as u64).unwrap();
    assert_eq!(receiver.offset().unwrap(), 8);
    assert!(
        receiver.append(9, &data[9..]).is_err(),
        "resume gap accepted"
    );
    receiver.append(0, &data[..8]).unwrap();
    assert!(receiver.append(0, b"badbytes").is_err());
    receiver.append(8, &data[8..]).unwrap();
    let blob = receiver.finish().unwrap();
    assert_eq!(
        cas.verified(&reference, data.len() as u64)
            .unwrap()
            .reference(),
        reference
    );
    std::fs::write(dir.path().join("primitive-output"), b"generated output").unwrap();
    let imported = cas
        .import(dir.path().join("primitive-output"), "d".repeat(64))
        .await
        .unwrap();
    assert_eq!(imported.size(), 16);
    let bad_ref = "b".repeat(64);
    let mut corrupt = cas.receive(&scope, &bad_ref, 3).unwrap();
    corrupt.append(0, b"bad").unwrap();
    assert!(
        corrupt.finish().is_err(),
        "corrupt complete bytes published"
    );
    assert!(!dir.path().join("cas/objects").join(bad_ref).exists());

    let writer = Registry
        .open(
            DbId::new(dir.path().join("sync.sqlite")).unwrap(),
            WriterOptions::default(),
        )
        .unwrap();
    writer.ready().await.unwrap();
    writer.submit(WriteJob::new(JobOptions::default(), |tx| { tx.execute_batch("CREATE TABLE files(ref TEXT PRIMARY KEY, owner_app TEXT); CREATE TABLE effects(ref TEXT)")?; Ok(()) })).unwrap().await.unwrap();
    // Real canonical writer: inserting files then failing the primitive rolls
    // back the file row too. No second connection/transaction inside the job.
    let insert = |tx: &Transaction<'_>, record: &FileRecord| {
        tx.execute(
            "INSERT INTO files VALUES (?1, ?2)",
            (&record.blob.r#ref, &record.owner_app),
        )?;
        Ok(())
    };
    let failed = blob
        .commit_with_file(
            &writer,
            "x".into(),
            "text/plain".into(),
            "app".into(),
            insert,
            |_, _| Err::<(), _>(invalid("primitive failed")),
        )
        .await;
    assert!(failed.is_err());
    let (published, ()) = cas
        .produce_file(
            dir.path().join("primitive-output"),
            "e".repeat(64),
            &writer,
            FileMetadata {
                name: "x".into(),
                mime: "text/plain".into(),
                owner_app: "app".into(),
            },
            insert,
            |tx, blob| {
                tx.execute("INSERT INTO effects VALUES (?1)", [&blob.r#ref])?;
                Ok(())
            },
        )
        .await
        .unwrap();
    assert_eq!(published.r#ref, imported.reference());
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            let matched: u64 = tx.query_row(
                "SELECT count(*) FROM files JOIN effects USING(ref)",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(matched, 1);
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    writer.shutdown().await.unwrap();
}
