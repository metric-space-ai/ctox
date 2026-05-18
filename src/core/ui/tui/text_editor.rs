//! Minimal reusable text-edit widget for the CTOX TUI.
//!
//! This is an independent Rust implementation — no code is derived from any
//! other editor. The public API is intentionally small: a `TextEditor` owns
//! a text buffer, tracks cursor/scroll, consumes `KeyEvent`s, and renders
//! just its text (no title, no footer, no status chrome) into whatever
//! `Rect` the caller hands it.
//!
//! Hosts mount this widget wherever they like — full-screen, in a popup,
//! inside a Settings panel — and layer their own framing (titles, borders,
//! status lines, key hints) around it. The widget deliberately does not
//! impose any layout decisions.
//!
//! Features:
//! - Arrows / Home / End / PgUp / PgDn / Ctrl+Home / Ctrl+End movement
//! - Typed characters, Enter (newline), Backspace, Delete, Tab (4 spaces)
//! - Ctrl+X: save to backing file (if any) and exit with `ExitReason::Saved`
//! - Esc: cancel without saving and exit with `ExitReason::Cancelled`
//!
//! Explicitly out of scope: cut/paste, search, replace, undo/redo, go-to-line,
//! file browser, syntax highlighting, multi-buffer, soft-wrap. If the caller
//! needs one of these, it can layer it on top by reading/mutating the buffer
//! through the public API.

use anyhow::Context;
use anyhow::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Tab insertion width in spaces. Modern default; tabs themselves are never
/// stored, only runs of spaces.
const TAB_WIDTH: usize = 4;

/// Fallback Page{Up,Down} step when the editor has not yet been rendered and
/// therefore does not know its viewport height.
const FALLBACK_PAGE_STEP: usize = 16;

/// Result of feeding a key event to the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    /// The editor consumed the key; keep it open.
    Continue,
    /// The editor wants to be torn down. Host collects `text()` and/or
    /// verifies on-disk state based on the reason.
    Exit(ExitReason),
}

/// Why the editor is asking to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// Ctrl+X — the buffer has been flushed: written to disk if there was a
    /// backing path, otherwise the in-memory text is final and the host
    /// should read it via `text()`.
    Saved,
    /// Esc — user abandoned the edit; any on-disk file is untouched since
    /// the last save, and the host should discard the in-memory text.
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TextEditor {
    /// Optional backing file. Ctrl+X writes here when present. When absent,
    /// Ctrl+X simply exits with `Saved` and the host owns persistence.
    path: Option<PathBuf>,
    /// Text lines. There is always at least one (possibly empty) line so
    /// indexing with `cursor_row` is always safe.
    lines: Vec<String>,
    /// Cursor in logical (row, char-col). Columns count Unicode scalar
    /// values, not bytes, so non-ASCII text behaves correctly.
    cursor_row: usize,
    cursor_col: usize,
    /// The widest column the cursor has reached; preserved across vertical
    /// movement so crossing a short line and returning to a long one
    /// restores horizontal position.
    target_col: usize,
    /// First visible logical row (vertical scroll). Updated during `render`,
    /// so it lives behind `Cell` — that keeps `render` callable as `&self`
    /// from the host's `draw(&App)` without requiring a mutable borrow.
    scroll_row: Cell<usize>,
    /// First visible character column (horizontal scroll).
    scroll_col: Cell<usize>,
    /// Dirty flag for host-side decisions (e.g., whether to prompt before
    /// closing a popup).
    modified: bool,
    /// Last rendered viewport height in rows. `0` before first render.
    viewport_rows: Cell<u16>,
}

impl TextEditor {
    /// Open a file from disk. A missing path starts as an empty buffer; the
    /// first save then creates the file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let contents = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to read file for editing: {}", path.display())
                });
            }
        };
        Ok(Self::from_text(Some(path), &contents))
    }

    /// In-memory buffer not backed by a file. The host retrieves the final
    /// text via `text()` after the editor exits with `Saved`.
    pub fn scratch(initial: &str) -> Self {
        Self::from_text(None, initial)
    }

    fn from_text(path: Option<PathBuf>, initial: &str) -> Self {
        let lines = if initial.is_empty() {
            vec![String::new()]
        } else {
            // Split on '\n' (not `.lines()`) so a trailing blank line is
            // preserved — authors often want that empty line at EOF.
            initial.split('\n').map(String::from).collect()
        };
        Self {
            path,
            lines,
            cursor_row: 0,
            cursor_col: 0,
            target_col: 0,
            scroll_row: Cell::new(0),
            scroll_col: Cell::new(0),
            modified: false,
            viewport_rows: Cell::new(0),
        }
    }

    /// Full text, joined with '\n'.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Whether the buffer has diverged from its last-saved state.
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Optional backing file path.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Cursor as (row, col) — both 0-based in the logical buffer. Handy if
    /// the host wants to render a "Line X, Col Y" status line outside the
    /// editor widget itself.
    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    /// Number of logical lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    // ---------- key handling ----------

    pub fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        // Single key path: save-and-exit.
        if ctrl && !alt && matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X')) {
            match self.save_if_possible() {
                Ok(()) => return EditorAction::Exit(ExitReason::Saved),
                Err(_io_err) => {
                    // On a write error, stay open; the host has no way to
                    // surface the problem from inside the widget, so we
                    // simply refuse to exit. In practice, the host should
                    // also render an outer status line that shows errors.
                    return EditorAction::Continue;
                }
            }
        }

        // Cancel without saving.
        if matches!(key.code, KeyCode::Esc) {
            return EditorAction::Exit(ExitReason::Cancelled);
        }

        match (key.code, ctrl, alt) {
            (KeyCode::Left, false, false) => self.move_left(),
            (KeyCode::Right, false, false) => self.move_right(),
            (KeyCode::Up, false, false) => self.move_up(),
            (KeyCode::Down, false, false) => self.move_down(),
            (KeyCode::Home, false, false) => {
                self.cursor_col = 0;
                self.target_col = 0;
            }
            (KeyCode::End, false, false) => {
                self.cursor_col = self.current_line_len();
                self.target_col = self.cursor_col;
            }
            (KeyCode::PageUp, false, false) => self.page_up(),
            (KeyCode::PageDown, false, false) => self.page_down(),
            (KeyCode::Home, true, false) => {
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.target_col = 0;
            }
            (KeyCode::End, true, false) => {
                self.cursor_row = self.lines.len().saturating_sub(1);
                self.cursor_col = self.current_line_len();
                self.target_col = self.cursor_col;
            }
            (KeyCode::Enter, false, false) => self.insert_newline(),
            (KeyCode::Backspace, _, false) => self.delete_before_cursor(),
            (KeyCode::Delete, _, false) => self.delete_at_cursor(),
            (KeyCode::Tab, false, false) => {
                for _ in 0..TAB_WIDTH {
                    self.insert_char(' ');
                }
            }
            (KeyCode::Char(ch), false, false) => self.insert_char(ch),
            _ => {} // ignored
        }

        EditorAction::Continue
    }

    // ---------- editing primitives ----------

    fn insert_char(&mut self, ch: char) {
        let line = &mut self.lines[self.cursor_row];
        let byte = char_byte_index(line, self.cursor_col);
        line.insert(byte, ch);
        self.cursor_col += 1;
        self.target_col = self.cursor_col;
        self.modified = true;
    }

    fn insert_newline(&mut self) {
        let line = self.lines[self.cursor_row].clone();
        let byte = char_byte_index(&line, self.cursor_col);
        let (left, right) = line.split_at(byte);
        self.lines[self.cursor_row] = left.to_string();
        self.lines.insert(self.cursor_row + 1, right.to_string());
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.target_col = 0;
        self.modified = true;
    }

    fn delete_before_cursor(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let prev = char_byte_index(line, self.cursor_col - 1);
            let cur = char_byte_index(line, self.cursor_col);
            line.replace_range(prev..cur, "");
            self.cursor_col -= 1;
            self.target_col = self.cursor_col;
            self.modified = true;
        } else if self.cursor_row > 0 {
            // Join with previous line.
            let current = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_len = self.current_line_len();
            self.lines[self.cursor_row].push_str(&current);
            self.cursor_col = prev_len;
            self.target_col = self.cursor_col;
            self.modified = true;
        }
    }

    fn delete_at_cursor(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col < len {
            let line = &mut self.lines[self.cursor_row];
            let cur = char_byte_index(line, self.cursor_col);
            let next = char_byte_index(line, self.cursor_col + 1);
            line.replace_range(cur..next, "");
            self.modified = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next_line);
            self.modified = true;
        }
    }

    // ---------- movement ----------

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
        }
        self.target_col = self.cursor_col;
    }

    fn move_right(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
        self.target_col = self.cursor_col;
    }

    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = self.current_line_len();
            self.cursor_col = self.target_col.min(len);
        }
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = self.current_line_len();
            self.cursor_col = self.target_col.min(len);
        }
    }

    fn page_up(&mut self) {
        let step = self.page_step();
        self.cursor_row = self.cursor_row.saturating_sub(step);
        let len = self.current_line_len();
        self.cursor_col = self.target_col.min(len);
    }

    fn page_down(&mut self) {
        let step = self.page_step();
        let last = self.lines.len().saturating_sub(1);
        self.cursor_row = (self.cursor_row + step).min(last);
        let len = self.current_line_len();
        self.cursor_col = self.target_col.min(len);
    }

    fn page_step(&self) -> usize {
        let raw = self.viewport_rows.get() as usize;
        if raw == 0 {
            FALLBACK_PAGE_STEP
        } else {
            raw.saturating_sub(1).max(1)
        }
    }

    // ---------- save ----------

    /// Write the buffer to disk if a path is known. Returns Ok(()) either
    /// way (no path = nothing to write; host takes ownership of the text).
    fn save_if_possible(&mut self) -> std::result::Result<(), String> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        write_file(&path, &self.text())?;
        self.modified = false;
        Ok(())
    }

    // ---------- helpers ----------

    fn current_line_len(&self) -> usize {
        self.lines
            .get(self.cursor_row)
            .map(|line| line.chars().count())
            .unwrap_or(0)
    }

    fn ensure_cursor_visible(&self, rows: u16, cols: u16) {
        let viewport_rows = rows.max(1) as usize;
        let viewport_cols = cols.max(1) as usize;

        let mut scroll_row = self.scroll_row.get();
        if self.cursor_row < scroll_row {
            scroll_row = self.cursor_row;
        } else if self.cursor_row >= scroll_row + viewport_rows {
            scroll_row = self.cursor_row + 1 - viewport_rows;
        }
        self.scroll_row.set(scroll_row);

        let mut scroll_col = self.scroll_col.get();
        if self.cursor_col < scroll_col {
            scroll_col = self.cursor_col;
        } else if self.cursor_col >= scroll_col + viewport_cols {
            scroll_col = self.cursor_col + 1 - viewport_cols;
        }
        self.scroll_col.set(scroll_col);
    }

    // ---------- render ----------

    /// Draw text + cursor into `area`. Callers add their own surrounding
    /// chrome (title, borders, status line) as the context requires.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let rows = area.height as usize;
        let cols = area.width as usize;
        if rows == 0 || cols == 0 {
            return;
        }

        self.viewport_rows.set(area.height);
        self.ensure_cursor_visible(area.height, area.width);

        let scroll_row = self.scroll_row.get();
        let scroll_col = self.scroll_col.get();

        let mut rendered: Vec<Line> = Vec::with_capacity(rows);
        for visual_row in 0..rows {
            let logical_row = scroll_row + visual_row;
            if logical_row >= self.lines.len() {
                rendered.push(Line::from(Span::styled(
                    "~",
                    Style::default().fg(Color::DarkGray),
                )));
                continue;
            }
            let line = &self.lines[logical_row];
            let slice = slice_line_by_chars(line, scroll_col, cols);

            if logical_row == self.cursor_row {
                let cursor_visual_col = self.cursor_col.saturating_sub(scroll_col);
                let (before, at, after) = split_for_cursor(&slice, cursor_visual_col);
                let mut spans: Vec<Span<'static>> = Vec::with_capacity(3);
                if !before.is_empty() {
                    spans.push(Span::raw(before));
                }
                spans.push(Span::styled(
                    at,
                    Style::default()
                        .bg(Color::White)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ));
                if !after.is_empty() {
                    spans.push(Span::raw(after));
                }
                rendered.push(Line::from(spans));
            } else {
                rendered.push(Line::from(Span::raw(slice)));
            }
        }

        let paragraph = Paragraph::new(rendered).block(Block::default());
        frame.render_widget(paragraph, area);
    }
}

// ---------- free helpers ----------

fn write_file(path: &Path, text: &str) -> std::result::Result<(), String> {
    // POSIX tools expect a trailing newline. Only add one when the buffer
    // is non-empty; empty files stay empty so diff-tools don't complain.
    let mut payload = text.to_string();
    if !payload.is_empty() && !payload.ends_with('\n') {
        payload.push('\n');
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
    }
    fs::write(path, payload.as_bytes()).map_err(|err| err.to_string())?;
    Ok(())
}

fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(s.len())
}

fn slice_line_by_chars(line: &str, skip: usize, take: usize) -> String {
    line.chars().skip(skip).take(take).collect()
}

fn split_for_cursor(line: &str, cursor: usize) -> (String, String, String) {
    let chars: Vec<char> = line.chars().collect();
    if cursor >= chars.len() {
        let before: String = chars.iter().collect();
        // A single-space placeholder gives the caller a rectangular cursor
        // cell at EOL instead of an invisible one.
        return (before, " ".to_string(), String::new());
    }
    let before: String = chars[..cursor].iter().collect();
    let at: String = chars[cursor].to_string();
    let after: String = chars[cursor + 1..].iter().collect();
    (before, at, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn scratch_starts_with_one_empty_line() {
        let editor = TextEditor::scratch("");
        assert_eq!(editor.lines, vec![String::new()]);
        assert_eq!(editor.cursor_position(), (0, 0));
        assert!(!editor.is_modified());
    }

    #[test]
    fn typing_chars_marks_modified() {
        let mut editor = TextEditor::scratch("");
        editor.handle_key(plain(KeyCode::Char('h')));
        editor.handle_key(plain(KeyCode::Char('i')));
        assert_eq!(editor.text(), "hi");
        assert!(editor.is_modified());
    }

    #[test]
    fn enter_splits_line() {
        let mut editor = TextEditor::scratch("hello");
        editor.handle_key(plain(KeyCode::Right));
        editor.handle_key(plain(KeyCode::Right));
        editor.handle_key(plain(KeyCode::Enter));
        assert_eq!(editor.text(), "he\nllo");
        assert_eq!(editor.cursor_position(), (1, 0));
    }

    #[test]
    fn backspace_joins_lines_at_col_zero() {
        let mut editor = TextEditor::scratch("ab\ncd");
        editor.handle_key(plain(KeyCode::Down));
        editor.handle_key(plain(KeyCode::Home));
        editor.handle_key(plain(KeyCode::Backspace));
        assert_eq!(editor.text(), "abcd");
        assert_eq!(editor.cursor_position(), (0, 2));
    }

    #[test]
    fn delete_joins_with_next_line_at_eol() {
        let mut editor = TextEditor::scratch("ab\ncd");
        editor.handle_key(plain(KeyCode::End));
        editor.handle_key(plain(KeyCode::Delete));
        assert_eq!(editor.text(), "abcd");
    }

    #[test]
    fn target_col_preserved_across_short_lines() {
        let mut editor = TextEditor::scratch("hello world\nhi\nlong line here");
        for _ in 0..8 {
            editor.handle_key(plain(KeyCode::Right));
        }
        assert_eq!(editor.cursor_position(), (0, 8));
        editor.handle_key(plain(KeyCode::Down));
        assert_eq!(editor.cursor_position(), (1, 2)); // clamped to "hi".len()
        editor.handle_key(plain(KeyCode::Down));
        assert_eq!(editor.cursor_position(), (2, 8)); // restored from target
    }

    #[test]
    fn ctrl_x_saves_and_exits() {
        let mut editor = TextEditor::scratch("hi");
        let action = editor.handle_key(ctrl(KeyCode::Char('x')));
        assert_eq!(action, EditorAction::Exit(ExitReason::Saved));
    }

    #[test]
    fn esc_cancels_and_exits() {
        let mut editor = TextEditor::scratch("hi");
        editor.handle_key(plain(KeyCode::Char('x')));
        assert!(editor.is_modified());
        let action = editor.handle_key(plain(KeyCode::Esc));
        assert_eq!(action, EditorAction::Exit(ExitReason::Cancelled));
    }

    #[test]
    fn tab_inserts_four_spaces() {
        let mut editor = TextEditor::scratch("");
        editor.handle_key(plain(KeyCode::Tab));
        assert_eq!(editor.text(), "    ");
        assert_eq!(editor.cursor_position(), (0, 4));
    }

    #[test]
    fn unicode_round_trip() {
        let mut editor = TextEditor::scratch("äöü");
        editor.handle_key(plain(KeyCode::End));
        editor.handle_key(plain(KeyCode::Char('ß')));
        assert_eq!(editor.text(), "äöüß");
        assert_eq!(editor.cursor_position(), (0, 4));
    }
}
