use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::app::{App, AppMode};

/// Renders one continuous scrollback pane — no borders, no separate input
/// box — so it reads as a real terminal rather than a Ratatui app with a
/// terminal widget inside it. The in-progress input line is appended as the
/// last line so it scrolls with everything else. In `Scroll` mode a status
/// line is reserved at the bottom so the mode is never silently active.
pub fn draw(frame: &mut Frame, app: &App) {
    let full_area = frame.size();

    let (area, status_area) = if app.mode == AppMode::Scroll {
        let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(full_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (full_area, None)
    };

    let prompt = app.prompt_string();
    let mut input_spans = vec![
        Span::styled(prompt.clone(), Style::default().fg(Color::Green)),
        Span::raw(app.input.clone()),
    ];
    if let Some(suggestion) = app.suggestion() {
        // Fish-style: the remainder of a matching history entry, dimmed,
        // shown right after the cursor. Never part of the real input until
        // accepted (Right arrow at end of line — see App::accept_suggestion).
        input_spans.push(Span::styled(suggestion, Style::default().fg(Color::DarkGray)));
    }
    let input_line = Line::from(input_spans);

    let mut lines = app.lines.clone();
    lines.push(input_line);

    let total = lines.len() as u16;
    let viewport = area.height;
    let max_scroll_from_top = total.saturating_sub(viewport);
    // scroll_offset counts lines scrolled *up* from the bottom; 0 means
    // "follow the latest output," matching how a real terminal behaves.
    let scroll_from_top = max_scroll_from_top.saturating_sub(app.scroll_offset);

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((scroll_from_top, 0));
    frame.render_widget(paragraph, area);

    if let Some(status_area) = status_area {
        let status = Paragraph::new(Line::from(Span::styled(
            "-- SCROLL MODE -- \u{2191}/\u{2193} PgUp/PgDn Home/End to navigate, Ctrl+Shift+Space or Esc to exit",
            Style::default().fg(Color::Black).bg(Color::Yellow),
        )));
        frame.render_widget(status, status_area);
    }

    // Only show a live cursor in Normal mode while following the bottom —
    // in Scroll mode, or once scrolled back into history, nothing is being
    // edited in view.
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
