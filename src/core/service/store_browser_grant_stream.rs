//! D: commit-driven per-app revocations, owned by the browser association.
use super::*;
pub(super) fn events(source: &Source, peer: &str) -> RxResult<BoxStream<'static,RxResult<Value>>> {
    let binding=source.admission.peer(peer).map_err(|_|invalid())?;
    let inputs=source.inputs.upgrade().ok_or_else(invalid)?;
    let session=scope_grant_route::bound_owner(&source.admission,&binding)?.session();
    let commits=inputs.sync.writer.subscribe_commits();
    let readers=inputs.sync.readers.clone();
    let signer=inputs.instance_signer().map_err(|_|invalid())?;
    Ok(stream::try_unfold((commits,readers,session,signer,std::collections::VecDeque::new()),
        |(mut commits,readers,session,signer,mut pending):(_,_,_,_,std::collections::VecDeque<Value>)|async move {
            loop {
                if let Some(frame)=pending.pop_front() {return Ok(Some((frame,(commits,readers,session,signer,pending))));}
                tokio::select! {
                    changed=commits.changed()=>{changed.map_err(|_|invalid())?;},
                    _=tokio::time::sleep(Duration::from_secs(240))=>{}
                }
                let owner=session.clone();
                let frames=readers.read(move |tx| {
                    let issued=owner.issued_claims().map_err(sql_invalid)?;
                    let revoked=owner.revocations_in_transaction(tx,now()/1000).map_err(sql_invalid)?;
                    let rights=sync_rights::read(tx)?;
                    let head=change_log::head(tx)?;
                    Ok(revoked.into_iter().filter_map(|frame|issued.iter().find(|c|c.app_id==frame.app_id).map(|claims|
                        serde_json::json!({"type":"scope_revoke","app_id":frame.app_id,"reason":"revoked",
                            "scope_hash":claims.scope_hash,"collections":claims.collections.iter().map(|c|c.collection.clone()).collect::<Vec<_>>(),
                            "rights_ver":rights.rights_ver.to_string(),"instance_id":claims.instance_id,"participant":claims.user_id,
                            "epoch":head.epoch,"core_sent_at_ms":now(),"remaining":[],"artifacts":[],"outbox_reason":"revoked"})
                    )).collect::<Vec<_>>())
                }).await.map_err(|_|invalid())?;
                for mut frame in frames {
                    frame["key_id"]=Value::String(signer.key_id().into());
                    let mut bytes=b"ctox.sync.v4.stream\0".to_vec();
                    bytes.extend(sync_rights::scope::canonical_json(&frame).map_err(|_|invalid())?);
                    frame["sig"]=serde_json::to_value(signer.sign(&bytes).map_err(|_|invalid())?).map_err(|_|invalid())?;
                    pending.push_back(frame);
                }
            }
        }).boxed())
}
