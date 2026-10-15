//! E14: acknowledgement is application delivery, never SCTP handoff.
//! This state belongs to an authenticated session, not a DataChannel generation.
use super::super::hello::{HelloFrame, WireSeq};
use super::super::lanes::Lane;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum SessionError {
    #[error("invalid reliable session or cursor")]
    Invalid,
    #[error("reliable session lane retention limit exceeded")]
    Limit,
    #[error("reliable session was retired or superseded")]
    Retired,
    #[error("reliable session received a sequence gap")]
    Gap,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionHello {
    pub session_id: String,
    pub resume_from: [WireSeq; 6],
}
impl SessionHello {
    pub fn validate(&self) -> Result<(), SessionError> {
        if self.session_id.is_empty()
            || self.session_id.len() > 128
            || !self
                .session_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(SessionError::Invalid);
        }
        Ok(())
    }
    /// P4 seam: parse the extension BEFORE the strict P4 hello decoder. Unknown
    /// fields still fail there. No unauthenticated offer attaches session state.
    pub fn decode(bytes: &[u8]) -> Result<(HelloFrame, Self), SessionError> {
        if bytes.len() > super::super::hello::MAX_HELLO_BYTES {
            return Err(SessionError::Limit);
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| SessionError::Invalid)?;
        let object = value.as_object_mut().ok_or(SessionError::Invalid)?;
        if !matches!(
            object.get("type").and_then(|v| v.as_str()),
            Some("hello" | "hello_ok")
        ) {
            return Err(SessionError::Invalid);
        }
        let session = serde_json::from_value::<Self>(serde_json::json!({
            "session_id": object.remove("session_id").ok_or(SessionError::Invalid)?,
            "resume_from": object.remove("resume_from").ok_or(SessionError::Invalid)?
        }))
        .map_err(|_| SessionError::Invalid)?;
        session.validate()?;
        let frame =
            HelloFrame::decode(&serde_json::to_vec(&value).map_err(|_| SessionError::Invalid)?)
                .map_err(|_| SessionError::Invalid)?;
        Ok((frame, session))
    }
    pub fn encode(&self, frame: &HelloFrame) -> Result<Vec<u8>, SessionError> {
        self.validate()?;
        let mut value = serde_json::to_value(frame).map_err(|_| SessionError::Invalid)?;
        let object = value.as_object_mut().ok_or(SessionError::Invalid)?;
        if !matches!(
            object.get("type").and_then(|v| v.as_str()),
            Some("hello" | "hello_ok")
        ) {
            return Err(SessionError::Invalid);
        }
        object.insert("session_id".into(), self.session_id.clone().into());
        object.insert(
            "resume_from".into(),
            serde_json::to_value(self.resume_from).unwrap(),
        );
        let bytes = serde_json::to_vec(&value).map_err(|_| SessionError::Invalid)?;
        Self::decode(&bytes)?;
        Ok(bytes)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionControl {
    Ack {
        session_id: String,
        lane: Lane,
        upto: WireSeq,
    },
}
// Lane is an integer on the wire (its ordinary enum serde form is not).
impl SessionControl {
    pub fn encode(&self) -> Vec<u8> {
        let Self::Ack {
            session_id,
            lane,
            upto,
        } = self;
        serde_json::to_vec(&serde_json::json!({"type":"ack", "session_id":session_id, "lane":lane.index(), "upto":upto})).unwrap()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SessionError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Ack {
            r#type: String,
            session_id: String,
            lane: usize,
            upto: WireSeq,
        }
        if bytes.len() > 512 {
            return Err(SessionError::Limit);
        }
        let ack: Ack = serde_json::from_slice(bytes).map_err(|_| SessionError::Invalid)?;
        if ack.r#type != "ack" {
            return Err(SessionError::Invalid);
        }
        Ok(Self::Ack {
            session_id: ack.session_id,
            lane: Lane::from_index(ack.lane).ok_or(SessionError::Invalid)?,
            upto: ack.upto,
        })
    }
}
pub struct Retained<T> {
    pub seq: WireSeq,
    pub value: T,
    _permit: OwnedSemaphorePermit,
}
struct State<T> {
    sent: [u64; 6],
    acked: [u64; 6],
    delivered: [u64; 6],
    retained: [VecDeque<Arc<Retained<T>>>; 6],
    generation: u64,
    retired: bool,
}
/// Keep this Arc across associations ONLY after authenticating the same owner,
/// instance, epoch and rights. A new epoch constructs a new session explicitly.
pub struct ReliableSession<T> {
    id: String,
    limits: [usize; 6],
    permits: [Arc<Semaphore>; 6],
    state: Mutex<State<T>>,
    changed: tokio::sync::watch::Sender<u64>,
}
impl<T> ReliableSession<T> {
    pub fn new(id: String, limits: [usize; 6]) -> Result<Arc<Self>, SessionError> {
        SessionHello {
            session_id: id.clone(),
            resume_from: [WireSeq(0); 6],
        }
        .validate()?;
        if limits.iter().any(|n| *n == 0 || *n > u32::MAX as usize) {
            return Err(SessionError::Limit);
        }
        Ok(Arc::new(Self {
            id,
            limits,
            changed: tokio::sync::watch::channel(0).0,
            permits: std::array::from_fn(|i| Arc::new(Semaphore::new(limits[i]))),
            state: Mutex::new(State {
                sent: [0; 6],
                acked: [0; 6],
                delivered: [0; 6],
                retained: std::array::from_fn(|_| VecDeque::new()),
                generation: 0,
                retired: false,
            }),
        }))
    }
    pub fn hello(&self) -> SessionHello {
        SessionHello {
            session_id: self.id.clone(),
            resume_from: self.state.lock().delivered.map(WireSeq),
        }
    }
    /// Rebind atomically invalidates old delivery tokens. Retention itself is
    /// untouched until all remote cursors have been validated together.
    pub fn attach(&self, remote: &SessionHello) -> Result<u64, SessionError> {
        remote.validate()?;
        let mut state = self.state.lock();
        if state.retired || remote.session_id != self.id {
            return Err(SessionError::Retired);
        }
        for i in 0..6 {
            if remote.resume_from[i].0 < state.acked[i] || remote.resume_from[i].0 > state.sent[i] {
                return Err(SessionError::Invalid);
            }
        }
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or(SessionError::Retired)?;
        for i in 0..6 {
            Self::ack_lane(&mut state, i, remote.resume_from[i].0);
        }
        self.changed.send_replace(state.generation);
        Ok(state.generation)
    }
    pub fn check_generation(&self, generation: u64) -> Result<(), SessionError> {
        let state = self.state.lock();
        if state.retired || generation != state.generation {
            Err(SessionError::Retired)
        } else {
            Ok(())
        }
    }
    pub async fn retain(
        &self,
        generation: u64,
        lane: Lane,
        bytes: usize,
        value: T,
    ) -> Result<Arc<Retained<T>>, SessionError> {
        let i = lane.index();
        // Charge metadata too: millions of empty messages cannot bypass the
        // byte budget by retaining Arc/deque/receipt allocations for free.
        let bytes = bytes.max(1).saturating_add(256);
        if bytes > self.limits[i] {
            return Err(SessionError::Limit);
        }
        let mut changed = self.changed.subscribe();
        self.check_generation(generation)?;
        let permit = tokio::select! {
            biased;
            _ = changed.changed() => return Err(SessionError::Retired),
            permit = self.permits[i].clone().acquire_many_owned(bytes as u32) => permit.map_err(|_| SessionError::Retired)?,
        };
        let mut state = self.state.lock();
        if state.retired || generation != state.generation {
            return Err(SessionError::Retired);
        }
        let seq = state.sent[i].checked_add(1).ok_or(SessionError::Retired)?;
        let message = Arc::new(Retained {
            seq: WireSeq(seq),
            value,
            _permit: permit,
        });
        state.sent[i] = seq;
        state.retained[i].push_back(message.clone());
        Ok(message)
    }
    pub fn replay(
        &self,
        generation: u64,
        lane: Lane,
    ) -> Result<Vec<Arc<Retained<T>>>, SessionError> {
        let state = self.state.lock();
        if state.retired || generation != state.generation {
            return Err(SessionError::Retired);
        }
        Ok(state.retained[lane.index()].iter().cloned().collect())
    }
    fn ack_lane(state: &mut State<T>, lane: usize, upto: u64) {
        state.acked[lane] = upto;
        while state.retained[lane]
            .front()
            .is_some_and(|m| m.seq.0 <= upto)
        {
            state.retained[lane].pop_front();
        }
    }
    pub fn acknowledge(&self, generation: u64, ack: &SessionControl) -> Result<(), SessionError> {
        let SessionControl::Ack {
            session_id,
            lane,
            upto,
        } = ack;
        let mut state = self.state.lock();
        let i = lane.index();
        if state.retired || state.generation != generation || session_id != &self.id {
            return Err(SessionError::Retired);
        }
        if upto.0 > state.sent[i] {
            return Err(SessionError::Invalid);
        }
        // Delayed cumulative ACKs are harmless; never move a cursor backwards.
        if upto.0 > state.acked[i] {
            Self::ack_lane(&mut state, i, upto.0);
        }
        Ok(())
    }
    /// Returns false for an already DELIVERED duplicate. A pending delivery
    /// does not advance this cursor and is therefore replayed after detach.
    pub fn should_deliver(
        &self,
        generation: u64,
        lane: Lane,
        seq: WireSeq,
    ) -> Result<bool, SessionError> {
        let state = self.state.lock();
        if state.retired || generation != state.generation {
            return Err(SessionError::Retired);
        }
        let upto = state.delivered[lane.index()];
        if seq.0 <= upto {
            return Ok(false);
        }
        if upto.checked_add(1) != Some(seq.0) {
            return Err(SessionError::Gap);
        }
        Ok(true)
    }
    pub fn delivered(
        &self,
        generation: u64,
        lane: Lane,
        seq: WireSeq,
    ) -> Result<SessionControl, SessionError> {
        let mut state = self.state.lock();
        if state.retired || generation != state.generation {
            return Err(SessionError::Retired);
        }
        let upto = &mut state.delivered[lane.index()];
        if seq.0 > *upto {
            if upto.checked_add(1) != Some(seq.0) {
                return Err(SessionError::Gap);
            }
            *upto = seq.0;
        }
        Ok(SessionControl::Ack {
            session_id: self.id.clone(),
            lane,
            upto: WireSeq(*upto),
        })
    }
    pub fn with_generation<R>(
        &self,
        generation: u64,
        action: impl FnOnce() -> R,
    ) -> Result<R, SessionError> {
        let state = self.state.lock();
        if state.retired || state.generation != generation {
            return Err(SessionError::Retired);
        }
        Ok(action())
    }
    pub fn retire(&self) {
        let mut state = self.state.lock();
        state.retired = true;
        self.changed.send_replace(0);
        for queue in &mut state.retained {
            queue.clear();
        }
        for permits in &self.permits {
            permits.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn v4_session_resume_retains_lost_messages_drops_duplicates_and_bounds_each_lane() {
        let session = ReliableSession::new("same-session".into(), [260; 6]).unwrap();
        let zero = session.hello();
        let first = session.attach(&zero).unwrap();
        let a = session
            .retain(first, Lane::Commands, 4, b"lost".to_vec())
            .await
            .unwrap();
        assert_eq!(a.seq, WireSeq(1));
        drop(a);
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(5),
            session.retain(first, Lane::Commands, 1, vec![])
        )
        .await
        .is_err());
        let control = session
            .retain(first, Lane::Control, 1, vec![0])
            .await
            .unwrap();
        drop(control);
        let second = session.attach(&zero).unwrap();
        assert!(session.check_generation(first).is_err());
        let replay = session.replay(second, Lane::Commands).unwrap();
        assert_eq!(
            replay
                .iter()
                .map(|m| (m.seq, m.value.clone()))
                .collect::<Vec<_>>(),
            vec![(WireSeq(1), b"lost".to_vec())]
        );
        drop(replay);
        assert!(session
            .should_deliver(second, Lane::Commands, WireSeq(1))
            .unwrap());
        let ack = session
            .delivered(second, Lane::Commands, WireSeq(1))
            .unwrap();
        assert!(!session
            .should_deliver(second, Lane::Commands, WireSeq(1))
            .unwrap());
        assert!(session
            .should_deliver(second, Lane::Commands, WireSeq(3))
            .is_err());
        session.acknowledge(second, &ack).unwrap();
        assert!(session.replay(second, Lane::Commands).unwrap().is_empty());
        let b = session
            .retain(second, Lane::Commands, 4, b"next".to_vec())
            .await
            .unwrap();
        assert_eq!(b.seq, WireSeq(2));
        drop(b);
        let mut forged = zero.clone();
        forged.resume_from[1] = WireSeq(3);
        assert!(session.attach(&forged).is_err());
        assert!(session.check_generation(second).is_ok());
        let resumed = session.hello();
        let third = session.attach(&resumed).unwrap();
        assert_eq!(
            session.replay(third, Lane::Commands).unwrap()[0].value,
            b"next"
        );
        assert!(session
            .delivered(second, Lane::Commands, WireSeq(2))
            .is_err());
        assert_eq!(SessionControl::decode(&ack.encode()).unwrap(), ack);
        assert!(SessionControl::decode(
            br#"{"type":"ack","session_id":"same-session","lane":1,"upto":1}"#
        )
        .is_err());
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("hello_fixture.json")).unwrap();
        for key in ["hello", "hello_ok"] {
            let frame = HelloFrame::decode(&serde_json::to_vec(&fixtures[key]).unwrap()).unwrap();
            let extension = SessionHello {
                session_id: "same-session".into(),
                resume_from: [WireSeq(u64::MAX); 6],
            };
            let (_, actual) = SessionHello::decode(&extension.encode(&frame).unwrap()).unwrap();
            assert_eq!(actual, extension);
        }
    }
}
