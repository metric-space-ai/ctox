// ref: mailserver/tests/conformance_test.rs
// ref: ctox-mailserver conformance tests verifying SMTP, DKIM, iCal/vCard, and SQLite store functionality

use ctox_mailserver::calcard::{ICalendar, VCard};
use ctox_mailserver::smtp::dsn::parse_dsn_report;
use ctox_mailserver::smtp::dkim::DkimSigner;
use ctox_mailserver::store::SqliteStore;
use ctox_mailserver::smtp::client_queue::SmtpOutboundQueue;
use ctox_mailserver::smtp::server::SmtpInboundServer;


#[test]
fn test_ical_parsing() {
    let ical_data = r#"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:test-event-123
SUMMARY:CTOX Sync Meeting
DTSTART:20260520T100000Z
DTEND:20260520T110000Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
END:VCALENDAR"#;

    let ical = ICalendar::parse(ical_data).unwrap();
    assert_eq!(ical.events.len(), 1);
    let event = &ical.events[0];
    assert_eq!(event.uid, "test-event-123");
    assert_eq!(event.summary, "CTOX Sync Meeting");
    assert_eq!(event.dtstart, "20260520T100000Z");
    assert_eq!(event.dtend, "20260520T110000Z");
    assert_eq!(event.rrule.as_deref(), Some("FREQ=DAILY;COUNT=5"));
}

#[test]
fn test_vcard_parsing() {
    let vcard_data = r#"BEGIN:VCARD
VERSION:4.0
FN:Michael Welsch
EMAIL:michael@ctox.local
TEL:+49-123-456789
END:VCARD"#;

    let vcard = VCard::parse(vcard_data).unwrap();
    assert_eq!(vcard.fn_name, "Michael Welsch");
    assert_eq!(vcard.emails.len(), 1);
    assert_eq!(vcard.emails[0], "michael@ctox.local");
    assert_eq!(vcard.tels.len(), 1);
    assert_eq!(vcard.tels[0], "+49-123-456789");
}

#[test]
fn test_dkim_signing() {
    let signer = DkimSigner::new("selector", "ctox.local", "dGVzdA==").unwrap();
    let signed = signer.sign("sender@ctox.local", "Hello World").unwrap();
    assert!(signed.contains("DKIM-Signature:"));
    assert!(signed.contains("Hello World"));
}

#[test]
fn test_sqlite_store_operations() {
    // Open SQLite store in a temp file
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.sqlite");
    let store = SqliteStore::new(db_path.to_str().unwrap());
    store.init().unwrap();

    // Test Domain adding and retrieving
    store.add_domain("ctox.local", "default", "pem_key_data").unwrap();
    let dkim = store.get_domain_dkim("ctox.local").unwrap().unwrap();
    assert_eq!(dkim.0, "default");
    assert_eq!(dkim.1, "pem_key_data");

    // Test SMTP queueing
    let id = store.queue_email("sender@ctox.local", "receiver@ctox.local", "Test Email Body").unwrap();
    let pending = store.get_pending_emails().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, id);
    assert_eq!(pending[0].1, "sender@ctox.local");
    assert_eq!(pending[0].2, "receiver@ctox.local");
    assert_eq!(pending[0].3, "Test Email Body");

    // Delete email
    store.delete_email(&id).unwrap();
    assert_eq!(store.get_pending_emails().unwrap().len(), 0);

    // Test CalDAV event putting and getting
    store.create_calendar("test_cal", "owner@ctox.local", "Test Calendar").unwrap();
    store.put_event("test_cal", "event_1", "ical_data_string").unwrap();
    let events = store.get_events("test_cal").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, "event_1");
    assert_eq!(events[0].2, "ical_data_string");
}

#[test]
fn test_bounce_report_parsing() {
    let bounce_email = "Subject: Undeliverable Mail\r\n\r\nFinal-Recipient: rfc822; user-not-found@ctox.local\r\nStatus: 5.1.1\r\n";
    let report = parse_dsn_report(bounce_email).unwrap();
    assert_eq!(report.recipient, "user-not-found@ctox.local");
    assert_eq!(report.status_code, "5.1.1");
    assert!(report.is_hard_bounce);

    let soft_bounce = "Subject: Mailbox Full\r\n\r\nFinal-Recipient: rfc822; full-box@ctox.local\r\nStatus: 4.2.2\r\n";
    let report_soft = parse_dsn_report(soft_bounce).unwrap();
    assert_eq!(report_soft.recipient, "full-box@ctox.local");
    assert_eq!(report_soft.status_code, "4.2.2");
    assert!(!report_soft.is_hard_bounce);
}

async fn read_until(stream: &mut tokio::net::TcpStream, expected: &str) -> String {
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; 1024];
    let mut accumulated = String::new();
    loop {
        let n = stream.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        accumulated.push_str(&String::from_utf8_lossy(&buf[..n]));
        if accumulated.contains(expected) {
            break;
        }
    }
    accumulated
}

#[tokio::test]
async fn test_smtp_and_imap_conformance() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.sqlite");
    let store = SqliteStore::new(db_path.to_str().unwrap());
    store.init().unwrap();

    // Register a local user
    store.add_user("test@ctox.local", "securepass").unwrap();
    assert!(store.user_exists("test@ctox.local").unwrap());

    // Setup Smtp and Imap servers
    let smtp_addr: std::net::SocketAddr = "127.0.0.1:25250".parse().unwrap();
    let imap_addr: std::net::SocketAddr = "127.0.0.1:11430".parse().unwrap();

    let smtp_config = ctox_mailserver::config::SmtpConfig {
        bind_address: smtp_addr,
        outbound_throttle_per_min: 100,
        max_connections: 10,
    };
    let imap_config = ctox_mailserver::config::ImapConfig {
        bind_address: imap_addr,
    };

    let smtp_server = std::sync::Arc::new(ctox_mailserver::smtp::server::SmtpInboundServer::new(
        store.clone(),
        smtp_config,
    ));
    let imap_server = std::sync::Arc::new(ctox_mailserver::imap::ImapServer::new(
        store.clone(),
        imap_config,
    ));

    // Spawn servers
    let smtp_handle = tokio::spawn(async move {
        let _ = smtp_server.start().await;
    });
    let imap_handle = tokio::spawn(async move {
        let _ = imap_server.start().await;
    });

    // Wait a brief moment for servers to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 1. SMTP: Connect and send an email
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(smtp_addr).await.unwrap();
        
        let mut response = vec![0u8; 1024];
        let n = stream.read(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response[..n]).contains("220"));

        stream.write_all(b"EHLO localhost\r\n").await.unwrap();
        let n = stream.read(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response[..n]).contains("250"));

        stream.write_all(b"MAIL FROM:<sender@ctox.local>\r\n").await.unwrap();
        let n = stream.read(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response[..n]).contains("250"));

        stream.write_all(b"RCPT TO:<test@ctox.local>\r\n").await.unwrap();
        let n = stream.read(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response[..n]).contains("250"));

        stream.write_all(b"DATA\r\n").await.unwrap();
        let n = stream.read(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response[..n]).contains("354"));

        let email_content = "Subject: Hello CTOX\r\nFrom: sender@ctox.local\r\nTo: test@ctox.local\r\n\r\nThis is a test email body.\r\n.\r\n";
        stream.write_all(email_content.as_bytes()).await.unwrap();
        let n = stream.read(&mut response).await.unwrap();
        assert!(String::from_utf8_lossy(&response[..n]).contains("250"));

        stream.write_all(b"QUIT\r\n").await.unwrap();
    }

    // Verify database contains the message
    let inbox_id = store.get_mailbox_id("test@ctox.local", "INBOX").unwrap().unwrap();
    let messages = store.get_messages(&inbox_id).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].3.as_deref(), Some("Hello CTOX"));
    assert_eq!(messages[0].4.trim(), "This is a test email body.");

    // 2. IMAP: Connect, login, select mailbox, and fetch message
    {
        use tokio::io::AsyncWriteExt;
        let mut stream = tokio::net::TcpStream::connect(imap_addr).await.unwrap();
        
        let welcome = read_until(&mut stream, "OK").await;
        assert!(welcome.contains("OK"));

        stream.write_all(b"A01 LOGIN test@ctox.local securepass\r\n").await.unwrap();
        let login_res = read_until(&mut stream, "A01 OK").await;
        assert!(login_res.contains("A01 OK"));

        stream.write_all(b"A02 SELECT INBOX\r\n").await.unwrap();
        let select_res = read_until(&mut stream, "A02 OK").await;
        assert!(select_res.contains("* 1 EXISTS"));
        assert!(select_res.contains("A02 OK"));

        stream.write_all(b"A03 FETCH 1 (UID FLAGS BODY.PEEK[])\r\n").await.unwrap();
        let fetch_res = read_until(&mut stream, "A03 OK").await;
        assert!(fetch_res.contains("* 1 FETCH"));
        assert!(fetch_res.contains("Hello CTOX"));
        assert!(fetch_res.contains("This is a test email body."));
        assert!(fetch_res.contains("A03 OK"));

        // Store flag check
        stream.write_all(b"A04 STORE 1 +FLAGS (\\Seen)\r\n").await.unwrap();
        let store_res = read_until(&mut stream, "A04 OK").await;
        assert!(store_res.contains("FLAGS (\\Seen)"));
        assert!(store_res.contains("A04 OK"));

        stream.write_all(b"A05 LOGOUT\r\n").await.unwrap();
        let _logout_res = read_until(&mut stream, "A05 OK").await;
    }

    // Clean up tasks
    smtp_handle.abort();
    imap_handle.abort();
}

#[tokio::test]
async fn test_outbound_queue_pooling() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.sqlite");
    let store = SqliteStore::new(db_path.to_str().unwrap());
    store.init().unwrap();

    // Register local recipients to receive emails
    store.add_user("alice@domaina.com", "pass1").unwrap();
    store.add_user("bob@domainb.com", "pass2").unwrap();

    // Bind address for the test SMTP server
    let smtp_addr: std::net::SocketAddr = "127.0.0.1:25251".parse().unwrap();
    let smtp_config = ctox_mailserver::config::SmtpConfig {
        bind_address: smtp_addr,
        outbound_throttle_per_min: 100,
        max_connections: 10,
    };

    let smtp_server = std::sync::Arc::new(SmtpInboundServer::new(
        store.clone(),
        smtp_config.clone(),
    ));

    // Spawn server
    let smtp_handle = tokio::spawn(async move {
        let _ = smtp_server.start().await;
    });

    // Wait a brief moment for server to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Queue three emails: two for domaina.com and one for domainb.com
    store.queue_email("sender@domainc.com", "alice@domaina.com", "Hello Alice 1").unwrap();
    store.queue_email("sender@domainc.com", "alice@domaina.com", "Hello Alice 2").unwrap();
    store.queue_email("sender@domainc.com", "bob@domainb.com", "Hello Bob").unwrap();

    // Verify queue has 3 pending emails
    assert_eq!(store.get_pending_emails().unwrap().len(), 3);

    // Run SmtpOutboundQueue's process_queue to deliver them
    let queue = SmtpOutboundQueue::new(store.clone(), smtp_config);
    queue.process_queue().await.unwrap();

    // Assert they are successfully processed and deleted from the queue
    assert_eq!(store.get_pending_emails().unwrap().len(), 0);

    // Verify both inboxes now contain their respective delivered messages!
    let inbox_a = store.get_mailbox_id("alice@domaina.com", "INBOX").unwrap().unwrap();
    let messages_a = store.get_messages(&inbox_a).unwrap();
    assert_eq!(messages_a.len(), 2);
    
    let bodies_a: Vec<String> = messages_a.iter().map(|m| m.4.trim().to_string()).collect();
    assert!(bodies_a.contains(&"Hello Alice 1".to_string()));
    assert!(bodies_a.contains(&"Hello Alice 2".to_string()));

    let inbox_b = store.get_mailbox_id("bob@domainb.com", "INBOX").unwrap().unwrap();
    let messages_b = store.get_messages(&inbox_b).unwrap();
    assert_eq!(messages_b.len(), 1);
    assert_eq!(messages_b[0].4.trim(), "Hello Bob");

    // Clean up
    smtp_handle.abort();
}

#[test]
fn test_greylisting_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.sqlite");
    let store = SqliteStore::new(db_path.to_str().unwrap());
    store.init().unwrap();

    // 1. Loopback addresses must bypass greylisting immediately (return true)
    assert!(store.check_greylist("127.0.0.1", "spammer@bad.com", "victim@local.com").unwrap());
    assert!(store.check_greylist("::1", "spammer@bad.com", "victim@local.com").unwrap());

    // 2. An external IP must trigger a 450 cooling-off period (returns false on first check)
    let external_ip = "192.168.1.50";
    let is_allowed_first = store.check_greylist(external_ip, "spammer@bad.com", "victim@local.com").unwrap();
    assert!(!is_allowed_first); // Should be greylisted/blocked first

    // A repeat check immediately after should still return false
    let is_allowed_second = store.check_greylist(external_ip, "spammer@bad.com", "victim@local.com").unwrap();
    assert!(!is_allowed_second);
}


