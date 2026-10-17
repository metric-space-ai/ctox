// ref: internal/tui/oauth_tab_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::oauth_tab::{
    should_accept_oauth_poll, should_accept_oauth_start, should_fail_oauth_status_poll,
    OAuthPollMessage, OAuthStartMessage, OAuthState, OAuthTabModel,
};
use super::{
    run_terminal, App, KeyCode, KeyEvent, SystemTerminalClock, TerminalBackend, TerminalEvent,
};
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

struct FakeTerminal {
    events: VecDeque<TerminalEvent>,
    rendered: Vec<String>,
    entered: bool,
    left: bool,
    fail_render: bool,
}
impl TerminalBackend for FakeTerminal {
    fn enter(&mut self) -> io::Result<()> {
        self.entered = true;
        Ok(())
    }
    fn size(&self) -> io::Result<(u16, u16)> {
        Ok((48, 12))
    }
    fn read_event(&mut self, _timeout: Duration) -> io::Result<Option<TerminalEvent>> {
        Ok(self.events.pop_front())
    }
    fn render(&mut self, snapshot: &str) -> io::Result<()> {
        self.rendered.push(snapshot.to_owned());
        if self.fail_render {
            Err(io::Error::other("render failed"))
        } else {
            Ok(())
        }
    }
    fn leave(&mut self) -> io::Result<()> {
        self.left = true;
        Ok(())
    }
}
