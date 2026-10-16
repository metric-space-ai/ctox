include!("service_sqlite_read_cache.rs");
#[cfg(test)]
static DURABLE_STATUS_LOAD_COUNTS: OnceLock<Mutex<BTreeMap<PathBuf, usize>>> = OnceLock::new();
#[cfg(test)]
static DURABLE_STATUS_LCM_OUTCOME_OPEN_COUNTS: OnceLock<Mutex<BTreeMap<PathBuf, usize>>> =
    OnceLock::new();
fn durable_status_snapshot_cache() -> &'static DurableServiceStatusCache {
    static CACHE: OnceLock<DurableServiceStatusCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn durable_status_snapshot_cached(root: &Path, ttl: Duration) -> DurableServiceStatusSnapshot {
    let root_path = root.to_path_buf();
    let file_stamp = durable_status_file_stamp(root);
    let cache = durable_status_snapshot_cache();
    {
        let guard = cache.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(cached) = guard.as_ref() {
            if cached.root == root_path
                && (cached.loaded_at.elapsed() < ttl || cached.file_stamp == file_stamp)
            {
                return cached.snapshot.clone();
            }
        }
    }
    let source_stamp = durable_status_source_stamp(root);
    {
        let mut guard = cache.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(cached) = guard.as_mut() {
            if cached.root == root_path && cached.source_stamp == source_stamp {
                cached.loaded_at = Instant::now();
                cached.file_stamp = durable_status_file_stamp(root);
                return cached.snapshot.clone();
            }
        }
    }
    let snapshot = load_durable_status_snapshot(root);
    let source_stamp = durable_status_source_stamp(root);
    let file_stamp = durable_status_file_stamp(root);
    *cache.lock().unwrap_or_else(|err| err.into_inner()) = Some(DurableServiceStatusCacheEntry {
        loaded_at: Instant::now(),
        root: root_path,
        file_stamp,
        source_stamp,
        snapshot: snapshot.clone(),
    });
    snapshot
}

fn durable_status_file_stamp(root: &Path) -> DurableStatusFileStamp {
    DurableStatusFileStamp {
        core_db: core_db_change_stamp(&crate::paths::core_db(root)),
        ticket_store: tickets::ticket_store_change_stamp(root),
    }
}

fn durable_status_source_stamp(root: &Path) -> DurableStatusSourceStamp {
    let core_db_path = crate::paths::core_db(root);
    DurableStatusSourceStamp {
        communication: durable_communication_source_stamp(root, &core_db_path),
        lcm_outcome: durable_lcm_outcome_source_stamp(&core_db_path),
        ticket_cases: durable_ticket_case_source_stamp(root),
    }
}

fn channel_router_source_stamp(root: &Path) -> ChannelRouterSourceStamp {
    let root_path = root.to_path_buf();
    let core_db_path = crate::paths::core_db(root);
    let core_db_stamp = core_db_change_stamp(&core_db_path);
    let business_os_db_path = router_document_report_db_path(root);
    let business_os_db_stamp = core_db_change_stamp(&business_os_db_path);
    let cache = CHANNEL_ROUTER_SOURCE_STAMP_CACHE.get_or_init(|| Mutex::new(None));
    {
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_ref() {
            if cached.root == root_path
                && cached.core_db_path == core_db_path
                && cached.core_db_stamp == core_db_stamp
                && cached.business_os_db_path == business_os_db_path
                && cached.business_os_db_stamp == business_os_db_stamp
            {
                return cached.source_stamp.clone();
            }
        }
    }
    #[cfg(test)]
    record_channel_router_source_stamp_load_for_tests(root);
    let source_stamp = ChannelRouterSourceStamp {
        communication: durable_communication_source_stamp(root, &core_db_path),
        schedule: router_schedule_source_stamp(&core_db_path),
        document_reports: router_document_report_source_stamp_for_path(&business_os_db_path),
        tickets: router_ticket_source_stamp(&core_db_path),
    };
    let cached_core_db_stamp = core_db_change_stamp(&core_db_path);
    let cached_business_os_db_stamp = core_db_change_stamp(&business_os_db_path);
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(ChannelRouterSourceStampCacheEntry {
        root: root_path,
        core_db_path,
        core_db_stamp: cached_core_db_stamp,
        business_os_db_path,
        business_os_db_stamp: cached_business_os_db_stamp,
        source_stamp: source_stamp.clone(),
    });
    source_stamp
}

fn channel_router_source_has_due_time(source_stamp: &ChannelRouterSourceStamp) -> bool {
    let RouterScheduleSourceStamp::Source(schedule) = &source_stamp.schedule else {
        return false;
    };
    if schedule.earliest_next_run_at.trim().is_empty() {
        return false;
    }
    DateTime::parse_from_rfc3339(&schedule.earliest_next_run_at)
        .map(|due_at| due_at.with_timezone(&Utc) <= Utc::now())
        .unwrap_or(false)
}

fn business_os_app_recovery_source_stamp(root: &Path) -> BusinessOsAppRecoverySourceStamp {
    let core_db_path = crate::paths::core_db(root);
    match business_os_app_recovery_queue_stamp(root, &core_db_path) {
        Ok(stamp) => BusinessOsAppRecoverySourceStamp::Source(stamp),
        Err(_) => BusinessOsAppRecoverySourceStamp::File(core_db_change_stamp(&core_db_path)),
    }
}

fn business_os_app_recovery_queue_stamp(
    root: &Path,
    core_db_path: &Path,
) -> Result<BusinessOsAppRecoveryQueueStamp> {
    if !core_db_path.exists() {
        return Ok(empty_business_os_app_recovery_queue_stamp(
            false, false, false,
        ));
    }
    let (messages_table_exists, routing_table_exists, source_rows) = crate::lcm::with_lcm_read(
        core_db_path,
        |conn| {
            let messages = sqlite_table_exists(conn, "communication_messages")?;
            let routing = sqlite_table_exists(conn, "communication_routing_state")?;
            if !messages || !routing {
                return Ok((messages, routing, Vec::new()));
            }
            let mut statement=conn.prepare(r#"
            SELECT m.message_key,m.body_text,m.metadata_json,COALESCE(r.route_status,'pending'),
            COALESCE(r.leased_at,''),COALESCE(r.updated_at,m.observed_at),
            COALESCE(m.external_created_at,''),COALESCE(m.observed_at,'')
            FROM communication_messages m JOIN communication_routing_state r ON r.message_key=m.message_key
            WHERE m.channel='queue' AND m.direction='inbound' AND lower(COALESCE(r.route_status,'pending'))='leased'
            ORDER BY m.message_key"#)?;
            let rows = statement
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, String>(7)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((messages, routing, rows))
        },
    )?;
    if !messages_table_exists || !routing_table_exists {
        return Ok(empty_business_os_app_recovery_queue_stamp(
            true,
            messages_table_exists,
            routing_table_exists,
        ));
    }
    let mut candidate_count = 0usize;
    let mut latest_route_updated_at = String::new();
    let mut next_recovery_due_epoch_secs: Option<u64> = None;
    let mut hasher = DefaultHasher::new();

    for (
        message_key,
        prompt,
        metadata_json,
        route_status,
        leased_at,
        route_updated_at,
        external_created_at,
        observed_at,
    ) in source_rows
    {
        let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or(Value::Null);
        let Some(target) = business_os_app_module_target_from_metadata(&metadata) else {
            continue;
        };
        candidate_count = candidate_count.saturating_add(1);
        if route_updated_at > latest_route_updated_at {
            latest_route_updated_at = route_updated_at.clone();
        }
        if let Some(due_epoch_secs) = business_os_app_recovery_due_epoch_secs(&leased_at) {
            next_recovery_due_epoch_secs = Some(
                next_recovery_due_epoch_secs
                    .map(|current| current.min(due_epoch_secs))
                    .unwrap_or(due_epoch_secs),
            );
        }

        message_key.hash(&mut hasher);
        prompt.hash(&mut hasher);
        metadata_json.hash(&mut hasher);
        route_status.hash(&mut hasher);
        leased_at.hash(&mut hasher);
        route_updated_at.hash(&mut hasher);
        external_created_at.hash(&mut hasher);
        observed_at.hash(&mut hasher);
        target.module_id.hash(&mut hasher);
        target.install_target.hash(&mut hasher);
        target.artifact_directory.hash(&mut hasher);

        let workspace_root =
            channels::workspace_root_from_queue_metadata_or_prompt(&metadata, &prompt)
                .map(PathBuf::from)
                .filter(|path| business_os_app_workspace_root_looks_valid(path))
                .unwrap_or_else(|| root.to_path_buf());
        let artifact_dir = workspace_root.join(&target.artifact_directory);
        business_os_app_artifact_tree_stamp(&artifact_dir).hash(&mut hasher);
    }

    Ok(BusinessOsAppRecoveryQueueStamp {
        database_exists: true,
        messages_table_exists: true,
        routing_table_exists: true,
        candidate_count,
        latest_route_updated_at,
        next_recovery_due_epoch_secs,
        source_fingerprint: hasher.finish(),
    })
}

fn empty_business_os_app_recovery_queue_stamp(
    database_exists: bool,
    messages_table_exists: bool,
    routing_table_exists: bool,
) -> BusinessOsAppRecoveryQueueStamp {
    BusinessOsAppRecoveryQueueStamp {
        database_exists,
        messages_table_exists,
        routing_table_exists,
        candidate_count: 0,
        latest_route_updated_at: String::new(),
        next_recovery_due_epoch_secs: None,
        source_fingerprint: 0,
    }
}

fn business_os_app_recovery_due_epoch_secs(leased_at: &str) -> Option<u64> {
    let leased_at = parse_rfc3339_system_time(leased_at)?;
    let leased_epoch_secs = leased_at.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(leased_epoch_secs.saturating_add(BUSINESS_OS_APP_RECOVERY_STALE_SECS))
}

fn business_os_app_recovery_source_has_due_time(
    source_stamp: &BusinessOsAppRecoverySourceStamp,
) -> bool {
    let BusinessOsAppRecoverySourceStamp::Source(stamp) = source_stamp else {
        return false;
    };
    stamp
        .next_recovery_due_epoch_secs
        .map(|due_epoch_secs| due_epoch_secs <= current_epoch_secs())
        .unwrap_or(false)
}

fn harness_audit_source_stamp(root: &Path) -> HarnessAuditSourceStamp {
    let core_db_path = crate::paths::core_db(root);
    match harness_audit_db_stamp(&core_db_path) {
        Ok(stamp) => HarnessAuditSourceStamp::Source(stamp),
        Err(_) => HarnessAuditSourceStamp::File(core_db_change_stamp(&core_db_path)),
    }
}

fn harness_audit_db_stamp(core_db_path: &Path) -> Result<HarnessAuditDbStamp> {
    if !core_db_path.exists() {
        return Ok(HarnessAuditDbStamp {
            database_exists: false,
            tables: Vec::new(),
        });
    }
    crate::lcm::with_lcm_read(core_db_path, |conn| {
        let table_specs = [
            ("ctox_core_transition_proofs", Some("updated_at"), None),
            ("ctox_process_events", Some("observed_at"), None),
            ("ctox_pm_state_violations", Some("detected_at"), None),
            ("ctox_pm_core_transition_audit", Some("scanned_at"), None),
            ("ctox_pm_core_transition_rules", Some("updated_at"), None),
            (
                "ctox_pm_event_transition_coverage",
                Some("scanned_at"),
                None,
            ),
            ("ctox_pm_unmapped_events", Some("scanned_at"), None),
            ("ctox_core_spawn_edges", Some("updated_at"), None),
            (
                "ctox_hm_findings",
                Some("last_seen_at"),
                Some("status = 'detected'"),
            ),
        ];
        let mut tables = Vec::with_capacity(table_specs.len());
        for (table_name, timestamp_column, where_clause) in table_specs {
            tables.push(harness_audit_table_stamp(
                &conn,
                table_name,
                timestamp_column,
                where_clause,
            )?);
        }
        Ok(HarnessAuditDbStamp {
            database_exists: true,
            tables,
        })
    })
}

fn harness_audit_table_stamp(
    conn: &Connection,
    table_name: &str,
    timestamp_column: Option<&str>,
    where_clause: Option<&str>,
) -> Result<HarnessAuditTableStamp> {
    if !sqlite_table_exists(conn, table_name)? {
        return Ok(HarnessAuditTableStamp {
            table_name: table_name.to_string(),
            table_exists: false,
            row_count: 0,
            max_rowid: 0,
            latest_timestamp: String::new(),
        });
    }
    let where_sql = where_clause
        .map(|clause| format!(" WHERE {clause}"))
        .unwrap_or_default();
    let sql = if let Some(column) = timestamp_column {
        format!(
            "SELECT COUNT(*), COALESCE(MAX(rowid), 0), COALESCE(MAX({column}), '') FROM {table_name}{where_sql}"
        )
    } else {
        format!("SELECT COUNT(*), COALESCE(MAX(rowid), 0), '' FROM {table_name}{where_sql}")
    };
    let (row_count, max_rowid, latest_timestamp) = conn.query_row(&sql, [], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    Ok(HarnessAuditTableStamp {
        table_name: table_name.to_string(),
        table_exists: true,
        row_count: row_count.max(0) as usize,
        max_rowid,
        latest_timestamp,
    })
}

fn harness_audit_source_has_detected_findings(source_stamp: &HarnessAuditSourceStamp) -> bool {
    let HarnessAuditSourceStamp::Source(stamp) = source_stamp else {
        return false;
    };
    stamp.tables.iter().any(|table| {
        table.table_name == "ctox_hm_findings" && table.table_exists && table.row_count > 0
    })
}

fn router_schedule_source_stamp(core_db_path: &Path) -> RouterScheduleSourceStamp {
    match router_schedule_table_stamp(core_db_path) {
        Ok(stamp) => RouterScheduleSourceStamp::Source(stamp),
        Err(_) => RouterScheduleSourceStamp::File(core_db_change_stamp(core_db_path)),
    }
}

fn router_schedule_table_stamp(core_db_path: &Path) -> Result<RouterScheduleTableStamp> {
    with_cached_service_sqlite_read_only(core_db_path, "router schedule stamp", |conn| {
        let Some(conn) = conn else {
            return Ok(RouterScheduleTableStamp {
                database_exists: false,
                table_exists: false,
                enabled_count: 0,
                earliest_next_run_at: String::new(),
                latest_updated_at: String::new(),
            });
        };
        if !sqlite_table_exists(conn, "scheduled_tasks")? {
            return Ok(RouterScheduleTableStamp {
                database_exists: true,
                table_exists: false,
                enabled_count: 0,
                earliest_next_run_at: String::new(),
                latest_updated_at: String::new(),
            });
        }
        let (enabled_count, earliest_next_run_at, latest_updated_at) = conn.query_row(
            r#"
            SELECT COUNT(*), COALESCE(MIN(next_run_at), ''), COALESCE(MAX(updated_at), '')
            FROM scheduled_tasks
            WHERE enabled = 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        Ok(RouterScheduleTableStamp {
            database_exists: true,
            table_exists: true,
            enabled_count: enabled_count.max(0) as usize,
            earliest_next_run_at,
            latest_updated_at,
        })
    })
}

fn router_document_report_source_stamp_for_path(path: &Path) -> RouterDocumentReportSourceStamp {
    match router_document_report_table_stamp(path) {
        Ok(stamp) => RouterDocumentReportSourceStamp::Source(stamp),
        Err(_) => RouterDocumentReportSourceStamp::File(core_db_change_stamp(path)),
    }
}

fn router_document_report_db_path(root: &Path) -> PathBuf {
    root.join("runtime").join("business-os.sqlite3")
}

fn router_document_report_table_stamp(path: &Path) -> Result<RouterDocumentReportTableStamp> {
    with_cached_service_sqlite_read_only(path, "router document report stamp", |conn| {
        let Some(conn) = conn else {
            return Ok(RouterDocumentReportTableStamp {
                database_exists: false,
                table_exists: false,
                pending_count: 0,
                latest_observed_at_ms: 0,
            });
        };
        if !sqlite_table_exists(conn, "business_commands")? {
            return Ok(RouterDocumentReportTableStamp {
                database_exists: true,
                table_exists: false,
                pending_count: 0,
                latest_observed_at_ms: 0,
            });
        }
        let query = format!(
            "SELECT COUNT(*), COALESCE(MAX(observed_at_ms), 0)
             FROM business_commands
             WHERE module = 'documents'
               AND command_type = 'research.systematic.report.create'
               AND status NOT IN ({})",
            crate::command_lifecycle::CTOX_COMMAND_TERMINAL_OUTCOME_SQL_LIST,
        );
        let (pending_count, latest_observed_at_ms) = conn.query_row(&query, [], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok(RouterDocumentReportTableStamp {
            database_exists: true,
            table_exists: true,
            pending_count: pending_count.max(0) as usize,
            latest_observed_at_ms,
        })
    })
}

fn router_ticket_source_stamp(core_db_path: &Path) -> RouterTicketSourceStamp {
    match router_ticket_table_stamp(core_db_path) {
        Ok(stamp) => RouterTicketSourceStamp::Source(stamp),
        Err(_) => RouterTicketSourceStamp::File(core_db_change_stamp(core_db_path)),
    }
}

fn router_ticket_table_stamp(core_db_path: &Path) -> Result<RouterTicketTableStamp> {
    with_cached_service_sqlite_read_only(core_db_path, "router ticket stamp", |conn| {
        let Some(conn) = conn else {
            return Ok(RouterTicketTableStamp {
                database_exists: false,
                events_table_exists: false,
                event_routing_table_exists: false,
                self_work_table_exists: false,
                cases_table_exists: false,
                routed_event_count: 0,
                latest_routed_event_updated_at: String::new(),
                active_self_work_count: 0,
                latest_active_self_work_updated_at: String::new(),
                open_case_count: 0,
                latest_open_case_updated_at: String::new(),
            });
        };
        let events_table_exists = sqlite_table_exists(conn, "ticket_events")?;
        let event_routing_table_exists = sqlite_table_exists(conn, "ticket_event_routing_state")?;
        let self_work_table_exists = sqlite_table_exists(conn, "ticket_self_work_items")?;
        let cases_table_exists = sqlite_table_exists(conn, "ticket_cases")?;
        let (routed_event_count, latest_routed_event_updated_at) =
            if events_table_exists && event_routing_table_exists {
                conn.query_row(
                    r#"
                SELECT COUNT(*), COALESCE(MAX(r.updated_at), '')
                FROM ticket_events e
                JOIN ticket_event_routing_state r ON r.event_key = e.event_key
                WHERE r.route_status NOT IN ('handled', 'blocked', 'failed', 'cancelled')
                "#,
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )?
            } else {
                (0, String::new())
            };
        let (active_self_work_count, latest_active_self_work_updated_at) = if self_work_table_exists
        {
            conn.query_row(
                r#"
            SELECT COUNT(*), COALESCE(MAX(updated_at), '')
            FROM ticket_self_work_items
            WHERE state IN ('published', 'queued', 'created', 'open', 'blocked', 'restored')
            "#,
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?
        } else {
            (0, String::new())
        };
        let (open_case_count, latest_open_case_updated_at) = if cases_table_exists {
            conn.query_row(
                r#"
            SELECT COUNT(*), COALESCE(MAX(updated_at), '')
            FROM ticket_cases
            WHERE state <> 'closed'
            "#,
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?
        } else {
            (0, String::new())
        };
        Ok(RouterTicketTableStamp {
            database_exists: true,
            events_table_exists,
            event_routing_table_exists,
            self_work_table_exists,
            cases_table_exists,
            routed_event_count: routed_event_count.max(0) as usize,
            latest_routed_event_updated_at,
            active_self_work_count: active_self_work_count.max(0) as usize,
            latest_active_self_work_updated_at,
            open_case_count: open_case_count.max(0) as usize,
            latest_open_case_updated_at,
        })
    })
}

fn durable_communication_source_stamp(
    root: &Path,
    core_db_path: &Path,
) -> DurableCommunicationSourceStamp {
    let root_path = root.to_path_buf();
    let core_db_path = core_db_path.to_path_buf();
    let core_db_stamp = core_db_change_stamp(&core_db_path);
    let cache = DURABLE_COMMUNICATION_SOURCE_STAMP_CACHE.get_or_init(|| Mutex::new(None));
    {
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_ref() {
            if cached.root == root_path
                && cached.core_db_path == core_db_path
                && cached.core_db_stamp == core_db_stamp
            {
                return cached.source_stamp.clone();
            }
        }
    }
    let source_stamp = match channels::communication_intake_source_stamp(root) {
        Ok(stamp) => DurableCommunicationSourceStamp::Source(stamp),
        Err(_) => DurableCommunicationSourceStamp::File(core_db_stamp.clone()),
    };
    let cached_core_db_stamp = core_db_change_stamp(&core_db_path);
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(DurableCommunicationSourceStampCacheEntry {
        root: root_path,
        core_db_path,
        core_db_stamp: cached_core_db_stamp,
        source_stamp: source_stamp.clone(),
    });
    source_stamp
}

fn durable_ticket_case_source_stamp(root: &Path) -> DurableTicketCaseSourceStamp {
    match tickets::ticket_case_status_stamp(root) {
        Ok(stamp) => DurableTicketCaseSourceStamp::Source(stamp),
        Err(_) => DurableTicketCaseSourceStamp::File(tickets::ticket_store_change_stamp(root)),
    }
}

fn durable_lcm_outcome_source_stamp(core_db_path: &Path) -> DurableLcmOutcomeSourceStamp {
    match lcm_last_agent_outcome_stamp(core_db_path) {
        Ok(stamp) => DurableLcmOutcomeSourceStamp::Source(stamp),
        Err(_) => DurableLcmOutcomeSourceStamp::File(core_db_change_stamp(core_db_path)),
    }
}

fn lcm_last_agent_outcome_stamp(core_db_path: &Path) -> Result<LcmLastAgentOutcomeStamp> {
    if !core_db_path.exists() {
        return Ok(empty_lcm_last_agent_outcome_stamp(false, false, false));
    }
    crate::lcm::with_lcm_read(core_db_path, |conn| {
        let messages_table_exists = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages' LIMIT 1",
                [],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !messages_table_exists {
            return Ok(empty_lcm_last_agent_outcome_stamp(true, false, false));
        }
        let agent_outcome_column_exists = conn
            .prepare("PRAGMA table_info(messages)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|name| name == "agent_outcome");
        if !agent_outcome_column_exists {
            return Ok(empty_lcm_last_agent_outcome_stamp(true, true, false));
        }
        let last = conn
            .query_row(
                r#"
            SELECT seq, COALESCE(agent_outcome, '')
            FROM messages
            WHERE conversation_id = ?1
              AND role = 'assistant'
            ORDER BY seq DESC
            LIMIT 1
            "#,
                params![turn_loop::CHAT_CONVERSATION_ID],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let (last_assistant_seq, last_assistant_outcome) =
            last.unwrap_or_else(|| (0, String::new()));
        Ok(LcmLastAgentOutcomeStamp {
            database_exists: true,
            messages_table_exists: true,
            agent_outcome_column_exists: true,
            last_assistant_seq,
            last_assistant_outcome,
        })
    })
}

fn empty_lcm_last_agent_outcome_stamp(
    database_exists: bool,
    messages_table_exists: bool,
    agent_outcome_column_exists: bool,
) -> LcmLastAgentOutcomeStamp {
    LcmLastAgentOutcomeStamp {
        database_exists,
        messages_table_exists,
        agent_outcome_column_exists,
        last_assistant_seq: 0,
        last_assistant_outcome: String::new(),
    }
}

fn load_durable_status_snapshot(root: &Path) -> DurableServiceStatusSnapshot {
    record_durable_status_load_for_test(root);
    let runnable_statuses = ["pending".to_string(), "leased".to_string()];
    let mut pending_previews = Vec::new();
    let runnable_durable_tasks = match channels::list_queue_tasks(root, &runnable_statuses, 6) {
        Ok(tasks) => tasks,
        Err(err) => {
            eprintln!("ctox service status queue read failed: {err:#}");
            pending_previews.push(format!(
                "queue status unavailable  {}",
                clip_text(&err.to_string(), 96)
            ));
            Vec::new()
        }
    };
    let runnable_count = channels::count_queue_tasks(root, &runnable_statuses)
        .unwrap_or(runnable_durable_tasks.len());
    let blocked_statuses = ["blocked".to_string()];
    let blocked_durable_tasks = match channels::list_queue_tasks(root, &blocked_statuses, 6) {
        Ok(tasks) => tasks,
        Err(err) => {
            eprintln!("ctox service blocked queue read failed: {err:#}");
            Vec::new()
        }
    };
    let blocked_count =
        channels::count_queue_tasks(root, &blocked_statuses).unwrap_or(blocked_durable_tasks.len());
    let mut blocked_previews = Vec::new();

    for task in &runnable_durable_tasks {
        if pending_previews.len() >= 6 {
            break;
        }
        let preview = format!("queue  {}", clip_text(task.title.trim(), 120));
        if !pending_previews.iter().any(|existing| existing == &preview) {
            pending_previews.push(preview);
        }
    }
    for task in &blocked_durable_tasks {
        if blocked_previews.len() >= 6 {
            break;
        }
        let preview = format!("queue blocked  {}", clip_text(task.title.trim(), 112));
        if !blocked_previews.iter().any(|existing| existing == &preview) {
            blocked_previews.push(preview);
        }
    }
    for case in tickets::list_cases(root, None, 6)
        .unwrap_or_default()
        .into_iter()
        .filter(|case| !matches!(case.state.as_str(), "closed"))
    {
        if pending_previews.len() >= 6 {
            break;
        }
        let preview = format!(
            "ticket  {} {}",
            case.label,
            clip_text(case.ticket_key.trim(), 96)
        );
        if !pending_previews.iter().any(|existing| existing == &preview) {
            pending_previews.push(preview);
        }
    }

    let last_agent_outcome = {
        let db_path = crate::paths::core_db(&root);
        record_durable_status_lcm_outcome_open_for_test(&db_path);
        lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())
            .ok()
            .and_then(|engine| {
                engine
                    .last_agent_outcome(turn_loop::CHAT_CONVERSATION_ID)
                    .ok()
                    .flatten()
            })
            .map(|outcome| outcome.as_str().to_string())
    };

    DurableServiceStatusSnapshot {
        runnable_count,
        pending_previews,
        blocked_count,
        blocked_previews,
        last_agent_outcome,
    }
}

#[cfg(test)]
fn record_durable_status_load_for_test(root: &Path) {
    let counts = DURABLE_STATUS_LOAD_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts.lock().unwrap_or_else(|err| err.into_inner());
    *counts.entry(root.to_path_buf()).or_insert(0) += 1;
}

#[cfg(not(test))]
fn record_durable_status_load_for_test(_root: &Path) {}

#[cfg(test)]
fn record_durable_status_lcm_outcome_open_for_test(db_path: &Path) {
    let counts = DURABLE_STATUS_LCM_OUTCOME_OPEN_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts.lock().unwrap_or_else(|err| err.into_inner());
    *counts.entry(db_path.to_path_buf()).or_insert(0) += 1;
}

#[cfg(not(test))]
fn record_durable_status_lcm_outcome_open_for_test(_db_path: &Path) {}

#[cfg(test)]
fn clear_durable_status_snapshot_cache_for_tests() {
    *durable_status_snapshot_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner()) = None;
    if let Some(counts) = DURABLE_STATUS_LOAD_COUNTS.get() {
        counts.lock().unwrap_or_else(|err| err.into_inner()).clear();
    }
    if let Some(counts) = DURABLE_STATUS_LCM_OUTCOME_OPEN_COUNTS.get() {
        counts.lock().unwrap_or_else(|err| err.into_inner()).clear();
    }
}

#[cfg(test)]
fn durable_status_load_count_for_test(root: &Path) -> usize {
    DURABLE_STATUS_LOAD_COUNTS
        .get()
        .and_then(|counts| {
            counts
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .get(root)
                .copied()
        })
        .unwrap_or(0)
}

#[cfg(test)]
fn durable_status_lcm_outcome_open_count_for_test(root: &Path) -> usize {
    let db_path = crate::paths::core_db(root);
    DURABLE_STATUS_LCM_OUTCOME_OPEN_COUNTS
        .get()
        .and_then(|counts| {
            counts
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .get(&db_path)
                .copied()
        })
        .unwrap_or(0)
}

fn status_from_shared_state(root: &Path, state: &Arc<Mutex<SharedState>>) -> Result<ServiceStatus> {
    // Keep service status a read-side control-plane operation. Business OS app
    // recovery runs from the dedicated maintenance loop and worker-finalization
    // paths; a status poll must not start validation, queue mutation, or broad
    // recovery scans.
    let shared = lock_shared_state(state);
    let worker_active_count = shared.worker_active_count;
    let worker_phase = shared.worker_phase.clone();
    let busy = shared.busy || worker_active_count > 0;
    let pid = Some(std::process::id());
    let current_goal_preview = shared.current_goal_preview.clone();
    let active_source_label = shared.active_source_label.clone();
    let recent_events = shared.recent_events.iter().cloned().collect::<Vec<_>>();
    let last_error = shared.last_error.clone();
    let last_completed_at = shared.last_completed_at.clone();
    let last_reply_chars = shared.last_reply_chars;
    let mut pending_previews = shared
        .pending_prompts
        .iter()
        .take(6)
        .map(|item| format!("{}  {}", item.source_label, item.preview))
        .collect::<Vec<_>>();
    let in_memory_pending_count = shared.pending_prompts.len();
    drop(shared);

    let durable_status = durable_status_snapshot_cached(
        root,
        Duration::from_secs(SERVICE_STATUS_DURABLE_CACHE_TTL_SECS),
    );
    let blocked_previews = durable_status.blocked_previews.clone();
    for preview in &durable_status.pending_previews {
        if pending_previews.len() >= 6 {
            break;
        }
        if !pending_previews.iter().any(|existing| existing == preview) {
            pending_previews.push(preview.clone());
        }
    }
    let runnable_durable_count = durable_status.runnable_count;
    let blocked_durable_count = durable_status.blocked_count;
    let last_agent_outcome = durable_status.last_agent_outcome.clone();
    // One cached probe instead of two fresh rounds (six systemctl spawns)
    // per status request: this handler answers the 250ms-budget UI poll
    // every 500ms, and a slow answer makes the TUI fall back to a degraded
    // snapshot.
    let systemd = systemd_unit_status_cached(root, Duration::from_secs(5))
        .ok()
        .flatten();
    Ok(ServiceStatus {
        running: true,
        busy,
        pid,
        listen_addr: service_listen_addr(root),
        autostart_enabled: systemd
            .as_ref()
            .map(|status| status.enabled)
            .unwrap_or(false),
        manager: systemd
            .as_ref()
            .map(|_| "systemd-user".to_string())
            .unwrap_or_else(|| "process".to_string()),
        pending_count: in_memory_pending_count
            .max(runnable_durable_count.max(pending_previews.len())),
        pending_previews,
        blocked_count: blocked_durable_count.max(blocked_previews.len()),
        blocked_previews,
        current_goal_preview,
        active_source_label,
        recent_events,
        last_error,
        last_completed_at,
        last_reply_chars,
        monitor_last_check_at: None,
        // The IPC status response is the daemon's hot read path. Lifecycle
        // alerts perform PID checks and duplicate-process scans; callers that
        // need them add them in `service_status_snapshot_with` according to
        // their poll cadence.
        monitor_alerts: Vec::new(),
        monitor_last_error: None,
        last_agent_outcome,
        worker_active_count,
        worker_phase,
        // Keep the daemon IPC status path cheap. The CLI/web callers enrich
        // the returned service state with Business OS health after the socket
        // response, so RxDB/SQLite diagnostics cannot make the daemon miss
        // its short control-plane timeout while an agent turn is busy.
        business_os: None,
        work_hours: crate::service::working_hours::snapshot(root),
        performance: service_performance_snapshot(),
        degraded_probe: false,
    })
}

fn recover_business_os_app_queue_tasks_for_idle_status_snapshot(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
) {
    {
        let mut shared = lock_shared_state(state);
        if !begin_business_os_app_recovery_locked(&mut shared, "idle status snapshot") {
            return;
        }
    }
    let recovered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        recover_stale_business_os_app_queue_task_summary(
            root,
            state,
            BUSINESS_OS_APP_RECOVERY_SCAN_LIMIT,
            0,
        )
    }));
    {
        let mut shared = lock_shared_state(state);
        shared.app_recovery_active = false;
        shared.app_recovery_started_epoch_secs = None;
    }
    match recovered {
        Ok(Ok(summary)) if summary.total() > 0 => {
            push_event(
                state,
                format!(
                    "Recovered {} abandoned Business OS app queue task(s) during idle status snapshot",
                    summary.total()
                ),
            );
        }
        Ok(Ok(_)) => {}
        Ok(Err(err)) => push_event(
            state,
            format!(
                "Business OS app recovery skipped during idle status snapshot: {}",
                clip_text(&err.to_string(), 180)
            ),
        ),
        Err(_) => push_event(
            state,
            "Business OS app recovery panicked during idle status snapshot; continuing".to_string(),
        ),
    }
}

#[cfg(test)]
const SERVICE_STATUS_SOURCES_MODULE_SIZE_BOUNDARY: () = ();
