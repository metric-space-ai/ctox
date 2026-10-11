// Origin: CTOX; License: AGPL-3.0-only
use super::*;
use crate::sdk::cliproxy::auth::{AccountCandidate, AccountRouter, AccountRoutingError, AccountSessionStore,
    CooldownStateRecord, CooldownStateStore, CooldownStoreError, SchedulerPickOptions};
use crate::internal::api::account_selection::{self, ACCOUNT_SELECTION};
use crate::internal::runtime::executor::helps::claude_quota_observation;
use std::sync::{Arc,Mutex};
#[derive(Default)]
struct Quotas(Mutex<BTreeMap<String,ReportedQuotaSnapshot>>);
impl AccountQuotaStore for Quotas {
    fn load(&self, _: &str, ids: &[String]) -> Result<BTreeMap<String,ReportedQuotaSnapshot>,AccountQuotaStoreError> {
        Ok(self.0.lock().unwrap().iter().filter(|(id,_)|ids.contains(id)).map(|(id,q)|(id.clone(),q.clone())).collect())
    }
    fn save(&self,_: &str,id:&str,q:&ReportedQuotaSnapshot)->Result<(),AccountQuotaStoreError>{
        q.validate()?;self.0.lock().unwrap().insert(id.into(),q.clone());Ok(())
    }
}
#[derive(Default)]
struct Sessions(Mutex<BTreeMap<String,String>>);
impl AccountSessionStore for Sessions {
    fn choose(&self,key:&str,pick:&mut dyn FnMut(Option<&str>)->Result<String,AccountRoutingError>)->Result<String,AccountRoutingError>{
        let mut entries=self.0.lock().unwrap();let id=pick(entries.get(key).map(String::as_str))?;entries.insert(key.into(),id.clone());Ok(id)
    }
}
#[derive(Default)]
struct Cooldowns(Mutex<Vec<CooldownStateRecord>>);
impl CooldownStateStore for Cooldowns {
    fn load(&self)->Result<Vec<CooldownStateRecord>,CooldownStoreError>{Ok(self.0.lock().unwrap().clone())}
    fn save(&self,v:&[CooldownStateRecord])->Result<(),CooldownStoreError>{*self.0.lock().unwrap()=v.to_vec();Ok(())}
}
fn snapshot(reset:i64, used:Option<f64>)->ReportedQuotaSnapshot{
    ReportedQuotaSnapshot{observed_at_ms:1_000,windows:vec![ReportedQuotaWindow{
        kind:QuotaWindowKind::FiveHour,status:ReportedQuotaStatus::Allowed,reset_at_ms:reset,utilization:used,
    }]}
}
fn accounts()->Vec<AccountCandidate>{
    ["account-a","account-b","account-c"].into_iter().enumerate().map(|(i,id)|AccountCandidate{
        auth_id:id.into(),provider:"claude".into(),priority:100-i as i32,weight:1,..Default::default()
    }).collect()
}
async fn pick(router:&AccountRouter,session:&str,now:i64,accounts:&[AccountCandidate])->String{
    ACCOUNT_SELECTION.scope(Mutex::new(Default::default()),async{
        account_selection::request_session(&BTreeMap::new(),&serde_json::to_vec(&serde_json::json!({"session_id":session})).unwrap());
        router.select("claude",None,now,accounts).unwrap().auth_id
    }).await
}
#[tokio::test]
async fn provider_quota_standard_prefers_earliest_reported_usable_reset_and_never_round_robins(){
    let quotas=Arc::new(Quotas::default());
    quotas.save("claude","account-b",&snapshot(9_000,Some(0.1))).unwrap();
    quotas.save("claude","account-c",&snapshot(5_000,Some(0.9))).unwrap();
    let router=AccountRouter::with_account_state_stores(Arc::new(Cooldowns::default()),Arc::new(Sessions::default()),quotas);
    let accounts=accounts();
    for session in ["one","two","three"]{assert_eq!(pick(&router,session,2_000,&accounts).await,"account-c");}
}
#[tokio::test]
async fn provider_quota_changed_windows_do_not_move_a_healthy_session_but_failover_uses_them(){
    let quotas=Arc::new(Quotas::default());let sessions=Arc::new(Sessions::default());
    let router=AccountRouter::with_account_state_stores(Arc::new(Cooldowns::default()),sessions.clone(),quotas.clone());
    let mut accounts=accounts();
    assert_eq!(pick(&router,"ongoing",2_000,&accounts).await,"account-a");
    quotas.save("claude","account-b",&snapshot(5_000,None)).unwrap();
    quotas.save("claude","account-c",&snapshot(9_000,Some(0.2))).unwrap();
    assert_eq!(pick(&router,"ongoing",2_000,&accounts).await,"account-a");
    assert_eq!(pick(&router,"new",2_000,&accounts).await,"account-b");
    accounts[0].disabled=true;
    assert_eq!(pick(&router,"ongoing",2_000,&accounts).await,"account-b");
    accounts[0].disabled=false;
    quotas.save("claude","account-c",&snapshot(4_000,Some(0.2))).unwrap();
    let restored=AccountRouter::with_account_state_stores(Arc::new(Cooldowns::default()),sessions,quotas);
    assert_eq!(pick(&restored,"ongoing",2_000,&accounts).await,"account-b");
}
#[tokio::test]
async fn provider_quota_expired_full_rejected_and_unknown_reports_do_not_invent_capacity(){
    let quotas=Arc::new(Quotas::default());
    quotas.save("claude","account-b",&snapshot(5_000,Some(1.0))).unwrap();
    let mut rejected=snapshot(6_000,Some(0.5));rejected.windows[0].status=ReportedQuotaStatus::Rejected;
    quotas.save("claude","account-c",&rejected).unwrap();
    let router=AccountRouter::with_account_state_stores(Arc::new(Cooldowns::default()),Arc::new(Sessions::default()),quotas.clone());
    assert_eq!(pick(&router,"full",2_000,&accounts()).await,"account-a");
    quotas.save("claude","account-b",&snapshot(3_000,Some(0.2))).unwrap();
    assert_eq!(pick(&router,"expired",4_000,&accounts()).await,"account-a");
    assert_eq!(snapshot(3_000,Some(0.2)).usable_reset_at(500),None);
}
#[tokio::test]
async fn provider_quota_disabled_and_explicitly_pinned_accounts_preserve_existing_authority(){
    let quotas=Arc::new(Quotas::default());quotas.save("claude","account-b",&snapshot(3_000,None)).unwrap();
    quotas.save("claude","account-c",&snapshot(4_000,None)).unwrap();
    let router=AccountRouter::with_account_state_stores(Arc::new(Cooldowns::default()),Arc::new(Sessions::default()),quotas);
    let mut accounts=accounts();accounts[1].disabled=true;
    assert_eq!(pick(&router,"new",2_000,&accounts).await,"account-c");
    let options=SchedulerPickOptions{pinned_auth_id:Some("account-a".into()),..Default::default()};
    assert_eq!(router.select_with_options("claude",None,2_000,&accounts,&options).unwrap().auth_id,"account-a");
}
fn headers(pairs:&[(&str,&str)])->crate::sdk::cliproxy::executor::Headers{
    pairs.iter().map(|(k,v)|(format!("aNtHrOpIc-RaTeLiMiT-uNiFiEd-{k}"),vec![v.to_string()])).collect()
}
#[test]
fn provider_quota_parser_preserves_reported_shared_windows_and_missing_percentage(){
    let h=headers(&[("5h-Status","allowed_warning"),("5h-Reset","5"),("5h-Utilization","0.75"),("7d-Status","allowed"),("7d-Reset","1970-01-01T00:00:09Z")]);
    let q=claude_quota_observation(&h,1_000).unwrap();
    assert_eq!(q.windows.len(),2);assert_eq!(q.windows[0].utilization,Some(0.75));
    assert_eq!(q.windows[1].utilization,None);assert_eq!(q.usable_reset_at(2_000),Some(5_000));
}
#[test]
fn provider_quota_parser_never_converts_billing_or_retry_headers_into_usable_quota(){
    for h in [BTreeMap::new(),headers(&[("Status","allowed"),("Reset","5"),("Overage-Reset","5"),("Overage-Status","allowed")]),
        BTreeMap::from([("retry-after".into(),vec!["60".into()])]),headers(&[("5h-Reset","5"),("5h-Utilization","0.5")])]{
        assert_eq!(claude_quota_observation(&h,1_000),None);
    }
}
#[test]
fn provider_quota_parser_rejects_invalid_or_stale_window_fields(){
    for (reset,used) in [("garbage","0.5"),("-1","0.5"),("1","0.5"),("NaN","0.5"),("9999999999999","0.5"),("5","-0.1"),("5","1.1"),("5","NaN"),("5","inf")]{
        assert_eq!(claude_quota_observation(&headers(&[("5h-Status","allowed"),("5h-Reset",reset),("5h-Utilization",used)]),1_000),None);
    }
}
#[test]
fn provider_quota_snapshot_wire_preserves_unknown_and_rejects_duplicate_windows(){
    let q=snapshot(5_000,None);let decoded:ReportedQuotaSnapshot=serde_json::from_slice(&serde_json::to_vec(&q).unwrap()).unwrap();
    assert_eq!(decoded,q);assert_eq!(decoded.windows[0].utilization,None);
    let mut invalid=q.clone();invalid.windows.push(q.windows[0].clone());assert!(invalid.validate().is_err());
}
