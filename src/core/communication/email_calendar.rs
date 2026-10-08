// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded, read-only calendar queries using the selected registered mailbox.
//! No credentials or endpoint from the instance mailbox are inherited.
use super::*;

pub(crate) fn read_registered_calendar(
    root: &Path,
    account: &super::super::email_accounts::EmailAccountConfig,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
) -> Result<Value> {
    anyhow::ensure!(
        end_ms
            .checked_sub(start_ms)
            .is_some_and(|span| span > 0 && span <= 400 * 86_400_000),
        "calendar range must be ordered and at most 400 days"
    );
    anyhow::ensure!((1..=100).contains(&limit), "calendar limit must be 1..100");
    let settings = super::super::email_accounts::account_runtime_overrides(root, account);
    let options = base_options_from_runtime(root, &settings, &root.join("runtime/ctox.sqlite3"));
    require_provider_credentials(&options)?;
    let start = chrono::DateTime::from_timestamp_millis(start_ms)
        .context("invalid calendar start")?
        .to_rfc3339();
    let end = chrono::DateTime::from_timestamp_millis(end_ms)
        .context("invalid calendar end")?
        .to_rfc3339();
    match options.provider.as_str() {
        "ews" => {
            let client = EwsClient::from_options(&options)?;
            let body = format!(
                r#"<m:ItemShape><t:BaseShape>IdOnly</t:BaseShape><t:AdditionalProperties>
<t:FieldURI FieldURI="item:Subject"/><t:FieldURI FieldURI="calendar:Start"/>
<t:FieldURI FieldURI="calendar:End"/><t:FieldURI FieldURI="calendar:IsAllDayEvent"/>
<t:FieldURI FieldURI="calendar:Location"/></t:AdditionalProperties></m:ItemShape>
<m:CalendarView MaxEntriesReturned="{limit}" StartDate="{}" EndDate="{}"/>
<m:ParentFolderIds><t:DistinguishedFolderId Id="calendar"/></m:ParentFolderIds>"#,
                xml_escape(&start),
                xml_escape(&end)
            );
            let xml = client.request("FindItem", r#" Traversal="Shallow""#, &body)?;
            parse_ews_calendar(&xml)
        }
        "graph" => {
            let client = GraphClient::from_options(&options)?;
            let page = client.request(
                "GET",
                &format!("/{}/calendarView", client.user),
                &[
                    ("startDateTime", start),
                    ("endDateTime", end),
                    ("$top", limit.to_string()),
                    ("$select", "id,subject,start,end,isAllDay,location".into()),
                ],
                None,
            )?;
            parse_graph_calendar(&page)
        }
        _ => bail!("registered mailbox provider has no calendar connector"),
    }
}

fn calendar_item(
    id: &str,
    title: &str,
    start: &str,
    end: &str,
    all_day: bool,
    location: &str,
) -> Result<Value> {
    anyhow::ensure!(
        !id.is_empty() && id.chars().count() <= 256,
        "invalid calendar item identity"
    );
    let start_ms = chrono::DateTime::parse_from_rfc3339(start)?.timestamp_millis();
    let end_ms = chrono::DateTime::parse_from_rfc3339(end)?.timestamp_millis();
    anyhow::ensure!(
        end_ms > start_ms,
        "calendar event end must follow its start"
    );
    Ok(json!({
        "external_id": id,
        "title": if title.trim().is_empty() { "(Untitled)".to_string() } else { title.chars().take(256).collect::<String>() },
        "start_ms": start_ms, "end_ms": end_ms, "all_day": all_day,
        "timezone": "UTC", "location": location.chars().take(512).collect::<String>(),
    }))
}

fn parse_ews_calendar(xml: &str) -> Result<Value> {
    let doc = Document::parse(xml)?;
    let responses = ews_response_messages(&doc, "FindItemResponseMessage", 1)?;
    let folder = responses[0]
        .children()
        .find(|n| n.has_tag_name("RootFolder"))
        .context("calendar response has no root folder")?;
    let items = folder
        .children()
        .find(|n| n.has_tag_name("Items"))
        .context("calendar response has no items")?;
    let mut events = Vec::new();
    for node in items.children().filter(|n| n.has_tag_name("CalendarItem")) {
        let id = node
            .children()
            .find(|n| n.has_tag_name("ItemId"))
            .and_then(|n| n.attribute("Id"))
            .context("calendar item has no id")?;
        events.push(calendar_item(
            id,
            &descendant_text(node, "Subject").unwrap_or_default(),
            &descendant_text(node, "Start").context("calendar item has no start")?,
            &descendant_text(node, "End").context("calendar item has no end")?,
            descendant_text(node, "IsAllDayEvent").as_deref() == Some("true"),
            &descendant_text(node, "Location").unwrap_or_default(),
        )?);
    }
    // Missing completeness evidence must never be reported as a complete sync.
    Ok(json!({"events": events,
        "truncated": folder.attribute("IncludesLastItemInRange") != Some("true")}))
}

fn graph_time(value: &Value) -> Result<String> {
    anyhow::ensure!(
        value["timeZone"].as_str() == Some("UTC"),
        "calendar response is not in the requested default UTC zone"
    );
    let raw = value["dateTime"]
        .as_str()
        .context("calendar event has no dateTime")?;
    Ok(if raw.ends_with('Z') {
        raw.into()
    } else {
        format!("{raw}Z")
    })
}

fn parse_graph_calendar(page: &Value) -> Result<Value> {
    let items = page["value"]
        .as_array()
        .context("calendar response has no event list")?;
    let events = items
        .iter()
        .map(|item| {
            calendar_item(
                item["id"].as_str().context("calendar item has no id")?,
                item["subject"].as_str().unwrap_or_default(),
                &graph_time(&item["start"])?,
                &graph_time(&item["end"])?,
                item["isAllDay"]
                    .as_bool()
                    .context("calendar item has no all-day flag")?,
                item["location"]["displayName"].as_str().unwrap_or_default(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"events": events,
        "truncated": page.get("@odata.nextLink").is_some_and(|v| !v.is_null())}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ews_recurring_occurrences_and_all_day_keep_provider_dates() -> Result<()> {
        let xml = r#"<Envelope><FindItemResponseMessage ResponseClass="Success"><ResponseCode>NoError</ResponseCode><RootFolder IncludesLastItemInRange="true"><Items><CalendarItem><ItemId Id="occurrence-1"/><Subject>All day</Subject><Start>2026-10-08T00:00:00Z</Start><End>2026-10-09T00:00:00Z</End><IsAllDayEvent>true</IsAllDayEvent></CalendarItem></Items></RootFolder></FindItemResponseMessage></Envelope>"#;
        let read = parse_ews_calendar(xml)?;
        assert_eq!(read["truncated"], false);
        assert_eq!(read["events"][0]["all_day"], true);
        assert_eq!(
            read["events"][0]["end_ms"].as_i64().unwrap()
                - read["events"][0]["start_ms"].as_i64().unwrap(),
            86_400_000
        );
        assert_eq!(
            parse_ews_calendar(&xml.replace(" IncludesLastItemInRange=\"true\"", ""))?["truncated"],
            true
        );
        Ok(())
    }
    #[test]
    fn provider_calendar_preserves_instants_all_day_and_completeness() -> Result<()> {
        let page = json!({"value": [{
            "id": "calendar-item-1", "subject": "Account meeting",
            "start": {"dateTime": "2026-10-08T08:00:00", "timeZone": "UTC"},
            "end": {"dateTime": "2026-10-08T09:30:00", "timeZone": "UTC"},
            "isAllDay": false, "location": {"displayName": "Office"}
        }], "@odata.nextLink": "not-followed"});
        let read = parse_graph_calendar(&page)?;
        assert_eq!(read["truncated"], true);
        assert_eq!(
            read["events"][0]["end_ms"].as_i64().unwrap()
                - read["events"][0]["start_ms"].as_i64().unwrap(),
            90 * 60_000
        );
        assert!(calendar_item(
            "id",
            "title",
            "2026-10-08T10:00:00Z",
            "2026-10-08T09:00:00Z",
            false,
            ""
        )
        .is_err());
        assert!(
            graph_time(&json!({"dateTime":"2026-10-08T08:00:00", "timeZone":"Unknown"})).is_err()
        );
        Ok(())
    }
}
