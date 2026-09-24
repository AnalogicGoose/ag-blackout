use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::backend::Backend;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::Terminal;

use crate::shell::{scenario, Shell};

use super::render;

const PAGE_SCROLL: u16 = 10;
const WHEEL_SCROLL: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    /// Typing edits the input line; Up/Down cycle command history.
    Normal,
    /// Entered via Ctrl+Shift+Space. Typing does nothing; Up/Down/PageUp/
    /// PageDown/Home/End navigate scrollback instead, with a status line
    /// shown so it's obvious the mode is active.
    Scroll,
}

/// State for the terminal-emulator-style UI: the game session plus
/// everything needed to render/edit a live input line on top of scrollback.
/// Deliberately holds no `Terminal`/backend — that's `terminal.rs`'s job —
/// so this stays testable without a real TTY (see the tests below).
pub struct App {
    shell: Shell,
    pub lines: Vec<Line<'static>>,
    pub input: String,
    pub cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    pub scroll_offset: u16,
    pub mode: AppMode,
    viewport_height: u16,
    should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            shell: scenario::tutorial(),
            lines: Vec::new(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            scroll_offset: 0,
            mode: AppMode::Normal,
            viewport_height: 0,
            should_quit: false,
        };
        app.push_plain("AG Linux 1.0.0 (Blackbird) — AnalogicGoose");
        app.push_plain("1 contract available: \"Retrieve the Q3 report\" — corp-fs01 — $3000");
        app.push_plain("Credentials: connect corp-fs01 guest guest");
        app.push_plain("");
        app
    }

    fn push_plain(&mut self, text: impl Into<String>) {
        self.lines.push(Line::from(text.into()));
    }

    pub fn prompt_string(&self) -> String {
        let user = self.shell.active_device().users.whoami(self.shell.context.uid).unwrap_or("?");
        format!("{user}@{}:{}$ ", self.shell.active_hostname(), self.shell.context.cwd)
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn char_len(&self) -> usize {
        self.input.chars().count()
    }

    fn byte_index_of(&self, char_index: usize) -> usize {
        self.input.char_indices().map(|(i, _)| i).nth(char_index).unwrap_or(self.input.len())
    }

    fn insert_char(&mut self, c: char) {
        let idx = self.byte_index_of(self.cursor);
        self.input.insert(idx, c);
        self.cursor += 1;
    }

    fn delete_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = self.byte_index_of(self.cursor);
        let start = self.byte_index_of(self.cursor - 1);
        self.input.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn delete_at_cursor(&mut self) {
        if self.cursor >= self.char_len() {
            return;
        }
        let start = self.byte_index_of(self.cursor);
        let end = self.byte_index_of(self.cursor + 1);
        self.input.replace_range(start..end, "");
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_len());
    }

    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_index {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_index = Some(next);
        self.input = self.history[next].clone();
        self.cursor = self.char_len();
    }

    fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(i) if i + 1 < self.history.len() => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
                self.cursor = self.char_len();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
        }
    }

    fn submit(&mut self) {
        let input = self.input.clone();
        let prompt = self.prompt_string();
        self.lines.push(Line::from(vec![
            Span::styled(prompt, Style::default().fg(Color::Green)),
            Span::raw(input.clone()),
        ]));

        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
        self.scroll_offset = 0;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.history.last().map(String::as_str) != Some(trimmed) {
            self.history.push(trimmed.to_string());
        }
        if trimmed == "exit" || trimmed == "quit" {
            self.should_quit = true;
            return;
        }

        let balance_before = self.shell.economy.balance();
        let result = self.shell.execute_line(trimmed);
        for line in result.stdout.lines() {
            self.push_plain(line.to_string());
        }
        for line in result.stderr.lines() {
            self.lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Red))));
        }
        if self.shell.economy.balance() != balance_before {
            self.push_plain(format!("Contract complete — balance: ${}", self.shell.economy.balance()));
        }
    }

    fn toggle_scroll_mode(&mut self) {
        self.mode = match self.mode {
            AppMode::Normal => AppMode::Scroll,
            AppMode::Scroll => {
                self.scroll_offset = 0;
                AppMode::Normal
            }
        };
    }

    /// The scrollback content is `lines.len() + 1` rows (the `+ 1` is the
    /// live input line, which `render.rs` appends the same way). Real
    /// terminal height, cached from the last frame via `set_viewport_height`.
    fn max_scroll(&self) -> u16 {
        let total = (self.lines.len() + 1) as u16;
        total.saturating_sub(self.viewport_height)
    }

    pub fn set_viewport_height(&mut self, height: u16) {
        self.viewport_height = height;
    }

    /// Clamped to `max_scroll` here, not just at render time — letting
    /// `scroll_offset` grow past the real maximum meant scrolling back down
    /// had to "pay off" the invisible excess before the screen visibly moved.
    fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines).min(self.max_scroll());
    }

    fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Mouse wheel always scrolls, in either mode — it's a separate input
    /// channel from the keyboard, so there's nothing for it to conflict with.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_up(WHEEL_SCROLL),
            MouseEventKind::ScrollDown => self.scroll_down(WHEEL_SCROLL),
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Ctrl+Space toggles Scroll Mode — deliberately not requiring the
        // SHIFT bit too. Many terminals (including some IDE-embedded ones)
        // send Ctrl+Space and Ctrl+Shift+Space as the exact same sequence at
        // the protocol level, so requiring SHIFT made the toggle silently
        // unreachable on those terminals even though the combo is labeled
        // "Ctrl+Shift+Space" in the UI.
        if key.code == KeyCode::Char(' ') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.toggle_scroll_mode();
            return;
        }

        if self.mode == AppMode::Scroll {
            match key.code {
                KeyCode::Up => self.scroll_up(1),
                KeyCode::Down => self.scroll_down(1),
                KeyCode::PageUp => self.scroll_up(PAGE_SCROLL),
                KeyCode::PageDown => self.scroll_down(PAGE_SCROLL),
                KeyCode::Home => self.scroll_offset = self.max_scroll(),
                KeyCode::End => self.scroll_offset = 0,
                KeyCode::Esc => self.toggle_scroll_mode(),
                _ => {}
            }
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    self.input.clear();
                    self.cursor = 0;
                    self.history_index = None;
                }
                KeyCode::Char('d') if self.input.is_empty() => self.should_quit = true,
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Enter => self.submit(),
            KeyCode::Char(c) => self.insert_char(c),
            KeyCode::Backspace => self.delete_before_cursor(),
            KeyCode::Delete => self.delete_at_cursor(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.char_len(),
            KeyCode::Up => self.history_up(),
            KeyCode::Down => self.history_down(),
            KeyCode::PageUp => self.scroll_up(PAGE_SCROLL),
            KeyCode::PageDown => self.scroll_down(PAGE_SCROLL),
            _ => {}
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// The event loop: redraw, wait up to 250ms for an event, handle it, repeat
/// until the player types `exit`/`quit` or hits Ctrl+D on an empty line.
pub fn run<B: Backend>(terminal: &mut Terminal<B>) -> std::io::Result<()> {
    let mut app = App::new();
    loop {
        let full_height = terminal.size()?.height;
        // Must match render.rs's own split: the status line steals one row
        // from the scrollback while Scroll Mode is active.
        let viewport_height = if app.mode == AppMode::Scroll { full_height.saturating_sub(1) } else { full_height };
        app.set_viewport_height(viewport_height);

        terminal.draw(|frame| render::draw(frame, &app))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                _ => {}
            }
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn ctrl_shift_space() -> KeyEvent {
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL | KeyModifiers::SHIFT)
    }

    fn wheel(kind: MouseEventKind) -> MouseEvent {
        MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE }
    }

    #[test]
    fn typing_appends_to_input_and_advances_cursor() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.input, "ps");
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn backspace_removes_the_character_before_the_cursor() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.input, "p");
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn left_right_move_the_cursor_within_bounds() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.cursor, 0);
        app.handle_key(key(KeyCode::Left)); // already at 0, stays clamped
        assert_eq!(app.cursor, 0);
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right)); // already at end, stays clamped
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn ctrl_c_clears_the_input_line() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(ctrl(KeyCode::Char('c')));
        assert_eq!(app.input, "");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn ctrl_d_on_empty_input_quits() {
        let mut app = App::new();
        assert!(!app.should_quit());
        app.handle_key(ctrl(KeyCode::Char('d')));
        assert!(app.should_quit());
    }

    #[test]
    fn ctrl_d_with_pending_input_does_not_quit() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(ctrl(KeyCode::Char('d')));
        assert!(!app.should_quit());
    }

    #[test]
    fn typing_exit_and_pressing_enter_quits() {
        let mut app = App::new();
        for c in "exit".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(app.should_quit());
    }

    #[test]
    fn submit_runs_the_command_and_clears_input() {
        let mut app = App::new();
        for c in "pwd".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        let lines_before = app.lines.len();
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.input, "");
        assert_eq!(app.cursor, 0);
        assert!(app.lines.len() > lines_before);
    }

    #[test]
    fn history_up_then_down_cycles_back_to_empty_input() {
        let mut app = App::new();
        for c in "whoami".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.input, "whoami");

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.input, "");
    }

    /// Drives the full Slice 1 loop purely through key events, proving the
    /// UI layer correctly reaches the underlying Shell/contract flow.
    #[test]
    fn completing_the_tutorial_contract_through_key_events() {
        let mut app = App::new();
        for c in "connect corp-fs01 guest guest".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        for c in "cat /home/guest/report.pdf".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.shell.economy.balance(), 3000);
        assert!(app.lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("Contract complete"))));
    }

    #[test]
    fn ctrl_shift_space_toggles_scroll_mode() {
        let mut app = App::new();
        assert_eq!(app.mode, AppMode::Normal);
        app.handle_key(ctrl_shift_space());
        assert_eq!(app.mode, AppMode::Scroll);
        app.handle_key(ctrl_shift_space());
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn plain_ctrl_space_also_toggles_scroll_mode() {
        // Some terminals can't distinguish Ctrl+Space from Ctrl+Shift+Space
        // at the protocol level, so the toggle must fire on Ctrl+Space alone.
        let mut app = App::new();
        app.handle_key(ctrl(KeyCode::Char(' ')));
        assert_eq!(app.mode, AppMode::Scroll);
        app.handle_key(ctrl(KeyCode::Char(' ')));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn scroll_mode_ignores_typing() {
        let mut app = App::new();
        app.handle_key(ctrl_shift_space());
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.input, "");
    }

    #[test]
    fn scroll_mode_up_down_adjust_scroll_offset() {
        let mut app = App::new();
        app.handle_key(ctrl_shift_space());
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.scroll_offset, 2);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn scroll_offset_clamps_to_actual_content_instead_of_growing_unbounded() {
        // 4 starting lines + the input line = 5 rows of content, viewport 3.
        let mut app = App::new();
        app.set_viewport_height(3);
        app.handle_key(ctrl_shift_space());

        for _ in 0..50 {
            app.handle_key(key(KeyCode::Up));
        }
        assert_eq!(app.scroll_offset, 2, "should stop at the true top (5 - 3), not keep growing");

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.scroll_offset, 1, "must move immediately, no leftover overshoot to pay off first");
    }

    #[test]
    fn escape_exits_scroll_mode() {
        let mut app = App::new();
        app.handle_key(ctrl_shift_space());
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn leaving_scroll_mode_resets_scroll_offset() {
        let mut app = App::new();
        app.handle_key(ctrl_shift_space());
        app.handle_key(key(KeyCode::PageUp));
        assert!(app.scroll_offset > 0);
        app.handle_key(ctrl_shift_space());
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn mouse_wheel_scrolls_in_either_mode() {
        let mut app = App::new();
        // Give it enough scrollback that WHEEL_SCROLL * 2 isn't clamped away
        // — with only the default banner lines there's nowhere near that
        // much content to scroll through.
        for _ in 0..5 {
            app.handle_key(key(KeyCode::Char('a')));
            app.handle_key(key(KeyCode::Enter));
        }
        app.set_viewport_height(3);

        app.handle_mouse(wheel(MouseEventKind::ScrollUp));
        assert_eq!(app.scroll_offset, WHEEL_SCROLL);

        app.handle_key(ctrl_shift_space());
        app.handle_mouse(wheel(MouseEventKind::ScrollUp));
        assert_eq!(app.scroll_offset, WHEEL_SCROLL * 2);

        app.handle_mouse(wheel(MouseEventKind::ScrollDown));
        assert_eq!(app.scroll_offset, WHEEL_SCROLL);
    }
}
