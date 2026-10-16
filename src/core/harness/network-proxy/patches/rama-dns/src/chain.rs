use std::net::{Ipv4Addr, Ipv6Addr};

use rama_core::error::{BoxError, OpaqueError};
use rama_net::address::Domain;

use crate::DnsResolver;

macro_rules! dns_resolver_chain_impl {
    () => {
        async fn txt_lookup(&self, domain: Domain) -> Result<Vec<Vec<u8>>, Self::Error> {
            let mut last_err = None;
            for resolver in self {
                match resolver.txt_lookup(domain.clone()).await {
                    Ok(values) => return Ok(values),
                    Err(err) => last_err = Some(err.into()),
                }
            }
            Err(last_err.unwrap_or_else(|| {
                OpaqueError::from_display("unknown dns error (erorr missing)").into_boxed()
            }))
        }

        async fn ipv4_lookup(&self, domain: Domain) -> Result<Vec<Ipv4Addr>, Self::Error> {
            let mut last_err = None;
            for resolver in self {
                match resolver.ipv4_lookup(domain.clone()).await {
                    Ok(ipv4s) => return Ok(ipv4s),
                    Err(err) => last_err = Some(err.into()),
                }
            }
            Err(last_err.unwrap_or_else(|| {
                OpaqueError::from_display("unknown dns error (erorr missing)").into_boxed()
            }))
        }

        async fn ipv6_lookup(&self, domain: Domain) -> Result<Vec<Ipv6Addr>, Self::Error> {
            let mut last_err = None;
            for resolver in self {
                match resolver.ipv6_lookup(domain.clone()).await {
                    Ok(ipv6s) => return Ok(ipv6s),
                    Err(err) => last_err = Some(err.into()),
                }
            }
            Err(last_err.unwrap_or_else(|| {
                OpaqueError::from_display("unknown dns error (erorr missing)").into_boxed()
            }))
        }
    };
}

impl<R> DnsResolver for Vec<R>
where
    R: DnsResolver + Send,
{
    type Error = BoxError;

    dns_resolver_chain_impl!();
}

impl<R, const N: usize> DnsResolver for [R; N]
where
    R: DnsResolver + Send,
{
    type Error = BoxError;

    dns_resolver_chain_impl!();
}
