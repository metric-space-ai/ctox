// Origin: CTOX
// License: AGPL-3.0-only
//! Owner-authorized local audio custody, not a gateway/provider execution proof.
//! Read bounded uploaded WAV bytes through the existing RxDB file/chunk store,
//! then freeze a native copy, its AudioRef and the domain receipt atomically.
use super::super::{store, store_policy, workjet_identity, workjet_jour_fixe_contract as wire};
use super::*;
use base64::Engine;
use rusqlite::{params, OpenFlags, OptionalExtension};
use wire::WireValidate;
const MAX_AUDIO: usize = 8 * 1024 * 1024;
const CHUNK: usize = 16 * 1024;
pub(in crate::business_os) const COMMAND: &str = "ctox.workjet.jour_fixe.narration.local_publish";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_local_narration (
 operation_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, intent_hash TEXT NOT NULL,
 receipt_json TEXT NOT NULL, projections_json TEXT NOT NULL
);";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn parse(command: &BusinessCommand) -> anyhow::Result<(wire::LocalNarrationRequest, Value)> {
    let mut payload = command.payload.clone();
    let object = payload
        .as_object_mut()
        .context("local narration must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let value = channel.as_str().context("inbound channel must be text")?;
        ensure!(
            !value.trim().is_empty() && value.len() <= 256,
            "invalid inbound channel"
        );
    }
    let r: wire::LocalNarrationRequest = serde_json::from_value(payload.clone())?;
    r.validate().map_err(anyhow::Error::msg)?;
    for s in [
        &r.operation_id,
        &r.instance_id,
        &r.project_id,
        &r.meeting_id,
        &r.slide_id,
        &r.file_id,
        &r.generation_id,
    ] {
        ensure!(
            !s.is_empty() && s.trim() == s,
            "local narration identity must be canonical"
        );
    }
    for s in [&r.audio_sha256, &r.narration_text_sha256] {
        ensure!(
            s.len() == 64
                && s.bytes()
                    .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
            "audio/text SHA256 must be canonical"
        );
    }
    ensure!(
        command.record_id.as_deref() == Some(&r.project_id),
        "narration routing conflicts with project"
    );
    Ok((r, payload))
}
fn scoped(
    root: &Path,
    conn: &Connection,
    actor: &str,
    r: &wire::LocalNarrationRequest,
) -> anyhow::Result<wire::Meeting> {
    ensure!(
        store::existing_instance_id(root)? == r.instance_id,
        "narration belongs to another native instance"
    );
    let m = super::jour_fixe_owner::owned(conn, actor, Some(&r.project_id), &r.meeting_id)?;
    ensure!(
        m.deck_revision == r.deck_revision
            && matches!(
                m.state,
                wire::MeetingState::Preparing
                    | wire::MeetingState::Ready
                    | wire::MeetingState::Live
            ),
        "narration deck is stale or unavailable"
    );
    let slide = m
        .slides
        .iter()
        .find(|s| s.id == r.slide_id)
        .context("narration slide missing")?;
    ensure!(
        slide.meeting_id == m.id
            && slide.body_markdown.len() <= 4096
            && !slide.body_markdown.trim().is_empty()
            && sha(slide.body_markdown.as_bytes()) == r.narration_text_sha256,
        "narration text differs from the stored slide body"
    );
    let role: String = conn.query_row(
        "SELECT role FROM business_users WHERE user_id=?1 AND active=1",
        [actor],
        |v| v.get(0),
    )?;
    for collection in ["desktop_files", "desktop_file_chunks"] {
        for permission in [
            super::super::policy::BusinessOsPermission::DataRead,
            super::super::policy::BusinessOsPermission::DataWrite,
        ] {
            ensure!(
                store_policy::trusted_actor_policy_decision_with_conn(
                    conn,
                    actor,
                    &role,
                    permission,
                    super::super::policy::BusinessOsScopeType::Collection,
                    Some(collection)
                )?
                .allowed,
                "native file custody policy denied"
            );
        }
    }
    Ok(m)
}
fn deleted(v: &Value) -> bool {
    ["_deleted", "deleted", "is_deleted"]
        .iter()
        .any(|k| v[*k].as_bool() == Some(true))
}
/// One read-only SQLite snapshot, bounded before allocating or decoding content.
fn uploaded_wav(
    root: &Path,
    policy: &Connection,
    owner: &str,
    r: &wire::LocalNarrationRequest,
) -> anyhow::Result<Vec<u8>> {
    let mut db = Connection::open_with_flags(
        store::rxdb_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = db.transaction()?;
    let (raw, tombstone): (String, bool) = tx.query_row(
        "SELECT data,deleted FROM ctox_business_os__desktop_files__v0 WHERE id=?1",
        [&r.file_id],
        |v| Ok((v.get(0)?, v.get(1)?)),
    )?;
    ensure!(
        raw.len() <= 16384 && !tombstone,
        "uploaded audio metadata unavailable"
    );
    let doc: Value = serde_json::from_str(&raw)?;
    ensure!(
        !deleted(&doc)
            && doc["id"] == r.file_id
            && doc["kind"] == "file"
            && doc["content_state"] == "available",
        "uploaded audio unavailable"
    );
    let file_owner = doc["owner_id"]
        .as_str()
        .context("uploaded audio requires an explicit owner")?;
    ensure!(
        workjet_identity::owner_from_connection(policy, file_owner)? == owner,
        "uploaded audio belongs to another owner"
    );
    let size = doc["size_bytes"]
        .as_u64()
        .context("uploaded audio size missing")?;
    ensure!(
        (44..=MAX_AUDIO as u64).contains(&size)
            && doc["content_hash"] == r.audio_sha256
            && doc["content_hash_scheme"] == "sha256-bytes-v1"
            && doc["content_generation_id"] == r.generation_id,
        "uploaded audio generation/hash/size differs"
    );
    let count = (size.div_ceil(3) * 4).div_ceil(CHUNK as u64);
    let mut statement = tx.prepare(
        "SELECT data,deleted FROM ctox_business_os__desktop_file_chunks__v0
  WHERE json_extract(data,'$.file_id')=?1 AND json_extract(data,'$.generation_id')=?2 LIMIT 685",
    )?;
    let mut rows = statement.query(params![r.file_id, r.generation_id])?;
    let mut parts = std::collections::BTreeMap::new();
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        let tombstone: bool = row.get(1)?;
        ensure!(
            !tombstone && raw.len() <= CHUNK + 4096,
            "uploaded audio chunk unavailable/oversized"
        );
        let v: Value = serde_json::from_str(&raw)?;
        ensure!(
            !deleted(&v) && v["encoding"] == "base64" && v["total"].as_u64() == Some(count),
            "uploaded audio chunk binding differs"
        );
        let idx = v["idx"].as_u64().context("audio chunk index missing")?;
        let data = v["data"].as_str().context("audio chunk bytes missing")?;
        ensure!(
            idx < count
                && data.len() <= CHUNK
                && v["chunk_hash_scheme"] == "sha256-base64-chunk-v1"
                && v["chunk_hash"] == sha(data.as_bytes()),
            "audio chunk hash/index invalid"
        );
        ensure!(
            parts.insert(idx, data.to_owned()).is_none(),
            "duplicate audio chunk index"
        );
    }
    ensure!(parts.len() as u64 == count, "uploaded audio incomplete");
    let encoded = parts.into_values().collect::<String>();
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    ensure!(
        bytes.len() as u64 == size && sha(&bytes) == r.audio_sha256,
        "uploaded audio content hash differs"
    );
    Ok(bytes)
}
/// Duration is computed from RIFF/WAVE PCM frames, never caller metadata.
pub(super) fn wav_duration(bytes: &[u8]) -> anyhow::Result<u64> {
    ensure!(
        bytes.len() >= 44 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "local narration requires RIFF/WAVE audio"
    );
    let u32at = |p: usize| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
    ensure!(
        u32at(4).checked_add(8) == Some(bytes.len()),
        "WAV size differs"
    );
    let mut offset = 12usize;
    let mut fmt = None;
    let mut data = None;
    while offset < bytes.len() {
        ensure!(bytes.len() - offset >= 8, "WAV chunk header truncated");
        let n = u32at(offset + 4);
        let start = offset + 8;
        let end = start.checked_add(n).context("WAV chunk overflow")?;
        ensure!(end <= bytes.len(), "WAV chunk truncated");
        if &bytes[offset..offset + 4] == b"fmt " {
            ensure!(
                fmt.is_none() && (16..=64).contains(&n),
                "WAV format invalid/duplicated"
            );
            let u16at = |p: usize| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap()) as usize;
            let format = u16at(start);
            let channels = u16at(start + 2);
            let rate = u32at(start + 4);
            let byte_rate = u32at(start + 8);
            let block = u16at(start + 12);
            let bits = u16at(start + 14);
            ensure!(
                (format == 1 && [16, 24, 32].contains(&bits) || format == 3 && bits == 32)
                    && [1, 2].contains(&channels)
                    && [16000, 22050, 24000, 44100, 48000].contains(&rate)
                    && block == channels * bits / 8
                    && byte_rate == rate * block,
                "unsupported WAV encoding"
            );
            fmt = Some((rate, block));
        } else if &bytes[offset..offset + 4] == b"data" {
            ensure!(data.replace(n).is_none(), "WAV data duplicated");
        }
        offset = end.checked_add(n % 2).context("WAV padding overflow")?;
        ensure!(offset <= bytes.len(), "WAV padding truncated");
    }
    let (rate, block) = fmt.context("WAV format missing")?;
    let n = data.context("WAV data missing")?;
    ensure!(n > 0 && n % block == 0, "WAV frames incomplete");
    let ms = (n as u64 / block as u64) * 1000 / rate as u64;
    ensure!(
        (1..=300000).contains(&ms),
        "WAV narration exceeds duration budget"
    );
    Ok(ms)
}
fn persist(
    tx: &Connection,
    collection: &str,
    id: &str,
    now: i64,
    doc: Value,
) -> anyhow::Result<DomainRecordRef> {
    tx.execute("INSERT INTO business_records(collection,record_id,rev,deleted,updated_at_ms,payload_json) VALUES (?1,?2,?3,0,?4,?5)",
  params![collection,id,format!("local-audio-{}",sha(serde_json::to_vec(&doc)?.as_slice())),now,doc.to_string()])?;
    Ok(DomainRecordRef {
        collection: collection.to_owned(),
        id: id.to_owned(),
    })
}
pub(in crate::business_os) fn validate_recovery_scope(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    actor: &str,
) -> anyhow::Result<()> {
    let (r, _) = parse(command)?;
    scoped(root, conn, actor, &r)?;
    Ok(())
}
pub(in crate::business_os) fn handle(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: &DomainEffectAdmission,
) -> anyhow::Result<Value> {
    let (r, payload) = parse(command)?;
    let intent = sha(&serde_json::to_vec(&payload)?);
    let mut conn = open_store(root)?;
    let m = scoped(root, &conn, actor, &r)?;
    conn.execute_batch(SCHEMA)?;
    // Original operation replays do not need the mutable upload. Native custody
    // was committed alongside its receipt, and recovery republishes that copy.
    let old:Option<(String,String,String)>=conn.query_row("SELECT owner_user_id,intent_hash,receipt_json FROM workjet_jour_fixe_local_narration WHERE operation_id=?1",[&r.operation_id],|v|Ok((v.get(0)?,v.get(1)?,v.get(2)?))).optional()?;
    let audio = if old.is_none() {
        Some(uploaded_wav(root, &conn, &m.owner_user_id, &r)?)
    } else {
        None
    };
    let duration = audio.as_ref().map(|v| wav_duration(v)).transpose()?;
    let applied=admission.apply(&mut conn,|tx| {
  let mut meeting=scoped(root,tx,actor,&r)?;
  let old:Option<(String,String,String,String)>=tx.query_row("SELECT owner_user_id,intent_hash,receipt_json,projections_json FROM workjet_jour_fixe_local_narration WHERE operation_id=?1",[&r.operation_id],|v|Ok((v.get(0)?,v.get(1)?,v.get(2)?,v.get(3)?))).optional()?;
  if let Some((owner,hash,result,projections))=old {
   ensure!(owner==meeting.owner_user_id && hash==intent,"local narration operation intent conflicts");
   return Ok(AppliedDomainEffect{result:serde_json::from_str(&result)?,projections:serde_json::from_str(&projections)?});
  }
  ensure!(meeting.state==wire::MeetingState::Preparing && meeting.revision==r.expected_revision,"narration requires the current preparing deck revision");
  let slide=meeting.slides.iter_mut().find(|s|s.id==r.slide_id).context("slide missing")?;
  ensure!(slide.audio.is_none(),"slide already narrated; reuse original operation or prepare a new deck");
  let bytes=audio.as_ref().context("local audio custody missing")?; let now=chrono::Utc::now().timestamp_millis();
  let file_id=stable_id("workjet_audio",&[&r.instance_id,&meeting.owner_user_id,&meeting.id,&r.deck_revision.to_string(),&r.slide_id,&r.operation_id,&r.audio_sha256]);
  let generation=stable_id("audio_generation",&[&file_id,&r.audio_sha256]);
  let encoded=base64::engine::general_purpose::STANDARD.encode(bytes);
  let total=encoded.len().div_ceil(CHUNK); let mut projections=Vec::with_capacity(total+1);
  projections.push(persist(tx,"desktop_files",&file_id,now,json!({"id":file_id,"name":format!("{}.wav",r.slide_id),"kind":"file","mime_type":"audio/wav","extension":"wav","size_bytes":bytes.len(),"owner_id":meeting.owner_user_id,"source":"ctox-jour-fixe-local-audio","linked_collection":"workjet_jour_fixe_meetings","linked_record_id":meeting.id,"content_state":"available","content_hash":r.audio_sha256,"content_hash_scheme":"sha256-bytes-v1","content_generation_id":generation,"is_deleted":false,"created_at_ms":now,"updated_at_ms":now}))?);
  for (idx,chunk) in encoded.as_bytes().chunks(CHUNK).enumerate() {
   let data=std::str::from_utf8(chunk)?; let id=format!("{file_id}_{generation}_{idx:06}");
   projections.push(persist(tx,"desktop_file_chunks",&id,now,json!({"id":id,"file_id":file_id,"generation_id":generation,"content_hash":r.audio_sha256,"content_hash_scheme":"sha256-bytes-v1","idx":idx,"total":total,"encoding":"base64","data":data,"chunk_hash":sha(data.as_bytes()),"chunk_hash_scheme":"sha256-base64-chunk-v1","size_bytes":data.len(),"created_at_ms":now}))?);
  }
  let reference=wire::AudioRef{file_id,sha256:r.audio_sha256.clone(),mime_type:"audio/wav".into(),duration_ms:duration.context("duration missing")?,narration_text_sha256:r.narration_text_sha256.clone(),source_run_id:r.operation_id.clone(),model:"owner-uploaded-local-audio".into(),format:"wav".into(),synthesis_duration_ms:0,provenance:Some(wire::AudioProvenance::AuthenticatedOwnerLocalAudio),generation_id:Some(generation)};
  reference.validate().map_err(anyhow::Error::msg)?; slide.audio=Some(reference.clone());
  if meeting.slides.iter().all(|s|s.audio.is_some()) { meeting.state=wire::MeetingState::Ready; }
  meeting.revision=meeting.revision.checked_add(1).context("meeting revision overflow")?;
  meeting.validate().map_err(anyhow::Error::msg)?;
  let raw=serde_json::to_string(&meeting)?; ensure!(raw.len()<=1024*1024,"meeting metadata exceeds native budget");
  let receipt=wire::LocalNarrationReceipt{operation_id:r.operation_id.clone(),instance_id:r.instance_id.clone(),project_id:meeting.project_id.clone(),meeting_id:meeting.id.clone(),slide_id:r.slide_id.clone(),deck_revision:meeting.deck_revision,owner_user_id:meeting.owner_user_id.clone(),revision:meeting.revision,audio:reference,persisted_at_ms:now,provenance:wire::AudioProvenance::AuthenticatedOwnerLocalAudio,provider_verified:false};
  receipt.validate().map_err(anyhow::Error::msg)?;
  let mutation=wire::MeetingMutationReceipt{operation_id:r.operation_id.clone(),meeting_id:meeting.id.clone(),project_id:meeting.project_id.clone(),revision:meeting.revision,state:meeting.state,changed_id:Some(r.slide_id.clone()),todos_revision:None};
  let result=json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"owner_user_id":meeting.owner_user_id,"mutation":mutation,"local_narration":receipt});
  tx.execute("UPDATE workjet_jour_fixe_meetings SET metadata_json=?2 WHERE meeting_id=?1",params![meeting.id,raw])?;
  tx.execute("INSERT INTO workjet_jour_fixe_local_narration VALUES (?1,?2,?3,?4,?5)",params![r.operation_id,meeting.owner_user_id,intent,result.to_string(),serde_json::to_string(&projections)?])?;
  Ok(AppliedDomainEffect{result,projections})
 })?;
    Ok(applied.result)
}
