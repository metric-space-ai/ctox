use {
    crate::terminal_emulator::{
        alacritty_support::{
            self, collect_styled_lines, new_state, snapshot_cursor, snapshot_modes, snapshot_output,
        },
        TerminalModes, TerminalProcessReport, TerminalSnapshot,
    },
    std::cell::RefCell,
};

#[derive(Clone)]
struct CachedOutputSnapshot {
    generation: u64,
    output: String,
}

#[derive(Clone)]
struct CachedStyledSnapshot {
    generation: u64,
    styled_lines: Vec<crate::terminal_emulator::TerminalStyledLine>,
    cursor: Option<crate::terminal_emulator::TerminalCursor>,
    modes: TerminalModes,
}

pub struct TerminalEmulator {
    state: alacritty_support::AlacrittyState,
    generation: u64,
    output_snapshot_cache: RefCell<Option<CachedOutputSnapshot>>,
    styled_snapshot_cache: RefCell<Option<CachedStyledSnapshot>>,
}

impl TerminalEmulator {
    pub fn with_size(rows: u16, cols: u16) -> Self {
        Self {
            state: new_state(rows, cols),
            generation: 0,
            output_snapshot_cache: RefCell::new(None),
            styled_snapshot_cache: RefCell::new(None),
        }
    }

    pub fn process_and_report(&mut self, bytes: &[u8]) -> TerminalProcessReport {
        if bytes.is_empty() {
            return TerminalProcessReport::default();
        }

        self.state.processor.advance(&mut self.state.term, bytes);
        let report = self.state.event_listener.take_process_report();
        self.generation = self.generation.saturating_add(1);
        self.output_snapshot_cache.get_mut().take();
        self.styled_snapshot_cache.get_mut().take();
        report
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let dimensions = alacritty_support::TerminalDimensions {
            rows: usize::from(rows),
            cols: usize::from(cols),
        };
        self.state.term.resize(dimensions);
        self.generation = self.generation.saturating_add(1);
        self.output_snapshot_cache.get_mut().take();
        self.styled_snapshot_cache.get_mut().take();
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let output_snapshot = self.output_snapshot();
        let styled_snapshot = self.styled_snapshot();
        TerminalSnapshot {
            output: output_snapshot.output,
            styled_lines: styled_snapshot.styled_lines,
            cursor: styled_snapshot.cursor,
            modes: styled_snapshot.modes,
            exit_code: None,
        }
    }

    fn output_snapshot(&self) -> CachedOutputSnapshot {
        if let Some(snapshot) = self.cached_output_snapshot() {
            return snapshot;
        }

        let snapshot = CachedOutputSnapshot {
            generation: self.generation,
            output: snapshot_output(&self.state.term),
        };
        *self.output_snapshot_cache.borrow_mut() = Some(snapshot.clone());
        snapshot
    }

    fn styled_snapshot(&self) -> CachedStyledSnapshot {
        if let Some(snapshot) = self.cached_styled_snapshot() {
            return snapshot;
        }

        let snapshot = CachedStyledSnapshot {
            generation: self.generation,
            styled_lines: collect_styled_lines(&self.state.term),
            cursor: snapshot_cursor(&self.state.term),
            modes: snapshot_modes(&self.state.term),
        };
        *self.styled_snapshot_cache.borrow_mut() = Some(snapshot.clone());
        snapshot
    }

    fn cached_output_snapshot(&self) -> Option<CachedOutputSnapshot> {
        self.output_snapshot_cache
            .borrow()
            .as_ref()
            .filter(|snapshot| snapshot.generation == self.generation)
            .cloned()
    }

    fn cached_styled_snapshot(&self) -> Option<CachedStyledSnapshot> {
        self.styled_snapshot_cache
            .borrow()
            .as_ref()
            .filter(|snapshot| snapshot.generation == self.generation)
            .cloned()
    }
}
