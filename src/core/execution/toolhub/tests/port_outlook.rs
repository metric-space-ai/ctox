//! Recorded port tests for the Outlook toolkit: one case per tool, request
//! expectations derived from the Activepieces `microsoft-outlook` run() code.
mod support;

use ctox_toolhub::toolkits::outlook::Outlook;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::Case;

/// Percent-encoded `$select` lists exactly as the adapter emits them.
const SEL_MSG: &str = "id%2Csubject%2CbodyPreview%2Cfrom%2Csender%2CtoRecipients%2CccRecipients%2CbccRecipients%2CreplyTo%2CreceivedDateTime%2CsentDateTime%2CcreatedDateTime%2ClastModifiedDateTime%2ChasAttachments%2CisRead%2CisDraft%2Cimportance%2Ccategories%2Cflag%2CwebLink%2CparentFolderId%2CconversationId%2CinternetMessageId";
const SEL_FOLDER: &str = "id%2CdisplayName%2CparentFolderId%2CchildFolderCount%2CunreadItemCount%2CtotalItemCount%2CisHidden";
const SEL_ATT: &str = "id%2Cname%2CcontentType%2Csize%2CisInline%2ClastModifiedDateTime";
const MAIL_TIPS: &str = "automaticReplies, mailboxFullStatus, customMailTip, externalMemberCount, maxMessageSize, deliveryRestriction, moderationStatus, recipientScope, totalMemberCount";

fn strs(v: Vec<&'static str>) -> &'static [&'static str] {
    Box::leak(v.into_boxed_slice())
}
/// JSON-pointer pairs; keys are normalized to leading-`/` form (serde_json's
/// `pointer()` returns None without it).
fn slice(v: Vec<(&'static str, Value)>) -> &'static [(&'static str, Value)] {
    let mapped: Vec<(&'static str, Value)> = v
        .into_iter()
        .map(|(k, val)| (format!("/{}", k.trim_start_matches('/')).leak() as &'static str, val))
        .collect();
    Box::leak(mapped.into_boxed_slice())
}

#[allow(clippy::too_many_arguments)]
fn case(
    tool: &'static str,
    args: Value,
    method: &'static str,
    url_contains: &'static [&'static str],
    body_eq: &'static [(&'static str, Value)],
    response_status: u16,
    response: Value,
    result_eq: &'static [(&'static str, Value)],
) -> Case<'static> {
    Case {
        tool,
        args,
        method,
        url_contains,
        body_eq,
        response_status,
        response,
        result_eq,
    }
}

fn cases() -> Vec<Case<'static>> {
    vec![
        // --- read / search ------------------------------------------------------
        case(
            "OUTLOOK_GET_MY_PROFILE",
            json!({}),
            "GET",
            strs(vec!["/v1.0/me?$select=id,displayName,mail,userPrincipalName"]),
            slice(vec![]),
            200,
            json!({"@odata.context": "https://graph.microsoft.com/v1.0/$metadata#me",
                "id": "u-1", "displayName": "Ann Baumann", "mail": "ann@contoso.com",
                "userPrincipalName": "ann@contoso.com"}),
            slice(vec![("mail", json!("ann@contoso.com")), ("display_name", json!("Ann Baumann"))]),
        ),
        case(
            "OUTLOOK_LIST_MESSAGES",
            json!({"folder_id": "inbox", "filter": "isRead eq false", "max_results": 10, "skip": 5}),
            "GET",
            strs(vec![
                "/v1.0/me/mailFolders/inbox/messages?$select=",
                SEL_MSG,
                "&$top=10&$filter=isRead%20eq%20false&$skip=5",
            ]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "AAMkAGI2TG93AAA=", "subject": "Quarterly numbers",
                    "isRead": false, "receivedDateTime": "2026-01-05T09:24:00Z"}],
                "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$skip=10"}),
            slice(vec![
                ("count", json!(1)),
                ("has_more", json!(true)),
                ("messages/0/id", json!("AAMkAGI2TG93AAA=")),
                ("messages/0/subject", json!("Quarterly numbers")),
            ]),
        ),
        case(
            "OUTLOOK_LIST_MESSAGES",
            json!({}),
            "GET",
            strs(vec![
                "/v1.0/me/messages?$select=",
                SEL_MSG,
                "&$top=25&$orderby=receivedDateTime%20desc",
            ]),
            slice(vec![]),
            200,
            json!({"value": []}),
            slice(vec![("count", json!(0)), ("has_more", json!(false))]),
        ),
        case(
            "OUTLOOK_SEARCH_MESSAGES",
            json!({"search_query": "from:ann@contoso.com hasAttachments:true", "folder_id": "inbox", "max_results": 50}),
            "GET",
            strs(vec![
                "/v1.0/me/mailFolders/inbox/messages?$search=%22from%3Aann%40contoso.com%20hasAttachments%3Atrue%22&$top=50",
            ]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "m-9", "subject": "Invoice 2026-01", "receivedDateTime": "2026-01-04T18:02:00Z"}]}),
            slice(vec![
                ("found", json!(true)),
                ("count", json!(1)),
                ("messages/0/id", json!("m-9")),
            ]),
        ),
        case(
            "OUTLOOK_GET_MESSAGE",
            json!({"message_id": "AAMkAGI2TG93AAA="}),
            "GET",
            strs(vec!["/v1.0/me/messages/AAMkAGI2TG93AAA%3D"]),
            slice(vec![]),
            200,
            json!({"id": "AAMkAGI2TG93AAA=", "subject": "Quarterly numbers",
                "from": {"emailAddress": {"name": "Ann", "address": "ann@contoso.com"}},
                "bodyPreview": "Numbers attached", "isRead": false, "hasAttachments": true,
                "categories": ["Work"], "receivedDateTime": "2026-01-05T09:24:00Z"}),
            slice(vec![
                ("/subject", json!("Quarterly numbers")),
                ("/from/emailAddress/address", json!("ann@contoso.com")),
            ]),
        ),
        case(
            "OUTLOOK_GET_MESSAGE_MIME",
            json!({"message_id": "m1"}),
            "GET",
            strs(vec!["/v1.0/me/messages/m1/$value"]),
            slice(vec![]),
            200,
            json!("hello mime"),
            slice(vec![
                ("file_name", json!("message.eml")),
                ("content_type", json!("message/rfc822")),
                ("size", json!(12)),
                ("content_base64", json!("ImhlbGxvIG1pbWUi")),
            ]),
        ),
        case(
            "OUTLOOK_LIST_MAIL_FOLDERS",
            json!({"max_results": 5}),
            "GET",
            strs(vec![
                "/v1.0/me/mailFolders?includeHiddenFolders=true&$select=",
                SEL_FOLDER,
                "&$top=5",
            ]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "AAMkAGZvbGRkZXI9", "displayName": "Inbox", "childFolderCount": 2,
                    "unreadItemCount": 3, "totalItemCount": 42, "isHidden": false}],
                "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/mailFolders?$skiptoken=5"}),
            slice(vec![
                ("count", json!(1)),
                ("has_more", json!(true)),
                ("folders/0/displayName", json!("Inbox")),
            ]),
        ),
        case(
            "OUTLOOK_LIST_CHILD_MAIL_FOLDERS",
            json!({"folder_id": "inbox", "max_results": 50}),
            "GET",
            strs(vec![
                "/v1.0/me/mailFolders/inbox/childFolders?includeHiddenFolders=true&$select=",
                SEL_FOLDER,
                "&$top=50",
            ]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "child-1", "displayName": "Receipts", "parentFolderId": "inbox"}]}),
            slice(vec![("count", json!(1)), ("folders/0/id", json!("child-1"))]),
        ),
        case(
            "OUTLOOK_GET_MAIL_FOLDER",
            json!({"folder_id": "inbox"}),
            "GET",
            strs(vec!["/v1.0/me/mailFolders/inbox?$select=", SEL_FOLDER]),
            slice(vec![]),
            200,
            json!({"id": "AAMkAGZvbGRkZXI9", "displayName": "Inbox", "parentFolderId": null,
                "childFolderCount": 2, "unreadItemCount": 3, "totalItemCount": 42}),
            slice(vec![("displayName", json!("Inbox")), ("totalItemCount", json!(42))]),
        ),
        case(
            "OUTLOOK_LIST_MESSAGE_ATTACHMENTS",
            json!({"message_id": "m1"}),
            "GET",
            strs(vec!["/v1.0/me/messages/m1/attachments?$select=", SEL_ATT]),
            slice(vec![]),
            200,
            json!({"value": [{"@odata.type": "#microsoft.graph.fileAttachment", "id": "att-1",
                    "name": "report.pdf", "contentType": "application/pdf", "size": 91642,
                    "isInline": false, "lastModifiedDateTime": "2026-01-05T09:24:11Z"}]}),
            slice(vec![
                ("count", json!(1)),
                ("attachments/0/name", json!("report.pdf")),
                ("attachments/0/attachment_type", json!("#microsoft.graph.fileAttachment")),
                ("attachments/0/is_inline", json!(false)),
            ]),
        ),
        case(
            "OUTLOOK_GET_MESSAGE_ATTACHMENT",
            json!({"message_id": "m1", "attachment_id": "att-1"}),
            "GET",
            strs(vec!["/v1.0/me/messages/m1/attachments/att-1?$select=", SEL_ATT]),
            slice(vec![]),
            200,
            json!({"@odata.type": "#microsoft.graph.fileAttachment", "id": "att-1",
                "name": "report.pdf", "contentType": "application/pdf", "size": 91642,
                "isInline": false, "lastModifiedDateTime": "2026-01-05T09:24:11Z"}),
            slice(vec![("name", json!("report.pdf")), ("size", json!(91642))]),
        ),
        case(
            "OUTLOOK_GET_MAIL_TIPS",
            json!({"email_addresses": ["bob@fabrikam.com"]}),
            "POST",
            strs(vec!["/v1.0/me/getMailTips"]),
            slice(vec![
                ("EmailAddresses/0", json!("bob@fabrikam.com")),
                ("MailTipsOptions", json!(MAIL_TIPS)),
            ]),
            200,
            json!({"value": [{"emailAddress": {"address": "bob@fabrikam.com", "name": null},
                    "automaticReplies": {"message": "Out of office until Friday"}}]}),
            slice(vec![
                ("count", json!(1)),
                ("mail_tips/0/automaticReplies/message", json!("Out of office until Friday")),
            ]),
        ),
        case(
            "OUTLOOK_GET_SUPPORTED_TIME_ZONES",
            json!({}),
            "GET",
            strs(vec!["/v1.0/me/outlook/supportedTimeZones"]),
            slice(vec![]),
            200,
            json!({"@odata.context": "https://graph.microsoft.com/v1.0/$metadata#me/outlook/supportedTimeZones",
                "value": [{"alias": "Europe/Berlin"}, {"alias": "UTC"}]}),
            slice(vec![("count", json!(2)), ("time_zones/0/alias", json!("Europe/Berlin"))]),
        ),
        case(
            "OUTLOOK_LIST_FOCUSED_INBOX_OVERRIDES",
            json!({}),
            "GET",
            strs(vec!["/v1.0/me/inferenceClassification/overrides"]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "ov-1", "classifyAs": "focused",
                    "senderEmailAddress": {"name": "Ann", "address": "ann@contoso.com"}}]}),
            slice(vec![("count", json!(1)), ("overrides/0/id", json!("ov-1"))]),
        ),
        case(
            "OUTLOOK_LIST_MESSAGES_DELTA",
            json!({"folder_id": "inbox"}),
            "GET",
            strs(vec![
                "/v1.0/me/mailFolders/inbox/messages/delta?$select=",
                SEL_MSG,
            ]),
            slice(vec![]),
            200,
            json!({"value": [{"@removed": {"reason": "deleted"}, "id": "m-old"},
                    {"id": "m-2", "subject": "Kept"}],
                "@odata.deltaLink": "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta?$deltatoken=abc"}),
            slice(vec![
                ("count", json!(2)),
                ("changes/0/removed", json!(true)),
                ("changes/0/removed_reason", json!("deleted")),
                ("changes/1/removed", json!(false)),
                ("changes/1/removed_reason", json!(null)),
                ("delta_link", json!("https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta?$deltatoken=abc")),
            ]),
        ),
        // --- compose / send -------------------------------------------------------
        case(
            "OUTLOOK_SEND_EMAIL",
            json!({"recipients": ["ann@contoso.com"], "cc_recipients": ["bob@contoso.com"],
                "bcc_recipients": [], "subject": "Quarterly numbers", "body_format": "html",
                "body": "<p>Numbers attached.</p>"}),
            "POST",
            strs(vec!["/v1.0/me/sendMail"]),
            slice(vec![
                ("message/subject", json!("Quarterly numbers")),
                ("message/body/contentType", json!("html")),
                ("message/body/content", json!("<p>Numbers attached.</p>")),
                ("message/toRecipients/0/emailAddress/address", json!("ann@contoso.com")),
                ("message/ccRecipients/0/emailAddress/address", json!("bob@contoso.com")),
                ("message/bccRecipients", json!([])),
                ("saveToSentItems", json!("true")),
            ]),
            202,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "OUTLOOK_CREATE_DRAFT",
            json!({"subject": "Notes", "body_format": "text", "body": "Hello",
                "recipients": ["ann@contoso.com"], "folder_id": "drafts"}),
            "POST",
            strs(vec!["/v1.0/me/mailFolders/drafts/messages"]),
            slice(vec![
                ("subject", json!("Notes")),
                ("body/contentType", json!("text")),
                ("body/content", json!("Hello")),
                ("toRecipients/0/emailAddress/address", json!("ann@contoso.com")),
            ]),
            201,
            json!({"@odata.context": "https://graph.microsoft.com/v1.0/$metadata#me/messages/$entity",
                "id": "AAMkZHJhZnQ9", "subject": "Notes", "isDraft": true}),
            slice(vec![("id", json!("AAMkZHJhZnQ9")), ("isDraft", json!(true))]),
        ),
        case(
            "OUTLOOK_CREATE_DRAFT",
            json!({"subject": "Notes", "body_format": "text", "body": "Hello"}),
            "POST",
            strs(vec!["/v1.0/me/messages"]),
            slice(vec![("subject", json!("Notes")), ("toRecipients", json!([]))]),
            201,
            json!({"id": "AAMkZHJhZnQy", "subject": "Notes", "isDraft": true}),
            slice(vec![("id", json!("AAMkZHJhZnQy"))]),
        ),
        case(
            "OUTLOOK_SEND_DRAFT",
            json!({"message_id": "AAMkZHJhZnQ9"}),
            "POST",
            strs(vec!["/v1.0/me/messages/AAMkZHJhZnQ9/send"]),
            slice(vec![]),
            202,
            json!(null),
            slice(vec![("success", json!(true)), ("message", json!("Draft sent successfully."))]),
        ),
        case(
            "OUTLOOK_REPLY_TO_MESSAGE",
            json!({"message_id": "m1", "comment": "On it.", "cc_recipients": ["bob@contoso.com"]}),
            "POST",
            strs(vec!["/v1.0/me/messages/m1/reply"]),
            slice(vec![
                ("comment", json!("On it.")),
                ("message/ccRecipients/0/emailAddress/address", json!("bob@contoso.com")),
            ]),
            202,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "OUTLOOK_REPLY_ALL_TO_MESSAGE",
            json!({"message_id": "m1", "comment": "Thanks all."}),
            "POST",
            strs(vec!["/v1.0/me/messages/m1/replyAll"]),
            slice(vec![("comment", json!("Thanks all."))]),
            202,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "OUTLOOK_FORWARD_MESSAGE",
            json!({"message_id": "m1", "recipients": ["carol@fabrikam.com"], "comment": "FYI"}),
            "POST",
            strs(vec!["/v1.0/me/messages/m1/forward"]),
            slice(vec![
                ("comment", json!("FYI")),
                ("toRecipients/0/emailAddress/address", json!("carol@fabrikam.com")),
            ]),
            202,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        // --- organize messages ----------------------------------------------------
        case(
            "OUTLOOK_MOVE_MESSAGE",
            json!({"message_id": "m1", "destination_folder_id": "archive"}),
            "POST",
            strs(vec!["/v1.0/me/messages/m1/move"]),
            slice(vec![("destinationId", json!("archive"))]),
            201,
            json!({"id": "AAMkbW92ZWQ9", "parentFolderId": "archive"}),
            slice(vec![("success", json!(true)), ("new_message_id", json!("AAMkbW92ZWQ9"))]),
        ),
        case(
            "OUTLOOK_COPY_MESSAGE",
            json!({"message_id": "m1", "destination_folder_id": "archive"}),
            "POST",
            strs(vec!["/v1.0/me/messages/m1/copy"]),
            slice(vec![("destinationId", json!("archive"))]),
            201,
            json!({"id": "AAMka29waWVz", "parentFolderId": "archive"}),
            slice(vec![("new_message_id", json!("AAMka29waWVz"))]),
        ),
        case(
            "OUTLOOK_DELETE_MESSAGE",
            json!({"message_id": "m1"}),
            "DELETE",
            strs(vec!["/v1.0/me/messages/m1"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true)), ("message", json!("Message moved to Deleted Items."))]),
        ),
        case(
            "OUTLOOK_PERMANENTLY_DELETE_MESSAGE",
            json!({"message_id": "m1", "confirm": true}),
            "POST",
            strs(vec!["/v1.0/me/messages/m1/permanentDelete"]),
            slice(vec![]),
            202,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "OUTLOOK_SET_MESSAGE_READ_STATUS",
            json!({"message_id": "m1", "is_read": "false"}),
            "PATCH",
            strs(vec!["/v1.0/me/messages/m1"]),
            slice(vec![("isRead", json!(false))]),
            200,
            json!({"id": "m1", "isRead": false}),
            slice(vec![("isRead", json!(false))]),
        ),
        case(
            "OUTLOOK_UPDATE_MESSAGE",
            json!({"message_id": "m1", "is_read": "true", "flag_status": "flagged",
                "importance": "high", "subject": "Updated subject"}),
            "PATCH",
            strs(vec!["/v1.0/me/messages/m1"]),
            slice(vec![
                ("isRead", json!(true)),
                ("flag/flagStatus", json!("flagged")),
                ("importance", json!("high")),
                ("subject", json!("Updated subject")),
            ]),
            200,
            json!({"id": "m1", "isRead": true, "importance": "high",
                "flag": {"flagStatus": "flagged"}, "subject": "Updated subject"}),
            slice(vec![("importance", json!("high")), ("flag/flagStatus", json!("flagged"))]),
        ),
        case(
            "OUTLOOK_ADD_MESSAGE_CATEGORIES",
            json!({"message_id": "m1", "categories": ["Blue", "Red"], "current_categories": ["Red"]}),
            "PATCH",
            strs(vec!["/v1.0/me/messages/m1"]),
            slice(vec![("categories", json!(["Red", "Blue"]))]),
            200,
            json!({"id": "m1", "categories": ["Red", "Blue"]}),
            slice(vec![("categories/1", json!("Blue"))]),
        ),
        case(
            "OUTLOOK_REMOVE_MESSAGE_CATEGORIES",
            json!({"message_id": "m1", "categories_to_remove": ["Red"],
                "current_categories": ["Red", "Blue"]}),
            "PATCH",
            strs(vec!["/v1.0/me/messages/m1"]),
            slice(vec![("categories", json!(["Blue"]))]),
            200,
            json!({"id": "m1", "categories": ["Blue"]}),
            slice(vec![("categories/0", json!("Blue"))]),
        ),
        case(
            "OUTLOOK_BATCH_MOVE_MESSAGES",
            json!({"message_ids": ["m1", "m2"], "destination_folder_id": "archive"}),
            "POST",
            strs(vec!["/v1.0/me/$batch"]),
            slice(vec![
                ("requests/0/id", json!("1")),
                ("requests/0/method", json!("POST")),
                ("requests/0/url", json!("/me/messages/m1/move")),
                ("requests/0/body/destinationId", json!("archive")),
                ("requests/1/id", json!("2")),
                ("requests/1/url", json!("/me/messages/m2/move")),
            ]),
            200,
            json!({"responses": [
                {"id": "1", "status": 200, "body": {"id": "AAMkbTI9"}},
                {"id": "2", "status": 404,
                 "body": {"error": {"code": "ErrorItemNotFound", "message": "The specified object was not found."}}},
            ]}),
            slice(vec![
                ("success", json!(false)),
                ("succeeded_count", json!(1)),
                ("failed_count", json!(1)),
                ("succeeded/0/new_message_id", json!("AAMkbTI9")),
                ("failed/0/request_id", json!("2")),
                ("failed/0/status", json!(404)),
                ("failed/0/error", json!("ErrorItemNotFound: The specified object was not found.")),
            ]),
        ),
        case(
            "OUTLOOK_BATCH_UPDATE_MESSAGES",
            json!({"message_ids": ["m1", "m2"], "is_read": "true", "importance": "low"}),
            "POST",
            strs(vec!["/v1.0/me/$batch"]),
            slice(vec![
                ("requests/0/method", json!("PATCH")),
                ("requests/0/url", json!("/me/messages/m1")),
                ("requests/0/body/isRead", json!(true)),
                ("requests/0/body/importance", json!("low")),
                ("requests/1/url", json!("/me/messages/m2")),
            ]),
            200,
            json!({"responses": [{"id": "1", "status": 200, "body": {}},
                {"id": "2", "status": 200, "body": {}}]}),
            slice(vec![
                ("success", json!(true)),
                ("succeeded_count", json!(2)),
                ("failed_count", json!(0)),
            ]),
        ),
        // --- folders ---------------------------------------------------------------
        case(
            "OUTLOOK_CREATE_MAIL_FOLDER",
            json!({"display_name": "Projects"}),
            "POST",
            strs(vec!["/v1.0/me/mailFolders"]),
            slice(vec![("displayName", json!("Projects"))]),
            201,
            json!({"id": "AAMkZm9sZDE9", "displayName": "Projects", "childFolderCount": 0}),
            slice(vec![("created", json!(true)), ("folder_id", json!("AAMkZm9sZDE9"))]),
        ),
        case(
            "OUTLOOK_CREATE_MAIL_FOLDER",
            json!({"display_name": "Projects", "parent_folder_id": "inbox"}),
            "POST",
            strs(vec!["/v1.0/me/mailFolders/inbox/childFolders"]),
            slice(vec![("displayName", json!("Projects"))]),
            201,
            json!({"id": "AAMkZm9sZDI9", "displayName": "Projects", "parentFolderId": "inbox"}),
            slice(vec![("folder_id", json!("AAMkZm9sZDI9"))]),
        ),
        case(
            "OUTLOOK_RENAME_MAIL_FOLDER",
            json!({"folder_id": "f1", "display_name": "Archive 2026"}),
            "PATCH",
            strs(vec!["/v1.0/me/mailFolders/f1"]),
            slice(vec![("displayName", json!("Archive 2026"))]),
            200,
            json!({"id": "f1", "displayName": "Archive 2026"}),
            slice(vec![("displayName", json!("Archive 2026"))]),
        ),
        case(
            "OUTLOOK_DELETE_MAIL_FOLDER",
            json!({"folder_id": "f1"}),
            "DELETE",
            strs(vec!["/v1.0/me/mailFolders/f1"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true)), ("message", json!("Mail folder deleted."))]),
        ),
        case(
            "OUTLOOK_MOVE_MAIL_FOLDER",
            json!({"folder_id": "f1", "destination_folder_id": "inbox"}),
            "POST",
            strs(vec!["/v1.0/me/mailFolders/f1/move"]),
            slice(vec![("destinationId", json!("inbox"))]),
            201,
            json!({"id": "AAMkZjItbmV3", "parentFolderId": "inbox"}),
            slice(vec![("success", json!(true)), ("new_folder_id", json!("AAMkZjItbmV3"))]),
        ),
        case(
            "OUTLOOK_COPY_MAIL_FOLDER",
            json!({"folder_id": "f1", "destination_folder_id": "inbox"}),
            "POST",
            strs(vec!["/v1.0/me/mailFolders/f1/copy"]),
            slice(vec![("destinationId", json!("inbox"))]),
            201,
            json!({"id": "AAMkZjIta29w", "parentFolderId": "inbox"}),
            slice(vec![("new_folder_id", json!("AAMkZjIta29w"))]),
        ),
        // --- focused inbox overrides -------------------------------------------------
        case(
            "OUTLOOK_CREATE_FOCUSED_INBOX_OVERRIDE",
            json!({"sender_email_address": "news@fabrikam.com", "classify_as": "other",
                "sender_name": "Fabrikam News"}),
            "POST",
            strs(vec!["/v1.0/me/inferenceClassification/overrides"]),
            slice(vec![
                ("classifyAs", json!("other")),
                ("senderEmailAddress/address", json!("news@fabrikam.com")),
                ("senderEmailAddress/name", json!("Fabrikam News")),
            ]),
            201,
            json!({"@odata.context": "https://graph.microsoft.com/v1.0/$metadata#me/inferenceClassification/overrides/$entity",
                "id": "ov-2", "classifyAs": "other",
                "senderEmailAddress": {"name": "Fabrikam News", "address": "news@fabrikam.com"}}),
            slice(vec![("id", json!("ov-2")), ("classifyAs", json!("other"))]),
        ),
        case(
            "OUTLOOK_UPDATE_FOCUSED_INBOX_OVERRIDE",
            json!({"override_id": "ov-1", "classify_as": "focused"}),
            "PATCH",
            strs(vec!["/v1.0/me/inferenceClassification/overrides/ov-1"]),
            slice(vec![("classifyAs", json!("focused"))]),
            200,
            json!({"id": "ov-1", "classifyAs": "focused"}),
            slice(vec![("classifyAs", json!("focused"))]),
        ),
        case(
            "OUTLOOK_DELETE_FOCUSED_INBOX_OVERRIDE",
            json!({"override_id": "ov-1"}),
            "DELETE",
            strs(vec!["/v1.0/me/inferenceClassification/overrides/ov-1"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "OUTLOOK_DELETE_MESSAGE_ATTACHMENT",
            json!({"message_id": "m1", "attachment_id": "att-1"}),
            "DELETE",
            strs(vec!["/v1.0/me/messages/m1/attachments/att-1"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
    ]
}

#[test]
fn recorded_requests_and_responses() {
    let a = Outlook::with_api_base("https://graph.test");
    let all = cases();
    support::assert_full_coverage(&a, &all);
    for c in &all {
        support::run(&a, c);
    }
}

/// Arguments that the TypeScript run() code refuses must fail here too.
#[test]
fn rejects_invalid_arguments() {
    let a = Outlook::with_api_base("https://graph.test");
    let b = |tool: &str, args: Value| a.build_request(tool, &args);

    assert!(b(
        "OUTLOOK_PERMANENTLY_DELETE_MESSAGE",
        json!({"message_id": "m1", "confirm": false})
    )
    .is_err());
    assert!(b("OUTLOOK_UPDATE_MESSAGE", json!({"message_id": "m1"})).is_err());
    assert!(b(
        "OUTLOOK_BATCH_UPDATE_MESSAGES",
        json!({"message_ids": ["m1"]})
    )
    .is_err());
    assert!(b("OUTLOOK_GET_MAIL_TIPS", json!({"email_addresses": []})).is_err());
    assert!(b(
        "OUTLOOK_BATCH_MOVE_MESSAGES",
        json!({"message_ids": (0..21).map(|i| format!("m{i}")).collect::<Vec<_>>(),
               "destination_folder_id": "archive"})
    )
    .is_err());

    // Graph IDs are opaque (they contain `=`, spaces, slashes): they are
    // percent-encoded into one path segment instead of rejected.
    let r = b(
        "OUTLOOK_SEARCH_MESSAGES",
        json!({"search_query": "x", "folder_id": "../evil"})
    )
    .unwrap();
    assert_eq!(
        r.url,
        "https://graph.test/v1.0/me/mailFolders/..%2Fevil/messages?$search=%22x%22&$top=25"
    );
}

/// Live calls against Microsoft Graph; set
/// CTOX_TOOLHUB_LIVE_OUTLOOK_TOKEN=... to run, otherwise skipped silently.
#[test]
#[ignore]
fn live_read_only_calls() {
    let a = Outlook::default();
    for (tool, args) in [
        ("OUTLOOK_GET_MY_PROFILE", json!({})),
        ("OUTLOOK_LIST_MAIL_FOLDERS", json!({"max_results": 5})),
        ("OUTLOOK_LIST_MESSAGES", json!({"max_results": 5})),
    ] {
        let Some(result) = support::live(&a, tool, args) else {
            continue; // no token configured
        };
        let out = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(out.is_object(), "{tool}: unexpected output {out}");
    }
}
