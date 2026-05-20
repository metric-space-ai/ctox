use std::io;
use std::net::TcpListener;
use std::sync::{Mutex, OnceLock};

const START_PORT: u16 = 18_669;
const PORT_MAX: u16 = u16::MAX;

static START_PORT_STATE: OnceLock<Mutex<u16>> = OnceLock::new();

pub fn next_port() -> io::Result<u16> {
    let state = START_PORT_STATE.get_or_init(|| Mutex::new(START_PORT));
    let mut start_port = state.lock().expect("port-manager mutex poisoned");

    for port in *start_port..=PORT_MAX {
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            *start_port = port.saturating_add(1);
            return Ok(port);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "no free conformance test port available",
    ))
}

#[test]
fn next_port_returns_monotonic_bindable_ports() {
    let first = next_port().unwrap();
    let second = next_port().unwrap();

    assert!(first >= START_PORT);
    assert!(second > first);
    let _listener = TcpListener::bind(("0.0.0.0", first)).unwrap();
}
