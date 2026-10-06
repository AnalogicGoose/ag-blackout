use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::career::{CoreLesson, LessonId};
use crate::filesystem::VirtualPath;
use crate::shell::{GameSession, LineResult, builtins, parser, scenario};

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
    /// Entered via Ctrl+R. Typing narrows an incremental search through
    /// `history` (most recent entry containing the typed substring);
    /// Ctrl+R again steps to the next older match, Enter runs the current
    /// match immediately, Esc/Ctrl+G restores the pre-search input. Any
    /// other key exits back to `Normal`, keeping whatever's currently shown.
    ReverseSearch,
}

/// State for the terminal-emulator-style UI: the game session plus
/// everything needed to render/edit a live input line on top of scrollback.
/// Deliberately holds no `Terminal`/backend — that's `terminal.rs`'s job —
/// so this stays testable without a real TTY (see the tests below).
pub struct App {
    game: GameSession,
    pub lines: Vec<Line<'static>>,
    pub input: String,
    pub cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    pub scroll_offset: u16,
    pub mode: AppMode,
    viewport_height: u16,
    should_quit: bool,
    search_query: String,
    search_history_index: Option<usize>,
    saved_input: String,
    saved_cursor: usize,
    pending_sudo: Option<String>,
    sudo_password: String,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            game: GameSession::new(scenario::tutorial()),
            lines: Vec::new(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            scroll_offset: 0,
            mode: AppMode::Normal,
            viewport_height: 0,
            should_quit: false,
            search_query: String::new(),
            search_history_index: None,
            saved_input: String::new(),
            saved_cursor: 0,
            pending_sudo: None,
            sudo_password: String::new(),
        };
        app.push_plain("AG Linux 1.0.0 (Blackbird) — AnalogicGoose");
        app.push_plain(
            "Type 'tutorial' to see guided lessons, or 'tutorial start terminal' to begin.",
        );
        app.push_plain("Use 'help' to see all commands and '<command> --help' for details.");
        app.push_plain("");
        app
    }

    fn push_plain(&mut self, text: impl Into<String>) {
        self.lines.push(Line::from(text.into()));
    }

    pub fn prompt_string(&self) -> String {
        let user = self
            .game
            .active_shell()
            .active_device()
            .users
            .whoami(self.game.active_shell().context.uid)
            .unwrap_or("?");
        format!(
            "{user}@{}:{}$ ",
            self.game.active_shell().active_hostname(),
            self.game.active_shell().context.cwd
        )
    }

    /// Whether the prompt should use the remote-session color — the
    /// hostname/cwd text already says so, but a distinct color makes it
    /// impossible to miss mid-scrollback. See `render.rs`'s `prompt_color`.
    pub fn is_connected_remotely(&self) -> bool {
        self.game.active_shell().is_connected_remotely()
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn char_len(&self) -> usize {
        self.input.chars().count()
    }

    fn byte_index_of(&self, char_index: usize) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(char_index)
            .unwrap_or(self.input.len())
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

    /// Ctrl+Backspace/Ctrl+W: deletes back through any whitespace right
    /// before the cursor, then through the word behind that — matching
    /// readline's `unix-word-rubout` rather than stopping at the first
    /// whitespace, so it eats the gap between words too.
    fn delete_word_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.input.chars().collect();
        let mut start = self.cursor;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let start_byte = self.byte_index_of(start);
        let cursor_byte = self.byte_index_of(self.cursor);
        self.input.replace_range(start_byte..cursor_byte, "");
        self.cursor = start;
    }

    fn delete_to_line_start(&mut self) {
        let cursor_byte = self.byte_index_of(self.cursor);
        self.input.replace_range(0..cursor_byte, "");
        self.cursor = 0;
    }

    fn delete_to_line_end(&mut self) {
        let cursor_byte = self.byte_index_of(self.cursor);
        self.input.truncate(cursor_byte);
    }

    fn clear_screen(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_len());
    }

    /// The Fish-style autosuggestion for the current input: the remainder of
    /// the most recent history entry that starts with (but isn't equal to)
    /// the current input. Computed fresh from `input`/`history` rather than
    /// cached — it's a pure function of both, so there's no state to keep in
    /// sync as the user types. Only offered at the end of the line, matching
    /// Fish: a suggestion extends the line rightward from the cursor, which
    /// only makes sense there.
    pub fn suggestion(&self) -> Option<String> {
        if self.input.is_empty() || self.cursor != self.char_len() {
            return None;
        }
        self.history
            .iter()
            .rev()
            .find(|entry| entry.starts_with(&self.input) && entry.as_str() != self.input)
            .map(|entry| entry[self.input.len()..].to_string())
    }

    /// Accepts the current autosuggestion (if any) into the input line —
    /// `Right` at the end of the line does this instead of the normal
    /// (no-op, already-clamped) cursor move.
    fn accept_suggestion(&mut self) {
        if let Some(suggestion) = self.suggestion() {
            self.input.push_str(&suggestion);
            self.cursor = self.char_len();
        }
    }

    /// Char index where the word under/before the cursor starts — the
    /// boundary tab-completion replaces from.
    fn current_word_start(&self) -> usize {
        let chars: Vec<char> = self.input.chars().collect();
        let mut start = self.cursor.min(chars.len());
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        start
    }

    fn current_word(&self) -> String {
        let start = self.current_word_start();
        self.input
            .chars()
            .skip(start)
            .take(self.cursor - start)
            .collect()
    }

    /// True only when the word under the cursor is the line's first word —
    /// i.e. everything before it is whitespace. That's the one position
    /// completion treats as "a command name" rather than a path.
    fn completing_command_name(&self) -> bool {
        self.input
            .chars()
            .take(self.current_word_start())
            .all(char::is_whitespace)
    }

    /// Every whitespace-separated word before the one under the cursor —
    /// `preceding[0]` names the command, `preceding.len()` the argument
    /// position of the word being completed (1 = first argument). Same
    /// lightweight whitespace scan as `current_word_start`, not the real
    /// parser (see that method's doc comment for why).
    fn preceding_words(&self) -> Vec<&str> {
        self.input[..self.byte_index_of(self.current_word_start())]
            .split_whitespace()
            .collect()
    }

    /// Completion candidates for the word under the cursor: command names
    /// (from the real `builtins::table()`, not a separate list — see
    /// docs/GAME_DESIGN.md) for the first word; for a handful of commands
    /// whose argument shape is known, the real candidates for that argument
    /// (`argument_completion_candidates`); otherwise directory entries on the
    /// active device matching what's typed so far. Each candidate is the
    /// full replacement text for that word (directories end in `/`).
    fn completion_candidates(&self) -> Vec<String> {
        let word = self.current_word();
        if self.completing_command_name() {
            let mut names: Vec<String> = builtins::table()
                .keys()
                .filter(|name| name.starts_with(&word))
                .map(|name| name.to_string())
                .collect();
            names.sort();
            names
        } else {
            let preceding = self.preceding_words();
            self.argument_completion_candidates(&preceding, &word)
                .unwrap_or_else(|| self.path_completion_candidates(&word))
        }
    }

    /// Command/argument-specific candidates for the word under the cursor,
    /// or `None` to fall back to generic path completion. Covers the
    /// arguments whose real values are cheap to enumerate and worth
    /// completing: service names, observed hosts, AGPKG package names,
    /// contract ids, and pids.
    fn argument_completion_candidates(
        &self,
        preceding: &[&str],
        partial: &str,
    ) -> Option<Vec<String>> {
        let candidates = match preceding {
            ["tutorial"] => vec!["list", "start", "hint", "leave"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            ["tutorial", "start"] => vec!["terminal".to_string()],
            ["service"] => self.active_service_names(),
            ["exploit"] => self
                .game
                .active_shell()
                .career
                .knowledge
                .hostnames()
                .map(str::to_string)
                .collect(),
            ["exploit", hostname] => self
                .game
                .active_shell()
                .career
                .knowledge
                .services()
                .filter(|observation| observation.hostname == *hostname)
                .map(|observation| observation.name.clone())
                .collect(),
            ["agpkg", "install"] => self.installable_package_names(),
            ["agpkg", "remove"] => self.installed_package_names(),
            ["contracts", "accept"] => self.available_contract_ids(),
            ["kill"] => self.active_pids(),
            _ => return None,
        };
        let mut matches: Vec<String> = candidates
            .into_iter()
            .filter(|c| c.starts_with(partial))
            .collect();
        matches.sort();
        Some(matches)
    }

    fn active_service_names(&self) -> Vec<String> {
        self.game
            .active_shell()
            .active_device()
            .services
            .list()
            .into_iter()
            .map(|s| s.name.clone())
            .collect()
    }

    /// AGPKG catalog entries not already installed — installing an
    /// already-installed package is just an error, so it's not worth
    /// offering as a completion candidate.
    fn installable_package_names(&self) -> Vec<String> {
        let device = self.game.active_shell().active_device();
        device
            .packages
            .repository
            .all()
            .into_iter()
            .filter(|m| !device.packages.installed.is_installed(&m.name))
            .map(|m| m.name.clone())
            .collect()
    }

    fn installed_package_names(&self) -> Vec<String> {
        self.game
            .active_shell()
            .active_device()
            .packages
            .installed
            .list()
            .into_iter()
            .map(|p| p.name.clone())
            .collect()
    }

    fn available_contract_ids(&self) -> Vec<String> {
        self.game
            .active_shell()
            .contracts
            .available()
            .map(|c| c.id.to_string())
            .collect()
    }

    fn active_pids(&self) -> Vec<String> {
        self.game
            .active_shell()
            .active_device()
            .processes
            .list()
            .into_iter()
            .map(|p| p.pid.to_string())
            .collect()
    }

    fn path_completion_candidates(&self, partial: &str) -> Vec<String> {
        let (dir_part, prefix) = match partial.rfind('/') {
            Some(i) => (&partial[..=i], &partial[i + 1..]),
            None => ("", partial),
        };
        let dir_path = if dir_part.is_empty() {
            self.game.active_shell().context.cwd.clone()
        } else {
            match VirtualPath::resolve(&self.game.active_shell().context.cwd, dir_part) {
                Ok(p) => p,
                Err(_) => return Vec::new(),
            }
        };
        let access = self.game.active_shell().context.fs_access();
        let Ok(mut entries) = self
            .game
            .active_shell()
            .active_device()
            .filesystem
            .list_dir(&access, &dir_path)
        else {
            return Vec::new();
        };
        entries
            .retain(|e| e.name.starts_with(prefix) && (prefix.starts_with('.') || !e.is_hidden()));
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
            .into_iter()
            .map(|e| {
                let full = format!("{dir_part}{}", e.name);
                if e.node_type == 'd' {
                    format!("{full}/")
                } else {
                    full
                }
            })
            .collect()
    }

    /// `Tab`'s actual logic: a single unambiguous candidate is completed
    /// directly (with a trailing space, unless it's a directory — then a
    /// trailing `/` instead, so the player can keep completing deeper);
    /// multiple candidates are listed in the scrollback, same as a command's
    /// own output, without touching the input line.
    fn handle_tab(&mut self) {
        let candidates = self.completion_candidates();
        match candidates.len() {
            0 => {}
            1 => self.apply_completion(&candidates[0]),
            _ => self.push_plain(candidates.join("  ")),
        }
    }

    fn apply_completion(&mut self, candidate: &str) {
        let start = self.current_word_start();
        let start_byte = self.byte_index_of(start);
        let cursor_byte = self.byte_index_of(self.cursor);
        let mut replacement = candidate.to_string();
        if !replacement.ends_with('/') {
            replacement.push(' ');
        }
        let inserted_chars = replacement.chars().count();
        self.input
            .replace_range(start_byte..cursor_byte, &replacement);
        self.cursor = start + inserted_chars;
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

    fn reverse_search_match(&self, query: &str, upper: usize) -> Option<usize> {
        self.history[..upper]
            .iter()
            .rposition(|entry| entry.contains(query))
    }

    /// Re-runs the search from the most recent history entry, called after
    /// every query edit (typing a char, backspacing). Falls back to the
    /// pre-search input once the query goes empty.
    fn refresh_reverse_search(&mut self) {
        if self.search_query.is_empty() {
            self.search_history_index = None;
            self.input = self.saved_input.clone();
            self.cursor = self.char_len();
            return;
        }
        self.search_history_index =
            self.reverse_search_match(&self.search_query, self.history.len());
        if let Some(i) = self.search_history_index {
            self.input = self.history[i].clone();
            self.cursor = self.char_len();
        }
    }

    /// Ctrl+R while already searching: steps to the next older match for the
    /// same query, if any. A no-op until at least one match has been found.
    fn reverse_search_older(&mut self) {
        let Some(current) = self.search_history_index else {
            return;
        };
        if let Some(i) = self.reverse_search_match(&self.search_query, current) {
            self.search_history_index = Some(i);
            self.input = self.history[i].clone();
            self.cursor = self.char_len();
        }
    }

    fn start_reverse_search(&mut self) {
        self.saved_input = self.input.clone();
        self.saved_cursor = self.cursor;
        self.search_query.clear();
        self.search_history_index = None;
        self.mode = AppMode::ReverseSearch;
    }

    fn cancel_reverse_search(&mut self) {
        self.input = self.saved_input.clone();
        self.cursor = self.saved_cursor;
        self.mode = AppMode::Normal;
    }

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    fn submit(&mut self) {
        let input = self.input.clone();
        let prompt = self.prompt_string();
        let prompt_color = if self.is_connected_remotely() {
            Color::Cyan
        } else {
            Color::Green
        };
        self.lines.push(Line::from(vec![
            Span::styled(prompt, Style::default().fg(prompt_color)),
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
        if trimmed == "clear" {
            let result = self.game.execute_line(trimmed);
            if result.exit_code == 0 {
                self.clear_screen();
            } else {
                self.show_result(result, self.game.active_shell().economy.balance());
            }
            return;
        }
        if self.handle_tutorial_command(trimmed) {
            return;
        }
        if trimmed == "exit" || trimmed == "quit" {
            if self.game.leave_lesson() {
                self.push_plain("Returned to campaign.");
            } else {
                self.should_quit = true;
            }
            return;
        }

        let needs_sudo_prompt = parser::parse(trimmed, &self.game.active_shell().context.env)
            .ok()
            .is_some_and(|pipeline| {
                if pipeline.stages.len() != 1 {
                    return false;
                }
                let stage = &pipeline.stages[0];
                stage.redirections.is_empty()
                    && stage.argv.first().map(String::as_str) == Some("sudo")
                    && stage.argv.len() > 1
                    && !matches!(stage.argv[1].as_str(), "-h" | "--help")
            });

        if needs_sudo_prompt && !self.game.active_shell().sudo_is_cached() {
            let device = self.game.active_shell().active_device();
            if device
                .sudoers
                .permits(&device.users, self.game.active_shell().context.uid)
            {
                self.pending_sudo = Some(trimmed.to_string());
                return;
            }
        }

        let balance_before = self.game.active_shell().economy.balance();
        let was_completed = self.game.lesson_completed();
        let result = self.game.execute_line(trimmed);
        self.show_result(result, balance_before);
        if !was_completed && self.game.lesson_completed() {
            self.push_plain("Terminal lesson complete! Use 'tutorial leave' to return.");
        }
    }

    fn toggle_scroll_mode(&mut self) {
        self.mode = match self.mode {
            AppMode::Normal => AppMode::Scroll,
            AppMode::Scroll => {
                self.scroll_offset = 0;
                AppMode::Normal
            }
            // Ctrl+Space during an active reverse-search: same fallback as
            // any other unhandled key there (see the ReverseSearch branch
            // in handle_key) — just drops out of search, keeping the match.
            AppMode::ReverseSearch => AppMode::Scroll,
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
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(lines)
            .min(self.max_scroll());
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

        if self.pending_sudo.is_some() {
            self.handle_sudo_password_key(key);
            return;
        }

        if self.mode == AppMode::ReverseSearch {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Char('r') if ctrl => self.reverse_search_older(),
                KeyCode::Char('g') if ctrl => self.cancel_reverse_search(),
                KeyCode::Char('c') if ctrl => self.cancel_reverse_search(),
                KeyCode::Char(c) if !ctrl => {
                    self.search_query.push(c);
                    self.refresh_reverse_search();
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.refresh_reverse_search();
                }
                KeyCode::Enter => {
                    self.mode = AppMode::Normal;
                    self.submit();
                }
                KeyCode::Esc => self.cancel_reverse_search(),
                _ => self.mode = AppMode::Normal,
            }
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
                // Ctrl+Backspace and Ctrl+W both delete the previous word —
                // bound to both because plenty of terminals don't send a
                // distinct sequence for Ctrl+Backspace at all (same class of
                // issue as Ctrl+Space vs. Ctrl+Shift+Space above), while
                // Ctrl+W is the traditional, near-universally reliable
                // readline binding for the same action.
                KeyCode::Backspace | KeyCode::Char('w') => self.delete_word_before_cursor(),
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.char_len(),
                KeyCode::Char('u') => self.delete_to_line_start(),
                KeyCode::Char('k') => self.delete_to_line_end(),
                KeyCode::Char('l') => self.clear_screen(),
                KeyCode::Char('r') => self.start_reverse_search(),
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
            KeyCode::Right => {
                if self.cursor == self.char_len() {
                    self.accept_suggestion();
                } else {
                    self.move_right();
                }
            }
            KeyCode::Tab => self.handle_tab(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.char_len(),
            KeyCode::Up => self.history_up(),
            KeyCode::Down => self.history_down(),
            KeyCode::PageUp => self.scroll_up(PAGE_SCROLL),
            KeyCode::PageDown => self.scroll_down(PAGE_SCROLL),
            _ => {}
        }
    }

    pub fn sudo_prompt(&self) -> Option<String> {
        self.pending_sudo.as_ref()?;
        let user = self
            .game
            .active_shell()
            .active_device()
            .users
            .whoami(self.game.active_shell().context.uid)
            .unwrap_or("unknown");
        Some(format!("[sudo] password for {user}: "))
    }

    fn show_result(&mut self, result: LineResult, balance_before: i64) {
        for line in result.stdout.lines() {
            self.push_plain(line.to_string());
        }
        for line in result.stderr.lines() {
            self.lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Red),
            )));
        }
        if self.game.active_shell().economy.balance() != balance_before {
            self.push_plain(format!(
                "Contract complete — balance: ${}",
                self.game.active_shell().economy.balance()
            ));
        }
    }

    fn handle_sudo_password_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => {
                let prompt = self.sudo_prompt().unwrap();
                self.push_plain(prompt);

                let command = self.pending_sudo.take().unwrap();
                let password = std::mem::take(&mut self.sudo_password);
                let balance_before = self.game.active_shell().economy.balance();
                let result = self
                    .game
                    .active_shell_mut()
                    .execute_sudo(&command, &password);
                self.show_result(result, balance_before);
            }
            KeyCode::Esc => {
                self.pending_sudo = None;
                self.sudo_password.clear();
                self.push_plain("^C");
            }
            KeyCode::Char('c') if ctrl => {
                self.pending_sudo = None;
                self.sudo_password.clear();
                self.push_plain("^C");
            }
            KeyCode::Backspace => {
                self.sudo_password.pop();
            }
            KeyCode::Char(c) if !ctrl => self.sudo_password.push(c),
            _ => {}
        }
    }

    fn handle_tutorial_command(&mut self, input: &str) -> bool {
        match input.split_whitespace().collect::<Vec<_>>().as_slice() {
            ["tutorial", "-h" | "--help"] => {
                for line in builtins::help_text("tutorial").lines() {
                    self.push_plain(line.to_string());
                }
                true
            }
            ["tutorial"] | ["tutorial", "list"] => {
                let done = self
                    .game
                    .campaign()
                    .career
                    .tutorial
                    .core_completed(CoreLesson::Terminal);
                self.push_plain(format!(
                    "Terminal — {}",
                    if done {
                        "completed; replay available"
                    } else {
                        "available"
                    }
                ));
                self.push_plain("Start: tutorial start terminal");
                true
            }
            ["tutorial", "start", "terminal"] => {
                match self.game.start_lesson(LessonId::Core(CoreLesson::Terminal)) {
                    Ok(()) => {
                        self.push_plain("Terminal practice");
                        self.push_plain(
                            "You are on a practice machine. Use pwd to see your location and ls to inspect its files.",
                        );
                        self.push_plain(
                            "Goal: read intro.txt with cat. Use 'tutorial hint' if you get stuck.",
                        );
                        self.push_plain("Use 'tutorial leave' to return to your campaign.");
                    }
                    Err(error) => self.push_plain(format!("tutorial: {error}")),
                }
                true
            }
            ["tutorial", "hint"] => {
                if self.game.active_lesson() == Some(LessonId::Core(CoreLesson::Terminal)) {
                    self.push_plain("Hint: list the files in your current directory, then use cat on the one named intro.txt.");
                } else {
                    self.push_plain("tutorial: no lesson is active");
                }
                true
            }
            ["tutorial", "leave"] => {
                if self.game.leave_lesson() {
                    self.push_plain("Returned to campaign.");
                } else {
                    self.push_plain("tutorial: no lesson is active");
                }
                true
            }
            ["tutorial", ..] => {
                self.push_plain("Usage: tutorial [list | start terminal | hint | leave]");
                true
            }
            _ => false,
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
        // from the scrollback while Scroll Mode or reverse-search is active.
        let viewport_height = if app.mode == AppMode::Scroll || app.mode == AppMode::ReverseSearch {
            full_height.saturating_sub(1)
        } else {
            full_height
        };
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
        KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )
    }

    fn wheel(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn submit_line(app: &mut App, s: &str) {
        type_str(app, s);
        app.handle_key(key(KeyCode::Enter));
    }

    #[test]
    fn tutorial_and_clear_are_discoverable_and_the_lesson_runs_from_the_ui() {
        let mut app = App::new();
        submit_line(&mut app, "help");
        let help = app
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(help.contains("tutorial"));
        assert!(help.contains("clear"));

        submit_line(&mut app, "tutorial --help");
        assert!(app.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("Usage: tutorial"))
        }));

        submit_line(&mut app, "tutorial start terminal");
        assert_eq!(app.game.active_shell().active_hostname(), "training-local");
        submit_line(&mut app, "cat intro.txt");
        assert!(app.game.lesson_completed());
        assert!(app.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("Terminal lesson complete!"))
        }));

        submit_line(&mut app, "clear");
        assert!(app.lines.is_empty());
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.game.active_shell().active_hostname(), "training-local");

        submit_line(&mut app, "tutorial leave");
        assert_eq!(app.game.active_shell().active_hostname(), "localhost");
    }

    #[test]
    fn tab_completes_tutorial_command_and_subcommands() {
        let mut app = App::new();
        type_str(&mut app, "tut");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "tutorial ");

        app.input.clear();
        app.cursor = 0;
        type_str(&mut app, "tutorial st");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "tutorial start ");

        type_str(&mut app, "te");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "tutorial start terminal ");
    }

    #[test]
    fn sudo_prompts_after_the_command_without_echoing_or_saving_the_password() {
        let mut app = App::new();
        app.game
            .active_shell_mut()
            .execute_line("su admin admin123");

        submit_line(&mut app, "sudo whoami");
        assert_eq!(
            app.sudo_prompt().as_deref(),
            Some("[sudo] password for admin: ")
        );
        type_str(&mut app, "admin123");
        assert!(app.input.is_empty());
        assert_eq!(app.history.last().map(String::as_str), Some("sudo whoami"));
        assert!(!app.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("admin123"))
        }));

        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
        terminal.draw(|frame| render::draw(frame, &app)).unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(screen.contains("[sudo] password for admin:"));
        assert!(!screen.contains("admin123"));

        app.handle_key(key(KeyCode::Enter));
        assert!(app.sudo_prompt().is_none());
        assert!(
            app.lines
                .iter()
                .any(|line| line.spans.iter().any(|span| span.content == "root"))
        );
        assert_eq!(
            app.game
                .active_shell_mut()
                .execute_line("whoami")
                .stdout
                .trim(),
            "admin"
        );
        assert!(!app.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("admin123"))
        }));
    }

    #[test]
    fn sudo_password_prompt_can_be_cancelled_without_running_the_command() {
        let mut app = App::new();
        app.game
            .active_shell_mut()
            .execute_line("su admin admin123");
        submit_line(&mut app, "sudo whoami");
        type_str(&mut app, "admin123");

        app.handle_key(key(KeyCode::Esc));
        assert!(app.sudo_prompt().is_none());
        assert!(app.sudo_password.is_empty());
        assert!(
            !app.lines
                .iter()
                .any(|line| line.spans.iter().any(|span| span.content == "root"))
        );
        assert_eq!(
            app.game
                .active_shell_mut()
                .execute_line("whoami")
                .stdout
                .trim(),
            "admin"
        );
    }

    #[test]
    fn cached_sudo_runs_without_another_password_prompt() {
        let mut app = App::new();
        app.game
            .active_shell_mut()
            .execute_line("su admin admin123");

        submit_line(&mut app, "sudo whoami");
        assert!(app.sudo_prompt().is_some());
        type_str(&mut app, "admin123");
        app.handle_key(key(KeyCode::Enter));

        let lines_before = app.lines.len();
        submit_line(&mut app, "sudo whoami");
        assert!(app.sudo_prompt().is_none());
        assert!(app.lines.len() > lines_before);
        assert_eq!(
            app.game
                .active_shell_mut()
                .execute_line("whoami")
                .stdout
                .trim(),
            "admin"
        );
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
    fn ctrl_backspace_deletes_the_previous_word() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01");
        app.handle_key(ctrl(KeyCode::Backspace));
        assert_eq!(app.input, "connect ");
        assert_eq!(app.cursor, app.input.chars().count());
    }

    #[test]
    fn ctrl_w_also_deletes_the_previous_word() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01");
        app.handle_key(ctrl(KeyCode::Char('w')));
        assert_eq!(app.input, "connect ");
    }

    #[test]
    fn ctrl_backspace_leaves_whitespace_before_the_word_intact() {
        // Only whitespace directly at the cursor gets eaten (see the
        // "repeated" test below for that case) — here the cursor sits right
        // after "corp-fs01", so only that word goes, not the gap before it.
        let mut app = App::new();
        type_str(&mut app, "connect   corp-fs01");
        app.handle_key(ctrl(KeyCode::Backspace));
        assert_eq!(app.input, "connect   ");
    }

    #[test]
    fn ctrl_backspace_from_after_whitespace_eats_the_whitespace_and_the_word_before_it() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01 ");
        app.handle_key(ctrl(KeyCode::Backspace));
        assert_eq!(app.input, "connect ");
    }

    #[test]
    fn ctrl_backspace_from_mid_word_deletes_back_to_the_previous_boundary() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01");
        app.handle_key(key(KeyCode::Left)); // cursor now between '0' and '1'
        app.handle_key(ctrl(KeyCode::Backspace));
        assert_eq!(app.input, "connect 1");
    }

    #[test]
    fn ctrl_backspace_on_empty_input_does_nothing() {
        let mut app = App::new();
        app.handle_key(ctrl(KeyCode::Backspace));
        assert_eq!(app.input, "");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn ctrl_a_and_ctrl_e_jump_to_line_boundaries() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01");
        app.handle_key(ctrl(KeyCode::Char('a')));
        assert_eq!(app.cursor, 0);
        app.handle_key(ctrl(KeyCode::Char('e')));
        assert_eq!(app.cursor, app.input.chars().count());
    }

    #[test]
    fn ctrl_u_deletes_from_cursor_to_line_start() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01");
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        app.handle_key(ctrl(KeyCode::Char('u')));
        assert_eq!(app.input, "01");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn ctrl_k_deletes_from_cursor_to_line_end() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01");
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        app.handle_key(ctrl(KeyCode::Char('k')));
        assert_eq!(app.input, "connect corp-fs");
        assert_eq!(app.cursor, app.input.chars().count());
    }

    #[test]
    fn ctrl_l_clears_the_scrollback() {
        let mut app = App::new();
        assert!(!app.lines.is_empty());
        app.handle_key(ctrl(KeyCode::Char('l')));
        assert!(app.lines.is_empty());
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn ctrl_r_starts_reverse_search_and_shows_the_most_recent_match() {
        let mut app = App::new();
        submit_line(&mut app, "connect corp-fs01 admin admin123");
        submit_line(&mut app, "whoami");
        app.handle_key(ctrl(KeyCode::Char('r')));
        assert_eq!(app.mode, AppMode::ReverseSearch);
        type_str(&mut app, "conn");
        assert_eq!(app.input, "connect corp-fs01 admin admin123");
    }

    #[test]
    fn ctrl_r_again_steps_to_an_older_match() {
        let mut app = App::new();
        submit_line(&mut app, "connect corp-fs01 admin admin123");
        // "whoami" deliberately doesn't contain "connect" as a substring —
        // "disconnect" would, which is a legitimate match, not noise.
        submit_line(&mut app, "whoami");
        submit_line(&mut app, "connect meridian-web01 guest guest");
        app.handle_key(ctrl(KeyCode::Char('r')));
        type_str(&mut app, "connect");
        assert_eq!(app.input, "connect meridian-web01 guest guest");
        app.handle_key(ctrl(KeyCode::Char('r')));
        assert_eq!(app.input, "connect corp-fs01 admin admin123");
    }

    #[test]
    fn escape_cancels_reverse_search_and_restores_the_original_input() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "ps");
        app.handle_key(ctrl(KeyCode::Char('r')));
        type_str(&mut app, "who");
        assert_eq!(app.input, "whoami");
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.input, "ps");
    }

    #[test]
    fn enter_during_reverse_search_runs_the_matched_command() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        app.handle_key(ctrl(KeyCode::Char('r')));
        type_str(&mut app, "who");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.input, "");
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0], "whoami");
    }

    #[test]
    fn an_unhandled_key_during_reverse_search_exits_to_normal_mode_keeping_the_match() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        app.handle_key(ctrl(KeyCode::Char('r')));
        type_str(&mut app, "who");
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.input, "whoami");
    }

    #[test]
    fn reverse_search_with_no_match_does_not_panic_or_clear_the_query() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        app.handle_key(ctrl(KeyCode::Char('r')));
        type_str(&mut app, "zzz");
        assert_eq!(app.search_query(), "zzz");
        assert_eq!(app.mode, AppMode::ReverseSearch);
    }

    #[test]
    fn repeated_ctrl_backspace_clears_the_whole_line_word_by_word() {
        let mut app = App::new();
        type_str(&mut app, "connect corp-fs01 guest guest");
        for _ in 0..4 {
            app.handle_key(ctrl(KeyCode::Backspace));
        }
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

        for c in "contracts accept 1".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        for c in "connect corp-fs01 dvance Q3report!".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        for c in "download /home/dvance/report.pdf".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.game.active_shell_mut().economy.balance(), 3000);
        assert!(app.lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains("Contract complete"))
        }));
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
        assert_eq!(
            app.scroll_offset, 2,
            "should stop at the true top (5 - 3), not keep growing"
        );

        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.scroll_offset, 1,
            "must move immediately, no leftover overshoot to pay off first"
        );
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

    #[test]
    fn tab_completes_a_unique_command_name() {
        let mut app = App::new();
        type_str(&mut app, "contr");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "contracts ");
        assert_eq!(app.cursor, app.input.chars().count());
    }

    #[test]
    fn tab_lists_multiple_matching_commands_without_touching_the_input() {
        let mut app = App::new();
        type_str(&mut app, "con");
        let lines_before = app.lines.len();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "con");
        assert!(app.lines.len() > lines_before);
        let listed = app
            .lines
            .last()
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(listed.contains("connect"));
        assert!(listed.contains("contracts"));
    }

    #[test]
    fn tab_with_no_matches_does_nothing() {
        let mut app = App::new();
        type_str(&mut app, "zzz");
        let lines_before = app.lines.len();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "zzz");
        assert_eq!(app.lines.len(), lines_before);
    }

    #[test]
    fn tab_completes_a_file_in_the_current_directory() {
        let mut app = App::new();
        submit_line(&mut app, "touch note.txt");
        type_str(&mut app, "cat n");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "cat note.txt ");
    }

    #[test]
    fn tab_completes_a_directory_with_a_trailing_slash_and_no_space() {
        let mut app = App::new();
        submit_line(&mut app, "mkdir subdir");
        type_str(&mut app, "cd sub");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "cd subdir/");
    }

    #[test]
    fn tab_completes_a_service_name_for_the_service_command() {
        let mut app = App::new();
        type_str(&mut app, "service ss");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "service sshd ");
    }

    #[test]
    fn tab_completes_an_installable_package_name_for_agpkg_install() {
        let mut app = App::new();
        type_str(&mut app, "agpkg install nm");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "agpkg install nmap ");
    }

    #[test]
    fn tab_only_offers_not_yet_installed_packages_for_agpkg_install() {
        let mut app = App::new();
        app.game
            .active_shell_mut()
            .active_device_mut()
            .packages
            .installed
            .insert(crate::package::InstalledPackage {
                name: "nmap".to_string(),
                version: "7.94".to_string(),
                explicit: true,
            });
        type_str(&mut app, "agpkg install nm");
        let lines_before = app.lines.len();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "agpkg install nm"); // already installed — no candidates left
        assert_eq!(app.lines.len(), lines_before);
    }

    #[test]
    fn tab_completes_an_installed_package_name_for_agpkg_remove() {
        let mut app = App::new();
        app.game
            .active_shell_mut()
            .active_device_mut()
            .packages
            .installed
            .insert(crate::package::InstalledPackage {
                name: "nmap".to_string(),
                version: "7.94".to_string(),
                explicit: true,
            });
        type_str(&mut app, "agpkg remove nm");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "agpkg remove nmap ");
    }

    #[test]
    fn tab_completes_an_available_contract_id_for_contracts_accept() {
        let mut app = App::new();
        type_str(&mut app, "contracts accept 1");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "contracts accept 1 ");
    }

    #[test]
    fn tab_completes_a_pid_for_kill() {
        let mut app = App::new();
        type_str(&mut app, "kill 2");
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.input, "kill 2 ");
    }

    #[test]
    fn is_connected_remotely_reflects_shell_state() {
        let mut app = App::new();
        assert!(!app.is_connected_remotely());
        submit_line(&mut app, "connect corp-fs01 admin admin123");
        assert!(app.is_connected_remotely());
        submit_line(&mut app, "disconnect");
        assert!(!app.is_connected_remotely());
    }

    #[test]
    fn no_suggestion_for_empty_input() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        assert_eq!(app.suggestion(), None);
    }

    #[test]
    fn no_suggestion_without_a_matching_history_entry() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "zzz");
        assert_eq!(app.suggestion(), None);
    }

    #[test]
    fn no_suggestion_when_the_cursor_is_not_at_the_end_of_the_line() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "who");
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.suggestion(), None);
    }

    #[test]
    fn suggestion_offers_the_remainder_of_a_matching_history_entry() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "who");
        assert_eq!(app.suggestion(), Some("ami".to_string()));
    }

    #[test]
    fn suggestion_prefers_the_most_recent_matching_history_entry() {
        let mut app = App::new();
        submit_line(&mut app, "contracts accept 1"); // older match
        submit_line(&mut app, "contracts accept 5"); // more recent match
        type_str(&mut app, "contracts");
        assert_eq!(app.suggestion(), Some(" accept 5".to_string()));
    }

    #[test]
    fn suggestion_is_none_when_input_exactly_matches_the_most_recent_entry() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "whoami");
        assert_eq!(app.suggestion(), None);
    }

    #[test]
    fn right_arrow_at_end_of_line_accepts_the_suggestion() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "who");
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input, "whoami");
        assert_eq!(app.cursor, app.input.chars().count());
    }

    #[test]
    fn right_arrow_moves_the_cursor_normally_when_not_at_the_end_of_the_line() {
        let mut app = App::new();
        submit_line(&mut app, "whoami");
        type_str(&mut app, "who");
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        let cursor_before = app.cursor;
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.input, "who"); // suggestion not accepted
        assert_eq!(app.cursor, cursor_before + 1);
    }
}
