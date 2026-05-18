use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};

use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use thiserror::Error;

use crate::{
    connector::SessionSpec,
    installations::LaunchTarget,
    terminal_emulator::{TerminalEmulator, TerminalModes, TerminalSnapshot, TerminalStyledLine},
    webrtc_terminal::WebRtcRemoteTerminal,
};

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("PTY error: {0}")]
    Pty(String),
    #[error("internal terminal lock poisoned: {0}")]
    LockPoisoned(&'static str),
    #[error("remote terminal error: {0}")]
    Remote(String),
}

type EmbeddedSnapshot = Arc<TerminalSnapshot>;

pub enum TerminalSession {
    Local(EmbeddedTerminal),
    Remote(WebRtcRemoteTerminal),
}

impl TerminalSession {
    pub fn spawn(
        spec: &SessionSpec,
        initial_rows: u16,
        initial_cols: u16,
    ) -> Result<Self, TerminalError> {
        match spec {
            SessionSpec::Local(launch) => {
                EmbeddedTerminal::spawn(launch, initial_rows, initial_cols).map(Self::Local)
            }
            SessionSpec::Remote(request) => {
                WebRtcRemoteTerminal::connect(request, initial_rows, initial_cols)
                    .map(Self::Remote)
                    .map_err(|error| TerminalError::Remote(error.to_string()))
            }
        }
    }

    pub fn write_input(&self, bytes: &[u8], interactive: bool) -> Result<(), TerminalError> {
        match self {
            Self::Local(terminal) => terminal.write_input(bytes),
            Self::Remote(terminal) => terminal
                .write_input(bytes, interactive)
                .map_err(|error| TerminalError::Remote(error.to_string())),
        }
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        match self {
            Self::Local(terminal) => terminal.snapshot(),
            Self::Remote(terminal) => terminal.snapshot(),
        }
    }

    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> Result<(), TerminalError> {
        match self {
            Self::Local(terminal) => terminal.resize(rows, cols, pixel_width, pixel_height),
            Self::Remote(terminal) => terminal
                .resize(rows, cols, pixel_width, pixel_height)
                .map_err(|error| TerminalError::Remote(error.to_string())),
        }
    }

    pub fn close(&self) {
        match self {
            Self::Local(terminal) => terminal.close(),
            Self::Remote(terminal) => terminal.close(),
        }
    }
}

#[derive(Clone)]
pub struct EmbeddedTerminal {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    emulator: Arc<Mutex<TerminalEmulator>>,
    snapshot: Arc<Mutex<EmbeddedSnapshot>>,
    snapshot_generation: Arc<AtomicU64>,
    exit_code: Arc<Mutex<Option<i32>>>,
    generation: Arc<AtomicU64>,
    killer: Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
    size: Arc<Mutex<(u16, u16, u16, u16)>>,
}

impl EmbeddedTerminal {
    pub fn spawn(
        launch: &LaunchTarget,
        initial_rows: u16,
        initial_cols: u16,
    ) -> Result<Self, TerminalError> {
        let rows = initial_rows.max(1);
        let cols = initial_cols.max(2);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| TerminalError::Pty(format!("failed to create PTY: {error}")))?;

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

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| TerminalError::Pty(format!("failed to spawn PTY process: {error}")))?;
        let killer = child.clone_killer();

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| TerminalError::Pty(format!("failed to clone PTY reader: {error}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| TerminalError::Pty(format!("failed to open PTY writer: {error}")))?;
        let master = pair.master;

        let emulator = Arc::new(Mutex::new(TerminalEmulator::with_size(rows, cols)));
        let snapshot = Arc::new(Mutex::new(empty_embedded_snapshot()));
        let snapshot_generation = Arc::new(AtomicU64::new(0));
        let exit_code = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicU64::new(1));
        let killer = Arc::new(Mutex::new(Some(killer)));
        let size = Arc::new(Mutex::new((rows, cols, 0, 0)));

        refresh_embedded_snapshot_cache(&emulator, &snapshot, None);
        snapshot_generation.store(1, Ordering::Relaxed);

        spawn_reader_thread(reader, emulator.clone(), generation.clone());
        spawn_wait_thread(
            child,
            emulator.clone(),
            exit_code.clone(),
            killer.clone(),
            generation.clone(),
        );

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            master: Arc::new(Mutex::new(master)),
            emulator,
            snapshot,
            snapshot_generation,
            exit_code,
            generation,
            killer,
            size,
        })
    }

    pub fn write_input(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        if bytes.is_empty() {
            return Ok(());
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| TerminalError::LockPoisoned("PTY writer"))?;
        writer
            .write_all(bytes)
            .map_err(|error| TerminalError::Pty(format!("failed to write to PTY: {error}")))?;
        writer
            .flush()
            .map_err(|error| TerminalError::Pty(format!("failed to flush PTY writer: {error}")))
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let current_generation = self.generation.load(Ordering::Relaxed);
        if self.snapshot_generation.load(Ordering::Relaxed) != current_generation {
            let exit_code = self
                .exit_code
                .lock()
                .ok()
                .map(|guard| *guard)
                .unwrap_or(None);
            refresh_embedded_snapshot_cache(&self.emulator, &self.snapshot, exit_code);
            self.snapshot_generation
                .store(current_generation, Ordering::Relaxed);
        }

        match self.snapshot.lock() {
            Ok(snapshot) => (*snapshot).as_ref().clone(),
            Err(poisoned) => poisoned.into_inner().as_ref().clone(),
        }
    }

    pub fn resize(
        &self,
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> Result<(), TerminalError> {
        let rows = rows.max(1);
        let cols = cols.max(2);
        let pixel_width = pixel_width.max(1);
        let pixel_height = pixel_height.max(1);

        {
            let size = self
                .size
                .lock()
                .map_err(|_| TerminalError::LockPoisoned("terminal size"))?;
            if *size == (rows, cols, pixel_width, pixel_height) {
                return Ok(());
            }
        }

        {
            let mut emulator = self
                .emulator
                .lock()
                .map_err(|_| TerminalError::LockPoisoned("emulator"))?;
            emulator.resize(rows, cols);
        }

        {
            let master = self
                .master
                .lock()
                .map_err(|_| TerminalError::LockPoisoned("PTY master"))?;
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width,
                    pixel_height,
                })
                .map_err(|error| TerminalError::Pty(format!("failed to resize PTY: {error}")))?;
        }

        {
            let mut size = self
                .size
                .lock()
                .map_err(|_| TerminalError::LockPoisoned("terminal size"))?;
            *size = (rows, cols, pixel_width, pixel_height);
        }

        self.generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn close(&self) {
        let mut killer_guard = match self.killer.lock() {
            Ok(lock) => lock,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(killer) = killer_guard.as_mut() {
            let _ = killer.kill();
        }
    }
}

impl Drop for EmbeddedTerminal {
    fn drop(&mut self) {
        if Arc::strong_count(&self.killer) != 1 {
            return;
        }

        let mut killer_guard = match self.killer.lock() {
            Ok(lock) => lock,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(killer) = killer_guard.as_mut() {
            let _ = killer.kill();
        }
    }
}

fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    emulator: Arc<Mutex<TerminalEmulator>>,
    generation: Arc<AtomicU64>,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    process_embedded_terminal_bytes(&emulator, &generation, &buffer[..read])
                }
                Err(error) => {
                    process_embedded_terminal_bytes(
                        &emulator,
                        &generation,
                        format!("\r\n[terminal reader error: {error}]\r\n").as_bytes(),
                    );
                    break;
                }
            }
        }
    });
}

fn spawn_wait_thread(
    child: Box<dyn Child + Send + Sync>,
    emulator: Arc<Mutex<TerminalEmulator>>,
    exit_code: Arc<Mutex<Option<i32>>>,
    killer: Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>,
    generation: Arc<AtomicU64>,
) {
    thread::spawn(move || {
        let mut child = child;
        let status = child.wait();

        let (final_code, exit_message) = match status {
            Ok(status) => {
                let code = i32::try_from(status.exit_code()).unwrap_or(i32::MAX);
                let message = format!("\n\n[session exited with code {code}]\n");
                (Some(code), message)
            }
            Err(error) => (
                Some(1),
                format!("\n\n[session failed to wait for process exit: {error}]\n"),
            ),
        };

        {
            let mut exit_guard = match exit_code.lock() {
                Ok(lock) => lock,
                Err(poisoned) => poisoned.into_inner(),
            };
            *exit_guard = final_code;
        }

        {
            let mut killer_guard = match killer.lock() {
                Ok(lock) => lock,
                Err(poisoned) => poisoned.into_inner(),
            };
            *killer_guard = None;
        }

        process_embedded_terminal_bytes(&emulator, &generation, exit_message.as_bytes());
    });
}

fn empty_embedded_snapshot() -> EmbeddedSnapshot {
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

fn refresh_embedded_snapshot_cache(
    emulator: &Mutex<TerminalEmulator>,
    snapshot: &Mutex<EmbeddedSnapshot>,
    exit_code: Option<i32>,
) {
    let mut next_snapshot = match emulator.lock() {
        Ok(emulator) => emulator.snapshot(),
        Err(poisoned) => poisoned.into_inner().snapshot(),
    };
    next_snapshot.exit_code = exit_code;

    match snapshot.lock() {
        Ok(mut cached) => *cached = Arc::new(next_snapshot),
        Err(poisoned) => *poisoned.into_inner() = Arc::new(next_snapshot),
    }
}

fn process_embedded_terminal_bytes(
    emulator: &Mutex<TerminalEmulator>,
    generation: &AtomicU64,
    bytes: &[u8],
) {
    match emulator.lock() {
        Ok(mut emulator) => emulator.process(bytes),
        Err(poisoned) => poisoned.into_inner().process(bytes),
    }

    generation.fetch_add(1, Ordering::Relaxed);
}
