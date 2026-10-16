//! E25 registration adapter. Handler implementations and user checks stay in
//! their original modules. Both legacy bring-up and the V4 composition root use
//! this function once, before admitting command execution on the retained pool.
use super::*;

pub(crate) fn register_v4_native_command_handlers(
    pool: &ctox_sync::native::NativePool,
    database: &Arc<RxDatabase>,
    root: &Path,
) -> rxdb::rx_error::RxResult<()> {
    let root = root.to_path_buf();
    for route in ctox_store::change_log::subscription::write_contract::routes::ROUTES
        .iter()
        .copied()
        .filter(|r| {
            ctox_store::change_log::subscription::write_contract::routes::is_command_bus(*r)
        })
    {
        let core_root = root.clone();
        pool.register_native_command_executor(
            route.method,
            Arc::new(move |peer, token, command, authority| {
                let root = core_root.clone();
                Box::pin(async move {
                    tokio::task::spawn_blocking(move || {
                        crate::business_os::command_plane::execute_v4_registered_command(
                            &root, command, &peer, &token, authority,
                        )
                    })
                    .await
                    .map_err(|_| {
                        ctox_store::change_log::subscription::write_contract::routes::error(
                            "unavailable",
                            "native_executor_unavailable",
                        )
                    })?
                })
            }),
        )?;
    }
    let browser_live_root = root.clone();
    let browser_live_database = Arc::clone(database);
    pool.register_auxiliary_request_handler(
        BROWSER_LIVE_WEBRTC_METHOD,
        Arc::new(move |_peer, capability_token, params| {
            let root = browser_live_root.clone();
            let database = Arc::clone(&browser_live_database);
            Box::pin(async move {
                handle_browser_live_webrtc_request(&root, &database, &capability_token, params)
                    .await
            })
        }),
    )?;
    let lookup_root = root.clone();
    pool.register_auxiliary_request_handler(
        OUTBOUND_SELLIFY_LOOKUP_WEBRTC_METHOD,
        Arc::new(move |_peer, capability_token, params| {
            let root = lookup_root.clone();
            Box::pin(async move {
                if !store::capability_allows_collection_permission(
                    &root,
                    &capability_token,
                    "outbound_lead_generation_leads",
                    crate::business_os::policy::BusinessOsPermission::DataRead,
                ) {
                    return Err(
                        "outbound Sellify lookup capability may not read managed tenant leads"
                            .to_string(),
                    );
                }
                let payload = params.first().cloned().unwrap_or(Value::Null);
                outbound_sellify_lookup(&root, &payload).map_err(|e| e.to_string())
            })
        }),
    )?;
    crate::business_os::rxdb_peer_jour_fixe_speech::register(pool, &root)?;
    crate::business_os::rxdb_peer_dictation::register(pool, &root)?;
    crate::business_os::rxdb_peer_grok::register(pool, &root)?;
    crate::business_os::rxdb_peer_speech_settings::register(pool, &root)?;
    let calendar_connect_root = root.clone();
    pool.register_auxiliary_request_handler(
        crate::business_os::mcp_channel::WORKJET_CALENDAR_CONNECT_METHOD,
        Arc::new(move |_peer_identity, capability_token, params| {
            let root = calendar_connect_root.clone();
            Box::pin(async move {
                tokio::task::spawn_blocking(move || {
                    crate::business_os::mcp_channel::connect_workjet_calendar_webrtc(
                        &root,
                        &capability_token,
                        params,
                    )
                })
                .await
                .map_err(|_| "token_exchange_failed".to_string())?
            })
        }),
    )?;
    let calendar_root = root.clone();
    pool.register_auxiliary_request_handler(
        crate::business_os::mcp_channel::WORKJET_CALENDAR_READ_METHOD,
        Arc::new(move |_peer, capability_token, params| {
            let root = calendar_root.clone();
            Box::pin(async move {
                tokio::task::spawn_blocking(move || {
                    crate::business_os::mcp_channel::read_workjet_calendar_webrtc(
                        &root,
                        &capability_token,
                        params,
                    )
                })
                .await
                .map_err(|_| "CALENDAR_READ_UNAVAILABLE".to_string())?
            })
        }),
    )?;
    pool.register_auxiliary_request_handler(
        WORKJET_DEVICE_WEBRTC_METHOD,
        Arc::new(move |_peer, capability_token, params| {
            let root = root.clone();
            Box::pin(async move {
                handle_workjet_device_webrtc_request(&root, &capability_token, params).await
            })
        }),
    )?;
    Ok(())
}
