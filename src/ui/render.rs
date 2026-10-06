use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::app::{App, AppMode};

/// Renders one continuous scrollback pane — no borders, no separate input
/// box — so it reads as a real terminal rather than a Ratatui app with a
/// terminal widget inside it. The in-progress input line is appended as the
/// last line so it scrolls with everything else. In `Scroll` mode, and while
/// an incremental reverse-history search (`Ctrl+R`) is in progress, a status
/// line is reserved at the bottom so the mode is never silently active.
pub fn draw(frame: &mut Frame, app: &App) {
    let full_area = frame.size();

    let reserves_status_row = matches!(app.mode, AppMode::Scroll | AppMode::ReverseSearch);
    let (area, status_area) = if reserves_status_row {
        let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(full_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (full_area, None)
    };

    let sudo_prompt = app.sudo_prompt();
    let prompt = sudo_prompt.clone().unwrap_or_else(|| app.prompt_string());
    let input_line = if sudo_prompt.is_some() {
        // The password stays in App's private buffer and is never drawn.
        Line::from(Span::styled(
            prompt.clone(),
            Style::default().fg(Color::Yellow),
        ))
    } else if app.mode == AppMode::ReverseSearch {
        // Bash-style: the normal prompt is replaced by the search prompt
        // while searching, showing whichever history entry currently matches.
        Line::from(vec![
            Span::styled(
                format!("(reverse-i-search)`{}': ", app.search_query()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(app.input.clone()),
        ])
    } else {
        // A distinct prompt color while connected to a remote device — the
        // hostname/cwd text already says so (App::prompt_string), but a
        // color cue makes it impossible to miss mid-scrollback, especially
        // once output has scrolled the earlier `connect` line out of view.
        let prompt_color = if app.is_connected_remotely() {
            Color::Cyan
        } else {
            Color::Green
        };
        let mut input_spans = vec![
            Span::styled(prompt.clone(), Style::default().fg(prompt_color)),
            Span::raw(app.input.clone()),
        ];
        if let Some(suggestion) = app.suggestion() {
            // Fish-style: the remainder of a matching history entry, dimmed,
            // shown right after the cursor. Never part of the real input until
            // accepted (Right arrow at end of line — see App::accept_suggestion).
            input_spans.push(Span::styled(
                suggestion,
                Style::default().fg(Color::DarkGray),
            ));
        }
        Line::from(input_spans)
    };

    let mut lines = app.lines.clone();
    lines.push(input_line);

    let total = lines.len() as u16;
    let viewport = area.height;
    let max_scroll_from_top = total.saturating_sub(viewport);
    // scroll_offset counts lines scrolled *up* from the bottom; 0 means
    // "follow the latest output," matching how a real terminal behaves.
    let scroll_from_top = max_scroll_from_top.saturating_sub(app.scroll_offset);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_from_top, 0));
    frame.render_widget(paragraph, area);

    if let Some(status_area) = status_area {
        let status_line = match app.mode {
            AppMode::Scroll => Line::from(Span::styled(
                "-- SCROLL MODE -- \u{2191}/\u{2193} PgUp/PgDn Home/End to navigate, Ctrl+Shift+Space or Esc to exit",
                Style::default().fg(Color::Black).bg(Color::Yellow),
            )),
            AppMode::ReverseSearch => Line::from(Span::styled(
                "-- REVERSE SEARCH -- type to narrow, Ctrl+R for an older match, Enter to run, Esc to cancel",
                Style::default().fg(Color::Black).bg(Color::Yellow),
            )),
            AppMode::Normal => {
                unreachable!("status_area is only Some in Scroll or ReverseSearch mode")
            }
        };
        frame.render_widget(Paragraph::new(status_line), status_area);
    }

    // Only show a live cursor in Normal mode while following the bottom —
    // in Scroll/ReverseSearch mode, or once scrolled back into history,
    // nothing is being edited in view via the real cursor.
    if app.mode == AppMode::Normal && app.scroll_offset == 0 {
        // The input line is content row `total - 1`; its screen row is that
        // minus however much got scrolled off the top. Using the viewport's
        // last row here instead was the bug: it's only correct once content
        // actually fills the screen, not while there's still empty space
        // below a short scrollback.
        let content_row = total.saturating_sub(1);
        let screen_row = content_row.saturating_sub(scroll_from_top);
        let cursor_x = area.x + (prompt.chars().count() + app.cursor) as u16;
        let cursor_y = area.y + screen_row;
        frame.set_cursor(cursor_x, cursor_y);
    }
}
