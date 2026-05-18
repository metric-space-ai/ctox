use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use reqwest::Client;
use serde::Deserialize;
use tokio::{
    runtime::Runtime,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex as AsyncMutex, Notify,
    },
    time::{interval, sleep, timeout},
};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    data_channel::{data_channel_message::DataChannelMessage, RTCDataChannel},
    ice_transport::ice_connection_state::RTCIceConnectionState,
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_credential_type::RTCIceCredentialType,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
};

use crate::{
    command_catalog::is_allowed_ctox_args,
    connector::{RemoteSessionRequest, SessionKind},
    installations::resolve_ctox_launch_from_root,
    signaling::connect_to_signal_server,
    terminal_emulator::{TerminalEmulator, TerminalModes, TerminalSnapshot, TerminalStyledLine},
    terminal_protocol::{HubMessage, PeerSignal, RemoteSessionKind, TerminalWireMessage},
};

#[derive(Clone)]
pub struct WebRtcRemoteTerminal {
    outgoing: UnboundedSender<RemoteControlMessage>,
    emulator: Arc<Mutex<TerminalEmulator>>,
    snapshot: Arc<Mutex<Arc<TerminalSnapshot>>>,
    snapshot_generation: Arc<AtomicU64>,
    exit_code: Arc<Mutex<Option<i32>>>,
    generation: Arc<AtomicU64>,
    size: Arc<Mutex<(u16, u16, u16, u16)>>,
}

#[derive(Debug, Clone)]
enum RemoteControlMessage {
    Input(Vec<u8>),
    Resize {
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    },
    Close,
}

#[derive(Debug, Clone)]
pub struct HostBridgeConfig {
    pub root: PathBuf,
    pub signaling_urls: Vec<String>,
    pub auth_token: String,
    pub password: String,
    pub room_id: String,
    pub host_name: String,
}

struct HostedTerminal {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    exit_code: Arc<Mutex<Option<i32>>>,
    interactive: bool,
    killer: Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
}

const TURN_CONFIG_URL: &str = "https://ctox.dev/turn-ice";
const TURN_EDGE_KEY: &str = "DeepDataEngineering";
const TURN_FETCH_TIMEOUT_MS: u64 = 3500;

#[derive(Debug, Default, Clone)]
struct ViewerProgress {
    joined_room: bool,
    host_seen: bool,
    offer_sent: bool,
    answer_received: bool,
    data_channel_open: bool,
    joined_peer_count: usize,
    ice_state: Option<String>,
    peer_state: Option<String>,
    used_turn: bool,
    turn_warning: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct IceServerConfigStatus {
    used_turn: bool,
    warning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TurnIceServerRecord {
    urls: IceUrls,
    username: Option<String>,
    credential: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IceUrls {
    One(String),
    Many(Vec<String>),
}

impl WebRtcRemoteTerminal {
    pub fn connect(
        request: &RemoteSessionRequest,
        initial_rows: u16,
        initial_cols: u16,
    ) -> Result<Self> {
        let emulator = Arc::new(Mutex::new(TerminalEmulator::with_size(
            initial_rows,
            initial_cols,
        )));
        let snapshot = Arc::new(Mutex::new(empty_snapshot()));
        let snapshot_generation = Arc::new(AtomicU64::new(0));
        let exit_code = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicU64::new(1));
        let size = Arc::new(Mutex::new((initial_rows, initial_cols, 0, 0)));
        let (outgoing, control_rx) = unbounded_channel();

        refresh_snapshot(&emulator, &snapshot, None);
        snapshot_generation.store(1, Ordering::Relaxed);
        push_status(&emulator, &generation, "Connecting...\n");

        let request = request.clone();
        let emulator_clone = emulator.clone();
        let snapshot_clone = snapshot.clone();
        let snapshot_generation_clone = snapshot_generation.clone();
        let exit_code_clone = exit_code.clone();
        let generation_clone = generation.clone();
        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    push_status(
                        &emulator_clone,
                        &generation_clone,
                        &format!("[remote] failed to create runtime: {error}\n"),
                    );
                    return;
                }
            };

            let result = runtime.block_on(run_viewer_session(
                request,
                initial_rows,
                initial_cols,
                control_rx,
                emulator_clone.clone(),
                generation_clone.clone(),
                exit_code_clone.clone(),
            ));

            if let Err(error) = result {
                push_status(
                    &emulator_clone,
                    &generation_clone,
                    &format!("{}\n", humanize_remote_error(&error)),
                );
                set_exit_code(&exit_code_clone, 1);
            }

            refresh_snapshot(
                &emulator_clone,
                &snapshot_clone,
                get_exit_code(&exit_code_clone),
            );
            snapshot_generation_clone
                .store(generation_clone.load(Ordering::Relaxed), Ordering::Relaxed);
        });

        Ok(Self {
            outgoing,
            emulator,
            snapshot,
            snapshot_generation,
            exit_code,
            generation,
            size,
        })
    }

    pub fn write_input(&self, bytes: &[u8], interactive: bool) -> Result<()> {
        if !interactive || bytes.is_empty() {
            return Ok(());
        }
        self.outgoing
            .send(RemoteControlMessage::Input(bytes.to_vec()))
            .map_err(|_| anyhow!("remote control channel closed"))
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let current_generation = self.generation.load(Ordering::Relaxed);
        if self.snapshot_generation.load(Ordering::Relaxed) != current_generation {
            refresh_snapshot(
                &self.emulator,
                &self.snapshot,
                get_exit_code(&self.exit_code),
            );
            self.snapshot_generation
                .store(current_generation, Ordering::Relaxed);
        }

        match self.snapshot.lock() {
            Ok(snapshot) => snapshot.as_ref().clone(),
            Err(poisoned) => poisoned.into_inner().as_ref().clone(),
        }
    }

    pub fn resize(&self, rows: u16, cols: u16, pixel_width: u16, pixel_height: u16) -> Result<()> {
        let rows = rows.max(1);
        let cols = cols.max(2);
        {
            let size = self
                .size
                .lock()
                .map_err(|_| anyhow!("remote terminal size lock poisoned"))?;
            if *size == (rows, cols, pixel_width, pixel_height) {
                return Ok(());
            }
        }

        {
            let mut emulator = self
                .emulator
                .lock()
                .map_err(|_| anyhow!("remote emulator lock poisoned"))?;
            emulator.resize(rows, cols);
        }

        {
            let mut size = self
                .size
                .lock()
                .map_err(|_| anyhow!("remote terminal size lock poisoned"))?;
            *size = (rows, cols, pixel_width, pixel_height);
        }

        self.generation.fetch_add(1, Ordering::Relaxed);
        self.outgoing
            .send(RemoteControlMessage::Resize {
                rows,
                cols,
                pixel_width,
                pixel_height,
            })
            .map_err(|_| anyhow!("remote control channel closed"))
    }

    pub fn close(&self) {
        let _ = self.outgoing.send(RemoteControlMessage::Close);
    }
}

pub fn run_host_bridge(config: HostBridgeConfig) -> Result<()> {
    let runtime = Runtime::new()?;
    runtime.block_on(async move {
        loop {
            if let Err(error) = run_host_bridge_async(config.clone()).await {
                eprintln!("ctox-desktop-host: signaling disconnected: {error}");
            } else {
                eprintln!("ctox-desktop-host: signaling connection closed");
            }
            sleep(Duration::from_secs(3)).await;
        }
    })
}

async fn run_viewer_session(
    request: RemoteSessionRequest,
    rows: u16,
    cols: u16,
    mut control_rx: UnboundedReceiver<RemoteControlMessage>,
    emulator: Arc<Mutex<TerminalEmulator>>,
    generation: Arc<AtomicU64>,
    exit_code: Arc<Mutex<Option<i32>>>,
) -> Result<()> {
    ensure_remote_request_is_safe(&request)?;
    let progress = Arc::new(Mutex::new(ViewerProgress::default()));

    let client_id = viewer_client_id(&request.client_name);
    let remote_kind = match request.kind {
        SessionKind::Tui => RemoteSessionKind::Tui,
        SessionKind::Command => RemoteSessionKind::Command,
    };

    push_status(&emulator, &generation, "Connecting to server...\n");

    let signaling_token = effective_signaling_token(&request.auth_token, &request.password);
    let socket =
        connect_to_signal_server(&request.signaling_urls, &signaling_token, &client_id).await?;
    let (peer, ice_status) = new_peer_connection().await?;
    {
        let mut state = lock_progress(&progress);
        state.used_turn = ice_status.used_turn;
        state.turn_warning = ice_status.warning.clone();
    }
    if let Some(warning) = &ice_status.warning {
        push_status(&emulator, &generation, &format!("{warning}\n"));
    }

    let (ws_tx, mut ws_rx) = unbounded_channel::<Message>();
    let (mut ws_write, mut ws_read) = socket.split();
    tokio::spawn(async move {
        while let Some(message) = ws_rx.recv().await {
            if ws_write.send(message).await.is_err() {
                break;
            }
        }
    });

    let own_peer_id = Arc::new(AsyncMutex::new(None::<String>));
    let target_peer_id = Arc::new(AsyncMutex::new(None::<String>));
    let data_channel_open = Arc::new(AtomicBool::new(false));

    let ws_tx_ice = ws_tx.clone();
    let own_peer_id_for_ice = own_peer_id.clone();
    let target_peer_id_for_ice = target_peer_id.clone();
    peer.on_ice_candidate(Box::new(move |candidate| {
        let ws_tx = ws_tx_ice.clone();
        let own_peer_id = own_peer_id_for_ice.clone();
        let target_peer_id = target_peer_id_for_ice.clone();
        Box::pin(async move {
            let Some(candidate) = candidate else {
                return;
            };
            let Ok(init) = candidate.to_json() else {
                return;
            };
            let own_peer_id = own_peer_id.lock().await.clone();
            let target_peer_id = target_peer_id.lock().await.clone();
            let (Some(sender_peer_id), Some(receiver_peer_id)) = (own_peer_id, target_peer_id)
            else {
                return;
            };
            let _ = queue_signal(
                &ws_tx,
                HubMessage::Signal {
                    sender_peer_id,
                    receiver_peer_id,
                    signal: PeerSignal::Candidate {
                        candidate: init.candidate,
                        sdp_mid: init.sdp_mid,
                        sdp_mline_index: init.sdp_mline_index,
                    },
                },
            );
        })
    }));

    let progress_for_ice = progress.clone();
    let emulator_for_ice = emulator.clone();
    let generation_for_ice = generation.clone();
    peer.on_ice_connection_state_change(Box::new(move |state: RTCIceConnectionState| {
        let progress = progress_for_ice.clone();
        let emulator = emulator_for_ice.clone();
        let generation = generation_for_ice.clone();
        Box::pin(async move {
            {
                let mut current = lock_progress(&progress);
                current.ice_state = Some(state.to_string());
            }
            match state {
                RTCIceConnectionState::Checking => {
                    push_status(&emulator, &generation, "Checking network path...\n");
                }
                RTCIceConnectionState::Connected | RTCIceConnectionState::Completed => {
                    push_status(&emulator, &generation, "Network path ready.\n");
                }
                RTCIceConnectionState::Disconnected => {
                    push_status(&emulator, &generation, "WebRTC connection interrupted.\n");
                }
                RTCIceConnectionState::Failed => {
                    push_status(&emulator, &generation, "ICE/TURN connection failed.\n");
                }
                _ => {}
            }
        })
    }));

    let progress_for_peer = progress.clone();
    let emulator_for_peer = emulator.clone();
    let generation_for_peer = generation.clone();
    peer.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
        let progress = progress_for_peer.clone();
        let emulator = emulator_for_peer.clone();
        let generation = generation_for_peer.clone();
        Box::pin(async move {
            {
                let mut current = lock_progress(&progress);
                current.peer_state = Some(state.to_string());
            }
            match state {
                RTCPeerConnectionState::Connecting => {
                    push_status(&emulator, &generation, "Building WebRTC connection...\n");
                }
                RTCPeerConnectionState::Connected => {
                    push_status(&emulator, &generation, "WebRTC connected.\n");
                }
                RTCPeerConnectionState::Failed => {
                    push_status(&emulator, &generation, "WebRTC setup failed.\n");
                }
                RTCPeerConnectionState::Disconnected => {
                    push_status(&emulator, &generation, "WebRTC disconnected.\n");
                }
                _ => {}
            }
        })
    }));

    let data_channel = peer.create_data_channel("ctox-terminal", None).await?;
    let ready = Arc::new(Notify::new());
    let started = Arc::new(AtomicBool::new(false));
    let data_channel_for_open = data_channel.clone();
    let ready_open = ready.clone();
    let started_open = started.clone();
    let data_channel_open_flag = data_channel_open.clone();
    let progress_for_open = progress.clone();
    let emulator_for_open = emulator.clone();
    let generation_for_open = generation.clone();
    let start_message = TerminalWireMessage::Start {
        session_kind: remote_kind,
        args: request.command_args.clone(),
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    data_channel.on_open(Box::new(move || {
        let data_channel = data_channel_for_open.clone();
        let ready = ready_open.clone();
        let started = started_open.clone();
        let start_message = start_message.clone();
        Box::pin(async move {
            if started.swap(true, Ordering::Relaxed) {
                return;
            }
            data_channel_open_flag.store(true, Ordering::Relaxed);
            {
                let mut current = lock_progress(&progress_for_open);
                current.data_channel_open = true;
            }
            push_status(
                &emulator_for_open,
                &generation_for_open,
                "Starting session...\n",
            );
            let _ = data_channel
                .send_text(serde_json::to_string(&start_message).unwrap_or_default())
                .await;
            ready.notify_waiters();
        })
    }));

    let emulator_messages = emulator.clone();
    let generation_messages = generation.clone();
    let exit_code_messages = exit_code.clone();
    data_channel.on_message(Box::new(move |message: DataChannelMessage| {
        let emulator = emulator_messages.clone();
        let generation = generation_messages.clone();
        let exit_code = exit_code_messages.clone();
        Box::pin(async move {
            let payload = message.data.to_vec();
            let Ok(frame) = serde_json::from_slice::<TerminalWireMessage>(&payload) else {
                push_status(&emulator, &generation, "[remote] received invalid frame\n");
                return;
            };
            match frame {
                TerminalWireMessage::Output { data_base64 } => {
                    if let Ok(bytes) = BASE64.decode(data_base64.as_bytes()) {
                        process_bytes(&emulator, &generation, &bytes);
                    }
                }
                TerminalWireMessage::Exit { code } => {
                    set_exit_code(&exit_code, code);
                    push_status(
                        &emulator,
                        &generation,
                        &format!("\n[remote session exited with code {code}]\n"),
                    );
                }
                TerminalWireMessage::Status { message } => {
                    push_status(&emulator, &generation, &format!("{message}\n"));
                }
                TerminalWireMessage::Start { .. }
                | TerminalWireMessage::Input { .. }
                | TerminalWireMessage::Resize { .. }
                | TerminalWireMessage::Close => {}
            }
        })
    }));

    let command_channel = data_channel.clone();
    tokio::spawn(async move {
        ready.notified().await;
        while let Some(control) = control_rx.recv().await {
            let frame = match control {
                RemoteControlMessage::Input(data) => TerminalWireMessage::Input {
                    data_base64: BASE64.encode(data),
                },
                RemoteControlMessage::Resize {
                    rows,
                    cols,
                    pixel_width,
                    pixel_height,
                } => TerminalWireMessage::Resize {
                    rows,
                    cols,
                    pixel_width,
                    pixel_height,
                },
                RemoteControlMessage::Close => TerminalWireMessage::Close,
            };
            let Ok(payload) = serde_json::to_string(&frame) else {
                continue;
            };
            if command_channel.send_text(payload).await.is_err() {
                break;
            }
        }
    });

    let ping_tx = ws_tx.clone();
    tokio::spawn(async move {
        let mut keepalive = interval(Duration::from_secs(30));
        loop {
            keepalive.tick().await;
            if queue_signal(
                &ping_tx,
                HubMessage::Ping {
                    t: Some(timestamp_ms()),
                },
            )
            .is_err()
            {
                break;
            }
        }
    });

    let mut joined_room = false;
    let mut answer_received = false;
    let mut pending_answer_candidates: Vec<RTCIceCandidateInit> = Vec::new();
    let answer_deadline = Instant::now() + Duration::from_secs(18);
    while !answer_received {
        if Instant::now() > answer_deadline {
            anyhow::bail!("{}", describe_offer_timeout(&lock_progress(&progress)));
        }
        let next = timeout(Duration::from_millis(450), ws_read.next()).await;
        let Some(message) = (match next {
            Ok(Some(message)) => Some(message?),
            Ok(None) => None,
            Err(_) => {
                continue;
            }
        }) else {
            anyhow::bail!("signaling server closed the connection");
        };
        match message {
            Message::Text(text) => {
                let signal: HubMessage = match serde_json::from_str(&text) {
                    Ok(signal) => signal,
                    Err(_) => continue,
                };
                match signal {
                    HubMessage::Init { your_peer_id } => {
                        *own_peer_id.lock().await = Some(your_peer_id);
                        if !joined_room {
                            queue_signal(
                                &ws_tx,
                                HubMessage::Join {
                                    room: request.room_id.clone(),
                                },
                            )?;
                            joined_room = true;
                            {
                                let mut state = lock_progress(&progress);
                                state.joined_room = true;
                            }
                            push_status(&emulator, &generation, "Waiting for host...\n");
                        }
                    }
                    HubMessage::Joined { other_peer_ids } => {
                        let own = own_peer_id.lock().await.clone();
                        let peer_count = other_peer_ids
                            .iter()
                            .filter(|peer_id| Some(peer_id.as_str()) != own.as_deref())
                            .count();
                        {
                            let mut state = lock_progress(&progress);
                            state.joined_peer_count = peer_count;
                            state.host_seen = peer_count > 0;
                        }
                        if peer_count == 0 {
                            push_status(
                                &emulator,
                                &generation,
                                "No host online in this room yet.\n",
                            );
                        }
                        let current_target = target_peer_id.lock().await.clone();
                        if current_target.is_none() {
                            let next_target = other_peer_ids
                                .iter()
                                .rev()
                                .find(|peer_id| Some(peer_id.as_str()) != own.as_deref())
                                .cloned();
                            if let Some(next_target) = next_target {
                                *target_peer_id.lock().await = Some(next_target.clone());
                                {
                                    let mut state = lock_progress(&progress);
                                    state.offer_sent = true;
                                }
                                push_status(
                                    &emulator,
                                    &generation,
                                    "Host found. Sending connection offer...\n",
                                );
                                let offer = peer.create_offer(None).await?;
                                let mut offer_gathering_complete =
                                    peer.gathering_complete_promise().await;
                                peer.set_local_description(offer).await?;
                                let _ = timeout(
                                    Duration::from_secs(8),
                                    offer_gathering_complete.recv(),
                                )
                                .await
                                .map_err(|_| {
                                    anyhow!("viewer timed out while gathering offer candidates")
                                })?;
                                let local = peer
                                    .local_description()
                                    .await
                                    .context("missing local viewer description")?;
                                let sender_peer_id = own_peer_id
                                    .lock()
                                    .await
                                    .clone()
                                    .context("viewer peer id missing")?;
                                queue_signal(
                                    &ws_tx,
                                    HubMessage::Signal {
                                        sender_peer_id,
                                        receiver_peer_id: next_target,
                                        signal: PeerSignal::Offer { sdp: local.sdp },
                                    },
                                )?;
                            }
                        }
                    }
                    HubMessage::Signal {
                        sender_peer_id,
                        receiver_peer_id,
                        signal,
                    } => {
                        let own = own_peer_id.lock().await.clone();
                        if Some(receiver_peer_id.as_str()) != own.as_deref() {
                            continue;
                        }

                        let current_target = target_peer_id.lock().await.clone();
                        if current_target.is_none() {
                            *target_peer_id.lock().await = Some(sender_peer_id.clone());
                        }
                        if target_peer_id.lock().await.as_deref() != Some(sender_peer_id.as_str()) {
                            continue;
                        }

                        match signal {
                            PeerSignal::Answer { sdp } => {
                                peer.set_remote_description(RTCSessionDescription::answer(sdp)?)
                                    .await?;
                                for candidate in pending_answer_candidates.drain(..) {
                                    peer.add_ice_candidate(candidate).await?;
                                }
                                {
                                    let mut state = lock_progress(&progress);
                                    state.answer_received = true;
                                }
                                push_status(
                                    &emulator,
                                    &generation,
                                    "Host answered. Opening session...\n",
                                );
                                answer_received = true;
                            }
                            PeerSignal::Candidate {
                                candidate,
                                sdp_mid,
                                sdp_mline_index,
                            } => {
                                let candidate = RTCIceCandidateInit {
                                    candidate,
                                    sdp_mid,
                                    sdp_mline_index,
                                    username_fragment: None,
                                };
                                if answer_received {
                                    peer.add_ice_candidate(candidate).await?;
                                } else {
                                    pending_answer_candidates.push(candidate);
                                }
                            }
                            PeerSignal::Offer { .. } => {}
                        }
                    }
                    HubMessage::Ping { t } => {
                        let _ = queue_signal(&ws_tx, HubMessage::Pong { t });
                    }
                    HubMessage::Pong { .. } => {}
                    HubMessage::Join { .. } => {}
                }
            }
            Message::Binary(_) => {}
            Message::Ping(payload) => {
                let _ = ws_tx.send(Message::Pong(payload));
            }
            Message::Pong(_) => {}
            Message::Close(_) => anyhow::bail!("signaling server closed the connection"),
            Message::Frame(_) => {}
        }
    }

    let channel_deadline = Instant::now() + Duration::from_secs(16);
    while !data_channel_open.load(Ordering::Relaxed) {
        if Instant::now() > channel_deadline {
            anyhow::bail!("{}", describe_channel_timeout(&lock_progress(&progress)));
        }
        let next = timeout(Duration::from_millis(450), ws_read.next()).await;
        let Some(message) = (match next {
            Ok(Some(message)) => Some(message?),
            Ok(None) => None,
            Err(_) => None,
        }) else {
            continue;
        };
        match message {
            Message::Text(text) => {
                let signal: HubMessage = match serde_json::from_str(&text) {
                    Ok(signal) => signal,
                    Err(_) => continue,
                };
                match signal {
                    HubMessage::Signal {
                        sender_peer_id,
                        receiver_peer_id,
                        signal,
                    } => {
                        let own = own_peer_id.lock().await.clone();
                        if Some(receiver_peer_id.as_str()) != own.as_deref() {
                            continue;
                        }
                        match signal {
                            PeerSignal::Candidate {
                                candidate,
                                sdp_mid,
                                sdp_mline_index,
                            } => {
                                if target_peer_id.lock().await.as_deref()
                                    != Some(sender_peer_id.as_str())
                                {
                                    continue;
                                }
                                peer.add_ice_candidate(RTCIceCandidateInit {
                                    candidate,
                                    sdp_mid,
                                    sdp_mline_index,
                                    username_fragment: None,
                                })
                                .await?;
                            }
                            PeerSignal::Answer { sdp } => {
                                if target_peer_id.lock().await.as_deref()
                                    != Some(sender_peer_id.as_str())
                                {
                                    continue;
                                }
                                peer.set_remote_description(RTCSessionDescription::answer(sdp)?)
                                    .await?;
                            }
                            PeerSignal::Offer { .. } => {}
                        }
                    }
                    HubMessage::Ping { t } => {
                        let _ = queue_signal(&ws_tx, HubMessage::Pong { t });
                    }
                    HubMessage::Joined { other_peer_ids } => {
                        let own = own_peer_id.lock().await.clone();
                        let peer_count = other_peer_ids
                            .iter()
                            .filter(|peer_id| Some(peer_id.as_str()) != own.as_deref())
                            .count();
                        let mut state = lock_progress(&progress);
                        state.joined_peer_count = peer_count;
                        state.host_seen = peer_count > 0;
                    }
                    HubMessage::Pong { .. } | HubMessage::Init { .. } | HubMessage::Join { .. } => {
                    }
                }
            }
            Message::Ping(payload) => {
                let _ = ws_tx.send(Message::Pong(payload));
            }
            Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
            Message::Close(_) => anyhow::bail!("signaling server closed the connection"),
        }
    }

    push_status(&emulator, &generation, "Connected.\n");

    while let Some(message) = ws_read.next().await {
        let message = message?;
        match message {
            Message::Text(text) => {
                let signal: HubMessage = match serde_json::from_str(&text) {
                    Ok(signal) => signal,
                    Err(_) => continue,
                };
                match signal {
                    HubMessage::Joined { other_peer_ids } => {
                        let own = own_peer_id.lock().await.clone();
                        let current_target = target_peer_id.lock().await.clone();
                        if current_target.is_none() {
                            if let Some(next_target) = other_peer_ids
                                .into_iter()
                                .find(|peer_id| Some(peer_id.as_str()) != own.as_deref())
                            {
                                *target_peer_id.lock().await = Some(next_target);
                            }
                        }
                    }
                    HubMessage::Signal {
                        sender_peer_id,
                        receiver_peer_id,
                        signal,
                    } => {
                        let own = own_peer_id.lock().await.clone();
                        if Some(receiver_peer_id.as_str()) != own.as_deref() {
                            continue;
                        }
                        match signal {
                            PeerSignal::Candidate {
                                candidate,
                                sdp_mid,
                                sdp_mline_index,
                            } => {
                                if target_peer_id.lock().await.as_deref()
                                    != Some(sender_peer_id.as_str())
                                {
                                    continue;
                                }
                                peer.add_ice_candidate(RTCIceCandidateInit {
                                    candidate,
                                    sdp_mid,
                                    sdp_mline_index,
                                    username_fragment: None,
                                })
                                .await?;
                            }
                            PeerSignal::Answer { sdp } => {
                                if target_peer_id.lock().await.as_deref()
                                    != Some(sender_peer_id.as_str())
                                {
                                    continue;
                                }
                                peer.set_remote_description(RTCSessionDescription::answer(sdp)?)
                                    .await?;
                            }
                            PeerSignal::Offer { .. } => {}
                        }
                    }
                    HubMessage::Ping { t } => {
                        let _ = queue_signal(&ws_tx, HubMessage::Pong { t });
                    }
                    HubMessage::Pong { .. } | HubMessage::Init { .. } | HubMessage::Join { .. } => {
                    }
                }
            }
            Message::Binary(_) => {}
            Message::Ping(payload) => {
                let _ = ws_tx.send(Message::Pong(payload));
            }
            Message::Pong(_) => {}
            Message::Close(_) => {
                push_status(&emulator, &generation, "Connection closed.\n");
                break;
            }
            Message::Frame(_) => {}
        }
    }

    let _ = peer.close().await;
    Ok(())
}

async fn run_host_bridge_async(config: HostBridgeConfig) -> Result<()> {
    let host_client_id = if config.host_name.trim().is_empty() {
        format!("ctox-host-{}", Uuid::new_v4().simple())
    } else {
        format!("{}-{}", config.host_name.trim(), Uuid::new_v4().simple())
    };

    let signaling_token = effective_signaling_token(&config.auth_token, &config.password);
    let socket =
        connect_to_signal_server(&config.signaling_urls, &signaling_token, &host_client_id).await?;
    let (mut ws_write, mut ws_read) = socket.split();
    let (ws_tx, mut ws_rx) = unbounded_channel::<Message>();
    tokio::spawn(async move {
        while let Some(message) = ws_rx.recv().await {
            if ws_write.send(message).await.is_err() {
                break;
            }
        }
    });

    let ping_tx = ws_tx.clone();
    tokio::spawn(async move {
        let mut keepalive = interval(Duration::from_secs(30));
        loop {
            keepalive.tick().await;
            if queue_signal(
                &ping_tx,
                HubMessage::Ping {
                    t: Some(timestamp_ms()),
                },
            )
            .is_err()
            {
                break;
            }
        }
    });

    let peers = Arc::new(AsyncMutex::new(std::collections::HashMap::<
        String,
        Arc<RTCPeerConnection>,
    >::new()));
    let own_peer_id = Arc::new(AsyncMutex::new(None::<String>));
    let mut joined_room = false;

    while let Some(message) = ws_read.next().await {
        let message = message?;
        match message {
            Message::Text(text) => {
                let signal: HubMessage = match serde_json::from_str(&text) {
                    Ok(signal) => signal,
                    Err(_) => continue,
                };

                match signal {
                    HubMessage::Init { your_peer_id } => {
                        *own_peer_id.lock().await = Some(your_peer_id.clone());
                        if !joined_room {
                            eprintln!("ctox-desktop-host: init peer_id={your_peer_id}");
                            queue_signal(
                                &ws_tx,
                                HubMessage::Join {
                                    room: config.room_id.clone(),
                                },
                            )?;
                            joined_room = true;
                            eprintln!("ctox-desktop-host: joined room={}", config.room_id);
                        }
                    }
                    HubMessage::Signal {
                        sender_peer_id,
                        receiver_peer_id,
                        signal,
                    } => {
                        let own = own_peer_id.lock().await.clone();
                        if Some(receiver_peer_id.as_str()) != own.as_deref() {
                            continue;
                        }
                        match signal {
                            PeerSignal::Offer { sdp } => {
                                eprintln!(
                                    "ctox-desktop-host: received offer from peer={} sdp_len={}",
                                    sender_peer_id,
                                    sdp.len()
                                );
                                let offer_result: anyhow::Result<()> = async {
                                    let existing_peer =
                                        { peers.lock().await.get(&sender_peer_id).cloned() };
                                    let peer = if let Some(existing) = existing_peer {
                                        existing
                                    } else {
                                        eprintln!("ctox-desktop-host: creating peer connection");
                                        let (peer, _ice_status) = new_peer_connection().await?;
                                        eprintln!("ctox-desktop-host: peer connection created");
                                        eprintln!("ctox-desktop-host: configuring host peer");
                                        configure_host_peer(
                                            peer.clone(),
                                            &config,
                                            &ws_tx,
                                            own_peer_id.clone(),
                                            sender_peer_id.clone(),
                                        )
                                        .await?;
                                        eprintln!("ctox-desktop-host: host peer configured");
                                        peers
                                            .lock()
                                            .await
                                            .insert(sender_peer_id.clone(), peer.clone());
                                        peer
                                    };

                                    eprintln!("ctox-desktop-host: applying remote offer");
                                    let offer_desc = RTCSessionDescription::offer(sdp)?;
                                    let peer_for_remote = peer.clone();
                                    let apply_remote = tokio::spawn(async move {
                                        peer_for_remote.set_remote_description(offer_desc).await
                                    });
                                    timeout(Duration::from_secs(8), apply_remote)
                                        .await
                                        .map_err(|_| {
                                            anyhow!("host timed out while applying remote offer")
                                        })?
                                        .context("host remote-offer task failed")?
                                        .context("host failed to apply remote offer")?;
                                    eprintln!("ctox-desktop-host: remote offer applied");

                                    eprintln!("ctox-desktop-host: creating answer");
                                    let peer_for_answer = peer.clone();
                                    let create_answer = tokio::spawn(async move {
                                        peer_for_answer.create_answer(None).await
                                    });
                                    let answer = timeout(Duration::from_secs(8), create_answer)
                                        .await
                                        .map_err(|_| {
                                            anyhow!("host timed out while creating answer")
                                        })?
                                        .context("host answer task failed")?
                                        .context("host failed to create answer")?;
                                    let mut answer_gathering_complete =
                                        peer.gathering_complete_promise().await;
                                    let peer_for_local = peer.clone();
                                    let set_local = tokio::spawn(async move {
                                        peer_for_local.set_local_description(answer).await
                                    });
                                    timeout(Duration::from_secs(8), set_local)
                                        .await
                                        .map_err(|_| {
                                            anyhow!("host timed out while setting local answer")
                                        })?
                                        .context("host local-answer task failed")?
                                        .context("host failed to set local answer")?;
                                    eprintln!("ctox-desktop-host: local answer set");
                                    let _ = timeout(
                                        Duration::from_secs(8),
                                        answer_gathering_complete.recv(),
                                    )
                                    .await
                                    .map_err(|_| {
                                        anyhow!("host timed out while gathering answer candidates")
                                    })?;
                                    let answer_sdp = peer
                                        .local_description()
                                        .await
                                        .context("host missing local answer description")?
                                        .sdp;

                                    let sender = own_peer_id
                                        .lock()
                                        .await
                                        .clone()
                                        .context("host peer id missing")?;
                                    eprintln!(
                                        "ctox-desktop-host: sending answer to peer={} sdp_len={}",
                                        sender_peer_id,
                                        answer_sdp.len()
                                    );
                                    queue_signal(
                                        &ws_tx,
                                        HubMessage::Signal {
                                            sender_peer_id: sender,
                                            receiver_peer_id: sender_peer_id,
                                            signal: PeerSignal::Answer { sdp: answer_sdp },
                                        },
                                    )?;
                                    Ok(())
                                }
                                .await;

                                if let Err(error) = offer_result {
                                    eprintln!(
                                        "ctox-desktop-host: offer handling failed: {error:#}"
                                    );
                                }
                            }
                            PeerSignal::Candidate {
                                candidate,
                                sdp_mid,
                                sdp_mline_index,
                            } => {
                                if let Some(peer) = peers.lock().await.get(&sender_peer_id).cloned()
                                {
                                    if let Err(error) = peer
                                        .add_ice_candidate(RTCIceCandidateInit {
                                            candidate,
                                            sdp_mid,
                                            sdp_mline_index,
                                            username_fragment: None,
                                        })
                                        .await
                                    {
                                        eprintln!(
                                            "ctox-desktop-host: failed to add ice candidate from peer={}: {error:#}",
                                            sender_peer_id
                                        );
                                    }
                                }
                            }
                            PeerSignal::Answer { .. } => {}
                        }
                    }
                    HubMessage::Ping { t } => {
                        let _ = queue_signal(&ws_tx, HubMessage::Pong { t });
                    }
                    HubMessage::Pong { .. }
                    | HubMessage::Join { .. }
                    | HubMessage::Joined { .. } => {}
                }
            }
            Message::Ping(payload) => {
                let _ = ws_tx.send(Message::Pong(payload));
            }
            Message::Pong(_) => {}
            Message::Close(_) => break,
            Message::Binary(_) | Message::Frame(_) => {}
        }
    }

    Ok(())
}

async fn configure_host_peer(
    peer: Arc<RTCPeerConnection>,
    config: &HostBridgeConfig,
    ws_tx: &UnboundedSender<Message>,
    own_peer_id: Arc<AsyncMutex<Option<String>>>,
    remote_peer_id: String,
) -> Result<()> {
    let ws_tx = ws_tx.clone();
    peer.on_ice_candidate(Box::new(move |candidate| {
        let ws_tx = ws_tx.clone();
        let own_peer_id = own_peer_id.clone();
        let remote_peer_id = remote_peer_id.clone();
        Box::pin(async move {
            let Some(candidate) = candidate else {
                return;
            };
            let Ok(init) = candidate.to_json() else {
                return;
            };
            let Some(sender_peer_id) = own_peer_id.lock().await.clone() else {
                return;
            };
            let _ = queue_signal(
                &ws_tx,
                HubMessage::Signal {
                    sender_peer_id,
                    receiver_peer_id: remote_peer_id,
                    signal: PeerSignal::Candidate {
                        candidate: init.candidate,
                        sdp_mid: init.sdp_mid,
                        sdp_mline_index: init.sdp_mline_index,
                    },
                },
            );
        })
    }));

    let root = config.root.clone();
    peer.on_data_channel(Box::new(move |channel| {
        let root = root.clone();
        Box::pin(async move {
            let _ = configure_host_data_channel(root, channel).await;
        })
    }));

    Ok(())
}

async fn configure_host_data_channel(root: PathBuf, channel: Arc<RTCDataChannel>) -> Result<()> {
    let process = Arc::new(AsyncMutex::new(None::<HostedTerminal>));
    let runtime_handle = tokio::runtime::Handle::current();

    let channel_open = channel.clone();
    channel.on_open(Box::new(move || {
        let channel = channel_open.clone();
        Box::pin(async move {
            let _ = channel
                .send_text(
                    serde_json::to_string(&TerminalWireMessage::Status {
                        message: "restricted CTOX host connected".to_owned(),
                    })
                    .unwrap_or_default(),
                )
                .await;
        })
    }));

    let process_messages = process.clone();
    let channel_for_messages = channel.clone();
    channel.on_message(Box::new(move |message: DataChannelMessage| {
        let root = root.clone();
        let channel = channel_for_messages.clone();
        let process = process_messages.clone();
        let runtime_handle = runtime_handle.clone();
        Box::pin(async move {
            let payload = message.data.to_vec();
            let Ok(frame) = serde_json::from_slice::<TerminalWireMessage>(&payload) else {
                let _ = send_host_status(&channel, "invalid control frame").await;
                return;
            };

            match frame {
                TerminalWireMessage::Start {
                    session_kind,
                    args,
                    rows,
                    cols,
                    pixel_width,
                    pixel_height,
                } => {
                    if !is_allowed_ctox_args(&args) {
                        let _ =
                            send_host_status(&channel, "command rejected by CTOX allowlist").await;
                        let _ = send_host_exit(&channel, 126).await;
                        return;
                    }

                    let launch = match resolve_ctox_launch_from_root(
                        &root,
                        &None,
                        &Default::default(),
                        &args.iter().map(|value| value.as_str()).collect::<Vec<_>>(),
                    ) {
                        Ok(launch) => launch,
                        Err(error) => {
                            let _ = send_host_status(
                                &channel,
                                &format!("failed to resolve CTOX launch target: {error}"),
                            )
                            .await;
                            let _ = send_host_exit(&channel, 127).await;
                            return;
                        }
                    };

                    match spawn_hosted_terminal(
                        &launch,
                        rows,
                        cols,
                        pixel_width,
                        pixel_height,
                        channel.clone(),
                        runtime_handle.clone(),
                        matches!(session_kind, RemoteSessionKind::Tui),
                    ) {
                        Ok(hosted) => {
                            *process.lock().await = Some(hosted);
                        }
                        Err(error) => {
                            let _ = send_host_status(
                                &channel,
                                &format!("failed to launch CTOX session: {error}"),
                            )
                            .await;
                            let _ = send_host_exit(&channel, 127).await;
                        }
                    }
                }
                TerminalWireMessage::Input { data_base64 } => {
                    let Some(process) = process.lock().await.as_ref().map(clone_hosted_terminal)
                    else {
                        return;
                    };
                    if !process.interactive {
                        return;
                    }
                    let Ok(bytes) = BASE64.decode(data_base64.as_bytes()) else {
                        return;
                    };
                    let mut writer = match process.writer.lock() {
                        Ok(lock) => lock,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
                TerminalWireMessage::Resize {
                    rows,
                    cols,
                    pixel_width,
                    pixel_height,
                } => {
                    let Some(process) = process.lock().await.as_ref().map(clone_hosted_terminal)
                    else {
                        return;
                    };
                    let master = match process.master.lock() {
                        Ok(lock) => lock,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let _ = master.resize(PtySize {
                        rows: rows.max(1),
                        cols: cols.max(2),
                        pixel_width,
                        pixel_height,
                    });
                }
                TerminalWireMessage::Close => {
                    if let Some(process) = process.lock().await.take() {
                        let mut killer = match process.killer.lock() {
                            Ok(lock) => lock,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        if let Some(killer) = killer.as_mut() {
                            let _ = killer.kill();
                        }
                    }
                }
                TerminalWireMessage::Output { .. }
                | TerminalWireMessage::Exit { .. }
                | TerminalWireMessage::Status { .. } => {}
            }
        })
    }));

    Ok(())
}

fn spawn_hosted_terminal(
    launch: &crate::installations::LaunchTarget,
    rows: u16,
    cols: u16,
    pixel_width: u16,
    pixel_height: u16,
    channel: Arc<RTCDataChannel>,
    runtime_handle: tokio::runtime::Handle,
    interactive: bool,
) -> Result<HostedTerminal> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: rows.max(1),
        cols: cols.max(2),
        pixel_width,
        pixel_height,
    })?;

    let mut command = CommandBuilder::new(&launch.program);
    for arg in &launch.args {
        command.arg(arg);
    }
    command.cwd(launch.cwd.as_os_str());
    for (key, value) in &launch.env {
        command.env(key, value);
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");

    let child = pair.slave.spawn_command(command)?;
    let killer = child.clone_killer();
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let master = pair.master;

    let hosted = HostedTerminal {
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(master)),
        exit_code: Arc::new(Mutex::new(None)),
        interactive,
        killer: Arc::new(Mutex::new(Some(killer))),
    };

    spawn_host_reader(reader, channel.clone(), runtime_handle.clone());
    spawn_host_waiter(child, hosted.clone(), channel, runtime_handle);
    Ok(hosted)
}

fn spawn_host_reader(
    mut reader: Box<dyn Read + Send>,
    channel: Arc<RTCDataChannel>,
    runtime_handle: tokio::runtime::Handle,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let payload = BASE64.encode(&buffer[..read]);
                    let channel = channel.clone();
                    runtime_handle.spawn(async move {
                        let _ = channel
                            .send_text(
                                serde_json::to_string(&TerminalWireMessage::Output {
                                    data_base64: payload,
                                })
                                .unwrap_or_default(),
                            )
                            .await;
                    });
                }
                Err(error) => {
                    let message = format!("host reader error: {error}");
                    let channel = channel.clone();
                    runtime_handle.spawn(async move {
                        let _ = send_host_status(&channel, &message).await;
                    });
                    break;
                }
            }
        }
    });
}

fn spawn_host_waiter(
    child: Box<dyn Child + Send + Sync>,
    hosted: HostedTerminal,
    channel: Arc<RTCDataChannel>,
    runtime_handle: tokio::runtime::Handle,
) {
    thread::spawn(move || {
        let mut child = child;
        let code = match child.wait() {
            Ok(status) => i32::try_from(status.exit_code()).unwrap_or(i32::MAX),
            Err(_) => 1,
        };
        set_exit_code(&hosted.exit_code, code);
        let mut killer = match hosted.killer.lock() {
            Ok(lock) => lock,
            Err(poisoned) => poisoned.into_inner(),
        };
        *killer = None;
        runtime_handle.spawn(async move {
            let _ = send_host_exit(&channel, code).await;
        });
    });
}

fn clone_hosted_terminal(hosted: &HostedTerminal) -> HostedTerminal {
    HostedTerminal {
        writer: hosted.writer.clone(),
        master: hosted.master.clone(),
        exit_code: hosted.exit_code.clone(),
        interactive: hosted.interactive,
        killer: hosted.killer.clone(),
    }
}

async fn send_host_status(channel: &RTCDataChannel, message: &str) -> Result<()> {
    channel
        .send_text(serde_json::to_string(&TerminalWireMessage::Status {
            message: message.to_owned(),
        })?)
        .await?;
    Ok(())
}

async fn send_host_exit(channel: &RTCDataChannel, code: i32) -> Result<()> {
    channel
        .send_text(serde_json::to_string(&TerminalWireMessage::Exit { code })?)
        .await?;
    Ok(())
}

async fn new_peer_connection() -> Result<(Arc<RTCPeerConnection>, IceServerConfigStatus)> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();
    let (ice_servers, ice_status) = load_ice_servers().await;

    Ok((
        Arc::new(
            api.new_peer_connection(RTCConfiguration {
                ice_servers,
                ..Default::default()
            })
            .await?,
        ),
        ice_status,
    ))
}

fn ensure_remote_request_is_safe(request: &RemoteSessionRequest) -> Result<()> {
    if request.signaling_urls.is_empty() {
        anyhow::bail!("remote installation has no signaling URL configured");
    }
    if request.password.trim().is_empty() {
        anyhow::bail!("remote installation has no session password configured");
    }
    if !is_allowed_ctox_args(&request.command_args) {
        anyhow::bail!("requested command is not in the CTOX allowlist");
    }
    Ok(())
}

fn effective_signaling_token(auth_token: &str, password: &str) -> String {
    let explicit = auth_token.trim();
    if !explicit.is_empty() {
        return explicit.to_owned();
    }
    password.trim().to_owned()
}

fn viewer_client_id(client_name: &str) -> String {
    if client_name.trim().is_empty() {
        format!("ctox-viewer-{}", Uuid::new_v4().simple())
    } else {
        format!("{}-{}", client_name.trim(), Uuid::new_v4().simple())
    }
}

fn refresh_snapshot(
    emulator: &Mutex<TerminalEmulator>,
    snapshot: &Mutex<Arc<TerminalSnapshot>>,
    exit_code: Option<i32>,
) {
    let mut next_snapshot = match emulator.lock() {
        Ok(emulator) => emulator.snapshot(),
        Err(poisoned) => poisoned.into_inner().snapshot(),
    };
    next_snapshot.exit_code = exit_code;
    match snapshot.lock() {
        Ok(mut lock) => *lock = Arc::new(next_snapshot),
        Err(poisoned) => *poisoned.into_inner() = Arc::new(next_snapshot),
    }
}

fn empty_snapshot() -> Arc<TerminalSnapshot> {
    Arc::new(TerminalSnapshot {
        output: String::new(),
        styled_lines: vec![TerminalStyledLine {
            cells: Vec::new(),
            runs: Vec::new(),
        }],
        cursor: None,
        modes: TerminalModes::default(),
        exit_code: None,
    })
}

fn process_bytes(emulator: &Mutex<TerminalEmulator>, generation: &AtomicU64, bytes: &[u8]) {
    match emulator.lock() {
        Ok(mut emulator) => emulator.process(bytes),
        Err(poisoned) => poisoned.into_inner().process(bytes),
    }
    generation.fetch_add(1, Ordering::Relaxed);
}

fn push_status(emulator: &Mutex<TerminalEmulator>, generation: &AtomicU64, message: &str) {
    process_bytes(emulator, generation, message.as_bytes());
}

fn set_exit_code(exit_code: &Mutex<Option<i32>>, code: i32) {
    match exit_code.lock() {
        Ok(mut lock) => *lock = Some(code),
        Err(poisoned) => *poisoned.into_inner() = Some(code),
    }
}

fn get_exit_code(exit_code: &Mutex<Option<i32>>) -> Option<i32> {
    match exit_code.lock() {
        Ok(lock) => *lock,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

fn queue_signal(sender: &UnboundedSender<Message>, signal: HubMessage) -> Result<()> {
    let payload = serde_json::to_string(&signal)?;
    sender
        .send(Message::Text(payload))
        .map_err(|_| anyhow!("signaling channel closed"))
}

fn lock_progress(
    progress: &Arc<Mutex<ViewerProgress>>,
) -> std::sync::MutexGuard<'_, ViewerProgress> {
    match progress.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn describe_offer_timeout(progress: &ViewerProgress) -> &'static str {
    if !progress.joined_room {
        return "no response from host or signaling server";
    }
    if !progress.host_seen {
        return "no remote host joined the room";
    }
    if progress.peer_state.as_deref() == Some("failed")
        || progress.ice_state.as_deref() == Some("failed")
    {
        return "webrtc transport failed during setup";
    }
    "host found but did not answer the webrtc offer"
}

fn describe_channel_timeout(progress: &ViewerProgress) -> &'static str {
    if progress.peer_state.as_deref() == Some("failed")
        || progress.ice_state.as_deref() == Some("failed")
    {
        return "webrtc transport failed during setup";
    }
    "webrtc data channel did not open"
}

async fn load_ice_servers() -> (Vec<RTCIceServer>, IceServerConfigStatus) {
    eprintln!("ctox-desktop-host: loading ICE server config");
    let fallback = vec![RTCIceServer {
        urls: vec![
            "stun:stun.l.google.com:19302".to_owned(),
            "stun:global.stun.twilio.com:3478".to_owned(),
        ],
        ..Default::default()
    }];

    let client = match Client::builder()
        .timeout(Duration::from_millis(TURN_FETCH_TIMEOUT_MS))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return (
                fallback,
                IceServerConfigStatus {
                    used_turn: false,
                    warning: Some(format!("TURN nicht verfuegbar, nutze nur STUN ({error}).")),
                },
            );
        }
    };

    let response = match client
        .get(TURN_CONFIG_URL)
        .header("X-Edge-Key", TURN_EDGE_KEY)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return (
                fallback,
                IceServerConfigStatus {
                    used_turn: false,
                    warning: Some(format!(
                        "TURN nicht erreichbar, versuche Direktverbindung ({error})."
                    )),
                },
            );
        }
    };

    if !response.status().is_success() {
        return (
            fallback,
            IceServerConfigStatus {
                used_turn: false,
                warning: Some(format!(
                    "TURN endpoint antwortet mit HTTP {}.",
                    response.status()
                )),
            },
        );
    }

    let payload: serde_json::Value = match response.json().await {
        Ok(value) => value,
        Err(error) => {
            return (
                fallback,
                IceServerConfigStatus {
                    used_turn: false,
                    warning: Some(format!("TURN-Antwort ungueltig ({error}).")),
                },
            );
        }
    };

    let raw_servers = payload.get("iceServers").cloned().unwrap_or(payload);

    let records: Vec<TurnIceServerRecord> = if raw_servers.is_array() {
        serde_json::from_value(raw_servers).unwrap_or_default()
    } else {
        serde_json::from_value(raw_servers)
            .map(|single| vec![single])
            .unwrap_or_default()
    };

    let ice_servers: Vec<RTCIceServer> = records
        .into_iter()
        .map(|record| {
            let urls = match record.urls {
                IceUrls::One(url) => vec![url],
                IceUrls::Many(urls) => urls,
            };
            let has_turn = urls
                .iter()
                .any(|url| url.starts_with("turn:") || url.starts_with("turns:"));
            RTCIceServer {
                urls,
                username: record.username.unwrap_or_default(),
                credential: record.credential.unwrap_or_default(),
                credential_type: if has_turn {
                    RTCIceCredentialType::Password
                } else {
                    RTCIceCredentialType::Unspecified
                },
            }
        })
        .filter(|server| !server.urls.is_empty())
        .collect();

    if ice_servers.is_empty() {
        return (
            fallback,
            IceServerConfigStatus {
                used_turn: false,
                warning: Some(
                    "TURN hat keine gueltigen ICE-Server geliefert. Nutze nur STUN.".to_owned(),
                ),
            },
        );
    }

    eprintln!(
        "ctox-desktop-host: ICE config ready with {} servers",
        ice_servers.len()
    );
    (
        ice_servers,
        IceServerConfigStatus {
            used_turn: true,
            warning: None,
        },
    )
}

fn humanize_remote_error(error: &anyhow::Error) -> String {
    humanize_remote_text(&error.to_string())
}

fn humanize_remote_text(message: &str) -> String {
    let trimmed = message.trim();
    match trimmed {
        "remote installation has no signaling URL configured" => {
            "Kein Server eingetragen.".to_owned()
        }
        "remote installation has no session password configured" => {
            "Please enter a password.".to_owned()
        }
        "requested command is not in the CTOX allowlist" => {
            "This command is not approved for remote use.".to_owned()
        }
        "no remote host joined the room" => {
            "In diesem Room ist aktuell kein CTOX-Host online.".to_owned()
        }
        "host found but did not answer the webrtc offer" => {
            "Der Host ist online, antwortet aber nicht auf das WebRTC-Angebot.".to_owned()
        }
        "webrtc data channel did not open" => {
            "Die WebRTC-Verbindung kam zustande, aber die CTOX-Session wurde nicht geoeffnet.".to_owned()
        }
        "webrtc transport failed during setup" => {
            "Die WebRTC-Verbindung ist am Netzwerkpfad gescheitert. TURN oder NAT blockiert die Verbindung.".to_owned()
        }
        "turn configuration unavailable" => {
            "TURN konnte nicht geladen werden. Die Verbindung versucht nur STUN und kann an NAT scheitern.".to_owned()
        }
        "no response from host or signaling server" => {
            "Keine Antwort vom Signaling-Server oder Host.".to_owned()
        }
        "signaling server closed the connection" => {
            "Der Server hat die Verbindung beendet. Bitte Server und Token prüfen.".to_owned()
        }
        "viewer session not authorized" => {
            "Host rejected the connection. Check room, password, and token.".to_owned()
        }
        _ if trimmed.contains("Connection reset without closing handshake") => {
            "Der Server hat die Verbindung zurückgesetzt. Bitte Server und Token prüfen.".to_owned()
        }
        _ if trimmed.contains("401") || trimmed.contains("403") => {
            "Anmeldung am Server fehlgeschlagen. Bitte Token prüfen.".to_owned()
        }
        "invalid turn server credentials" => {
            "TURN wurde geladen, aber die Zugangsdaten wurden vom Client noch falsch gesetzt.".to_owned()
        }
        _ if trimmed.contains("TURN endpoint") => {
            "TURN konnte nicht geladen werden. Bitte TURN-Server pruefen.".to_owned()
        }
        _ => format!("Verbindung fehlgeschlagen: {trimmed}"),
    }
}

impl Clone for HostedTerminal {
    fn clone(&self) -> Self {
        clone_hosted_terminal(self)
    }
}
