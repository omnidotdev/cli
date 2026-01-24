//! Session list dialog for browsing and switching sessions.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::core::session::{Session, SessionManager};

/// Brand colors.
const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const DIALOG_BG: Color = Color::Rgb(30, 32, 38);
const SELECTED_BG: Color = Color::Rgb(45, 48, 55);

/// Session list dialog state.
pub struct SessionListDialog {
    /// Sessions to display.
    sessions: Vec<Session>,
    /// Currently selected index.
    selected: usize,
    /// Search/filter input.
    filter: String,
    /// List widget state.
    list_state: ListState,
}

impl SessionListDialog {
    /// Create a new session list dialog.
    #[must_use]
    pub fn new(sessions: Vec<Session>) -> Self {
        let mut list_state = ListState::default();
        if !sessions.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            sessions,
            selected: 0,
            filter: String::new(),
            list_state,
        }
    }

    /// Load sessions from a session manager.
    ///
    /// # Errors
    ///
    /// Returns error if session loading fails.
    pub fn from_manager(manager: &SessionManager) -> anyhow::Result<Self> {
        let sessions = manager.list_sessions()?;
        Ok(Self::new(sessions))
    }

    /// Get filtered sessions.
    fn filtered_sessions(&self) -> Vec<&Session> {
        if self.filter.is_empty() {
            self.sessions.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.sessions
                .iter()
                .filter(|s| s.title.to_lowercase().contains(&filter_lower))
                .collect()
        }
    }

    /// Move selection up.
    pub fn select_previous(&mut self) {
        let filtered = self.filtered_sessions();
        if filtered.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.list_state.select(Some(self.selected));
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        let filtered = self.filtered_sessions();
        if filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(filtered.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    /// Get the currently selected session.
    #[must_use]
    pub fn selected_session(&self) -> Option<&Session> {
        let filtered = self.filtered_sessions();
        filtered.get(self.selected).copied()
    }

    /// Set filter text.
    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.selected = 0;
        self.list_state
            .select(if self.filtered_sessions().is_empty() {
                None
            } else {
                Some(0)
            });
    }

    /// Add character to filter.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
        self.list_state
            .select(if self.filtered_sessions().is_empty() {
                None
            } else {
                Some(0)
            });
    }

    /// Remove character from filter.
    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.selected = 0;
        self.list_state
            .select(if self.filtered_sessions().is_empty() {
                None
            } else {
                Some(0)
            });
    }

    /// Get the filter text.
    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Get mutable reference to list state.
    pub const fn list_state_mut(&mut self) -> &mut ListState {
        &mut self.list_state
    }
}

/// Format a timestamp as a relative time string.
fn format_relative_time(timestamp: i64) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let diff_ms = now - timestamp;
    let diff_secs = diff_ms / 1000;
    let diff_mins = diff_secs / 60;
    let diff_hours = diff_mins / 60;
    let diff_days = diff_hours / 24;

    if diff_secs < 60 {
        "just now".to_string()
    } else if diff_mins < 60 {
        format!("{diff_mins}m ago")
    } else if diff_hours < 24 {
        format!("{diff_hours}h ago")
    } else if diff_days < 7 {
        format!("{diff_days}d ago")
    } else {
        chrono::DateTime::from_timestamp_millis(timestamp)
            .map(|dt| dt.format("%b %d").to_string())
            .unwrap_or_default()
    }
}

/// Render the session list dialog.
#[allow(clippy::cast_possible_truncation)]
pub fn render_session_list(frame: &mut Frame, dialog: &mut SessionListDialog) {
    let area = frame.area();

    // Center the dialog.
    let dialog_width = (area.width * 3 / 4).min(80);
    let dialog_height = (area.height * 3 / 4).min(30);
    let dialog_x = (area.width - dialog_width) / 2;
    let dialog_y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    // Clear background.
    frame.render_widget(Clear, dialog_area);

    // Main block.
    let block = Block::default()
        .title(" Sessions ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BRAND_TEAL))
        .style(Style::default().bg(DIALOG_BG));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Layout: search box + list + help text.
    let chunks = Layout::vertical([
        Constraint::Length(3), // Search
        Constraint::Min(3),    // List
        Constraint::Length(2), // Help
    ])
    .split(inner);

    // Search box.
    let search_text = if dialog.filter.is_empty() {
        Line::from(Span::styled(
            "Type to filter...",
            Style::default().fg(DIMMED),
        ))
    } else {
        Line::from(Span::styled(
            dialog.filter(),
            Style::default().fg(Color::White),
        ))
    };

    let search_block = Block::default()
        .title(" Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIMMED));

    let search = Paragraph::new(search_text).block(search_block);
    frame.render_widget(search, chunks[0]);

    // Session list - build items from owned data to avoid borrow conflicts.
    let selected_idx = dialog.selected;
    let items: Vec<ListItem> = dialog
        .sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            if dialog.filter.is_empty() {
                true
            } else {
                s.title
                    .to_lowercase()
                    .contains(&dialog.filter.to_lowercase())
            }
        })
        .enumerate()
        .map(|(display_idx, (_, session))| {
            let is_selected = display_idx == selected_idx;
            let time_str = format_relative_time(session.time.updated);

            let style = if is_selected {
                Style::default()
                    .bg(SELECTED_BG)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let time_style = if is_selected {
                Style::default().bg(SELECTED_BG).fg(DIMMED)
            } else {
                Style::default().fg(DIMMED)
            };

            let line = Line::from(vec![
                Span::styled(format!(" {}", if is_selected { "▸" } else { " " }), style),
                Span::styled(session.title.clone(), style),
                Span::styled("  ", style),
                Span::styled(time_str, time_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DIMMED)),
    );

    frame.render_stateful_widget(list, chunks[1], dialog.list_state_mut());

    // Help text.
    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(BRAND_TEAL)),
        Span::styled(" navigate  ", Style::default().fg(DIMMED)),
        Span::styled("Enter", Style::default().fg(BRAND_TEAL)),
        Span::styled(" select  ", Style::default().fg(DIMMED)),
        Span::styled("Esc", Style::default().fg(BRAND_TEAL)),
        Span::styled(" close  ", Style::default().fg(DIMMED)),
        Span::styled("n", Style::default().fg(BRAND_TEAL)),
        Span::styled(" new session", Style::default().fg(DIMMED)),
    ]))
    .alignment(Alignment::Center);

    frame.render_widget(help, chunks[2]);
}
