#!/usr/bin/env python3
"""Bounded mutation proof, run only by qualify.sh on its isolated lane copy."""
import hashlib,json,os,pathlib,subprocess,sys
source=pathlib.Path.cwd(); store=pathlib.Path(sys.argv[1]); proof=pathlib.Path(os.environ['IMPORT_PROOF_DIR'])
rows=[]
def mutate(path,old,new,test):
    before=path.read_text(); assert before.count(old)==1,(path,old)
    line=before[:before.index(old)].count('\n')+1
    try:
        path.write_text(before.replace(old,new))
        result=subprocess.run(['cargo','test','--manifest-path',str(store/'Cargo.toml'),'--lib',test,'--','--include-ignored','--nocapture'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=120)
        log_name = str(len(rows)+1)+'-'+test.split('::')[-1]+'.mutant.log'
        (proof/log_name).write_text(result.stdout)

        # Compile errors or zero selection are not a demonstrated invariant.
        failed=result.returncode!=0 and 'test result: FAILED' in result.stdout and 'error[E' not in result.stdout
        rows.append({'file':str(path.relative_to(source)),'line':line,'change':old+' -> '+new,'test':test,'exit':result.returncode,'detected':failed,'log':log_name})
        assert failed, test+' mutation did not fail its assertion'
    finally:
        path.write_text(before)
        assert hashlib.sha256(path.read_bytes()).hexdigest()==hashlib.sha256(before.encode()).hexdigest()
try:
    mutate(source/'src/core/store/import/mod.rs','counts.source_rows != counts.target_documents','false','import::tests::files::copied_profile_files_tracking_and_counting_are_complete_and_replay_is_seq_neutral')
    mutate(source/'src/core/store/import/file_import.rs','total != chunks.len() as u64','false','import::tests::files::incomplete_chunks_and_escaping_paths_never_reach_cas')
    mutate(source/'src/core/store/import/core_import.rs','t.checked_mul(1000)','t.checked_mul(1)','import::tests::files::copied_profile_files_tracking_and_counting_are_complete_and_replay_is_seq_neutral')
    mutate(source/'src/core/store/import/core_import.rs','campaigns.get(token).cloned().unwrap_or_default()','String::new()','import::tests::files::copied_profile_files_tracking_and_counting_are_complete_and_replay_is_seq_neutral')
    mutate(source/'src/core/store/import/file_import.rs','source.owner_user.clone()','None','import::tests::files::copied_profile_files_tracking_and_counting_are_complete_and_replay_is_seq_neutral')
    mutate(source/'src/core/store/import/mod.rs','return Ok(false);','sql(tx.execute(&format!("UPDATE {table} SET data=json_set(data,\'$.title\',\'mutant\'),rev=rev || \'-mutant\' WHERE id=?1"), [&doc.id]))?; return Ok(false);','import::tests::publication_replay_and_core_import_are_seq_neutral_and_never_overwrite_app_edits')
    mutate(source/'src/core/store/import/mod.rs','return Ok(false);','return Ok(true);','import::tests::publication_replay_and_core_import_are_seq_neutral_and_never_overwrite_app_edits')
    mutate(source/'src/core/store/import/core_import.rs','"username,created_at"','"username,password_hash,created_at"','import::tests::source_snapshot_maps_bindings_tickets_witnesses_and_mail_never_reads_secrets')
    mutate(source/'src/core/store/import/base_release.rs','body["id"] = id.clone().into();','body["id"] = "wrong-id".into();','import::tests::legacy::conversations_take_existing_key_and_keep_deletions_and_source_cursor')
    mutate(source/'src/core/store/import/base_release.rs','entry.get("schema").unwrap_or(entry).clone()','entry.clone()','import::tests::release_input::actual_a9_base_is_activated_through_registry_and_replay_has_no_new_seq')
finally:
    (proof/'mutation-proof.json').write_text(json.dumps(rows,indent=2)+'\n')
