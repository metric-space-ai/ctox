//! P8 reader bound to the exact app grant issued to a retained native peer.
use super::{scope, AppGrantSession};
use ctox_store::change_log::{self,subscription::{ReadAuthority,SubscriptionKey}};
use rusqlite::Transaction;
use serde_json::Value;
use std::sync::Arc;
fn invalid(e:impl Into<String>)->rusqlite::Error {rusqlite::Error::InvalidParameterName(e.into())}
pub struct AppGrantReader {
    session:Arc<AppGrantSession>,key:SubscriptionKey,token:String,
    preferences:Option<ctox_store::preferences::Context>,
    clock:fn()->u64,
}
impl AppGrantReader {
    /// Clock is the host's seconds clock; no caller-supplied identity or policy.
    pub fn new(session:Arc<AppGrantSession>,key:SubscriptionKey,clock:fn()->u64)->scope::Result<Self> {
        let token=session.bound_token(Some(&key.scope_hash),&key.collection,key.gen,None)?;
        let claims=session.issued_claims()?.into_iter().find(|c|c.scope_hash==key.scope_hash).ok_or("revoked")?;
        if key.participant!=claims.user_id {return Err("foreign participant".into());}
        let preferences=if key.collection==ctox_store::preferences::COLLECTION {
            Some(ctox_store::preferences::Context::authenticated(&claims.app_id,&claims.user_id,claims.rights_ver.parse().map_err(|_|"invalid rights version")?).map_err(str::to_owned)?)
        } else {None};
        Ok(Self{session,key,token,preferences,clock})
    }
    fn scope(&self,tx:&Transaction<'_>,key:&SubscriptionKey)->rusqlite::Result<scope::ResolvedScope> {
        if key!=&self.key {return Err(invalid("subscription binding"));}
        self.session.scope_in_transaction(tx,&self.token,&key.collection,(self.clock)()).map_err(invalid)
    }
}
impl ReadAuthority for AppGrantReader {
    fn preference_context(&self,participant:&str)->Option<ctox_store::preferences::Context> {
        (participant==self.key.participant).then(||self.preferences.clone()).flatten()
    }
    fn authorize_preference_scope(&self,tx:&Transaction<'_>,key:&SubscriptionKey,_:&ctox_store::preferences::Context)->rusqlite::Result<()> {self.scope(tx,key).map(|_|())}
    fn authorize(&self,tx:&Transaction<'_>,key:&SubscriptionKey)->rusqlite::Result<()> {self.scope(tx,key).map(|_|())}
    fn project(&self,tx:&Transaction<'_>,key:&SubscriptionKey,doc:&change_log::ChangeDocument)->rusqlite::Result<Option<Value>> {
        let scope=self.scope(tx,key)?;
        match scope::scope_filter::project(scope.projection(&key.collection),&key.collection,&doc.id,doc.seq,doc.body.as_ref()).map_err(invalid)? {
            scope::scope_filter::ProjectedChange::Put{doc,..}=>Ok(Some(doc)),
            scope::scope_filter::ProjectedChange::Unproject{..}=>Ok(None),
        }
    }
}
