// Origin: CTOX
// License: AGPL-3.0-only
//! Holder-private API model-check reservation. No credential value is retained.
use crate::business_os::{consumer_authority::{AdmittedConsumerAuthority,ConsumerFacts},provider_federation::{capture_consumable_model,ConsumableModel}};
use super::cliproxyapi_integration::{load_provider_integration_config,ApiKeyAccountConfig,ProviderIntegrationConfig,PROVIDER_INTEGRATION_CONFIG_TABLE};
use anyhow::{ensure,Context,Result};
use rusqlite::{Connection,OpenFlags,OptionalExtension,Transaction,TransactionBehavior};
use sha2::{Digest,Sha256};
use std::{path::{Path,PathBuf},sync::Mutex};

pub(crate) struct NativeApiModelReservation {root:PathBuf,pub(super) selected:ConsumableModel,account:ApiKeyAccountConfig,encrypted:String,active:Mutex<bool>}
pub(crate) struct NativeApiPublicationCheck {account:ApiKeyAccountConfig,encrypted:String,runtime:Mutex<Connection>,secrets:Mutex<Connection>}
fn encrypted(conn:&Connection,account:&ApiKeyAccountConfig)->Result<String> {
    let key=&account.api_key_secret;
    let row:Option<(String,String)>=conn.query_row("SELECT nonce_b64,ciphertext_b64 FROM ctox_secret_records WHERE scope=?1 AND secret_name=?2",rusqlite::params![key.scope,key.name],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (nonce,ciphertext)=row.context("native API credential unavailable")?;let mut digest=Sha256::new();digest.update(b"ctox/native-api-encrypted-record/v1");
    for value in [key.scope.as_str(),key.name.as_str(),nonce.as_str(),ciphertext.as_str()] {digest.update((value.len() as u64).to_be_bytes());digest.update(value.as_bytes());}Ok(format!("{:x}",digest.finalize()))
}
fn account(conn:&Connection,id:&str)->Result<ApiKeyAccountConfig> {
    let json:String=conn.query_row(&format!("SELECT config_json FROM {PROVIDER_INTEGRATION_CONFIG_TABLE} WHERE config_id=1"),[],|r|r.get(0)).optional()?.context("native API configuration unavailable")?;
    let cfg:ProviderIntegrationConfig=serde_json::from_str(&json).map_err(|_|anyhow::anyhow!("native API configuration unavailable"))?;
    cfg.api_key_accounts.into_iter().find(|a|a.id==id).context("native API account removed")
}
impl NativeApiPublicationCheck {
    fn open(root:&Path,expected:&ApiKeyAccountConfig)->Result<Self> {
        let runtime=Connection::open_with_flags(crate::inference::runtime_env::runtime_config_path(root),OpenFlags::SQLITE_OPEN_READ_WRITE|OpenFlags::SQLITE_OPEN_NO_MUTEX)?;runtime.busy_timeout(std::time::Duration::ZERO)?;
        ensure!(account(&runtime,&expected.id)?==*expected,"native API configuration changed");
        let secrets=Connection::open_with_flags(crate::secrets::resolve_db_path(root),OpenFlags::SQLITE_OPEN_READ_ONLY)?;secrets.busy_timeout(std::time::Duration::ZERO)?;
        let encrypted=encrypted(&secrets,expected)?;Ok(Self{account:expected.clone(),encrypted,runtime:Mutex::new(runtime),secrets:Mutex::new(secrets)})
    }
    pub(crate) fn prepare(reservation:&NativeApiModelReservation)->Result<Self> {
        let check=Self::open(&reservation.root,&reservation.account)?;ensure!(check.encrypted==reservation.encrypted,"native API credential changed");Ok(check)
    }
    fn records<T>(&self,publish:impl FnOnce()->Result<T>)->Result<T> {
        let runtime=self.runtime.try_lock().map_err(|_|anyhow::anyhow!("native API configuration busy"))?;let runtime=Transaction::new_unchecked(&runtime,TransactionBehavior::Immediate)?;
        ensure!(account(&runtime,&self.account.id)?==self.account,"native API configuration changed");
        let secrets=self.secrets.try_lock().map_err(|_|anyhow::anyhow!("native API credential metadata busy"))?;
        ensure!(encrypted(&secrets,&self.account)?==self.encrypted,"native API credential changed");publish()
    }
    /// Only the actual controller's held issuer/Core/Policy scope may call this.
    /// Both connections are preopened; no secret decryption or transport reentry.
    pub(crate) fn with_current_in_held_policy<T>(&self,reservation:&NativeApiModelReservation,facts:&ConsumerFacts,policy:&Connection,publish:impl FnOnce()->Result<T>)->Result<T> {
        let mut active=reservation.active.try_lock().map_err(|_|anyhow::anyhow!("native API reservation busy"))?;
        let result=(||{ensure!(*active,"native API reservation released");ensure!(self.account==reservation.account && self.encrypted==reservation.encrypted,"native API prepared account changed");
            ensure!(self.account.owner_user_id.as_deref()==Some(facts.owner_user_id.as_str()),"native API account is not owned");
            reservation.selected.assert_current_in_policy(facts,policy)?;self.records(publish)})();if result.is_err(){*active=false;}result
    }
}
impl NativeApiModelReservation {
    pub(crate) fn prepare_model_check(authority:&AdmittedConsumerAuthority,id:&str,revision:i64,model:&str)->Result<Self> {
        let selected=capture_consumable_model(authority,id,revision,model)?;
        ensure!(matches!(selected.account().provider.as_str(),"zai"|"minimax"|"xai"|"kimi") && selected.account().holder_instance_id==authority.native_host_instance_id()?,"model check requires an API account held on this native instance");
        let binding=selected.private_configuration_binding().context("native API account requires current holder discovery")?;ensure!(binding.starts_with("api-key:v1:"),"model check requires native API binding");
        let root=authority.native_host_root().to_owned();let stored=load_provider_integration_config(&root)?;
        let account=stored.config.api_key_accounts.into_iter().find(|a|a.id==selected.account().private_local_account_id && a.endpoint.provider()==selected.account().provider).context("native API account unavailable")?;account.validate()?;ensure!(!account.disabled && account.owner_user_id.is_some() && account.creation_operation.is_some(),"native API account is not enabled and owned");
        ensure!(super::cliproxyapi_host::account_controls::account_binding(&root,account.endpoint.provider(),&account.id)?.as_deref()==Some(binding),"native API binding changed");
        let check=NativeApiPublicationCheck::open(&root,&account)?;ensure!(super::cliproxyapi_host::account_controls::account_binding(&root,account.endpoint.provider(),&account.id)?.as_deref()==Some(binding),"native API binding changed during preparation");let reservation=Self{root,selected,account,encrypted:check.encrypted.clone(),active:Mutex::new(true)};reservation.with_current(authority,||Ok(()))?;Ok(reservation)
    }
    pub(crate) fn with_current<T>(&self,authority:&AdmittedConsumerAuthority,publish:impl FnOnce()->Result<T>)->Result<T> {
        ensure!(self.root==authority.native_host_root(),"native API source host changed");let private=NativeApiPublicationCheck::prepare(self)?;
        authority.with_current(|facts,policy|private.with_current_in_held_policy(self,facts,policy,publish))
    }
    pub(crate) fn release(&self) {*self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=false;}
}

#[cfg(test)]
#[path="cliproxyapi_api_model_tests.rs"]
mod tests;
