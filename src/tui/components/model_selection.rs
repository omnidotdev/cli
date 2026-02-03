//! Model selection dialog component.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::config::ModelInfo;
use crate::core::agent::AgentMode;

const BRAND_TEAL: Color = Color::Rgb(77, 201, 176);
const PLAN_PURPLE: Color = Color::Rgb(160, 100, 200);
const DIMMED: Color = Color::Rgb(100, 100, 110);
const INPUT_BG: Color = Color::Rgb(22, 24, 28);
const DIALOG_BG: Color = Color::Rgb(30, 32, 38);

/// Dialog state for model selection.
#[derive(Debug)]
pub struct ModelSelectionDialog {
    /// Current filter text for searching models.
    pub filter: String,
    /// All models grouped by provider (raw data).
    provider_models: Vec<(String, Vec<ModelInfo>)>,
    /// Index into the flat filtered list.
    selected_idx: usize,
    /// Scroll offset for viewport.
    scroll_offset: usize,
}

impl ModelSelectionDialog {
    /// Create a new model selection dialog with the given provider groups.
    pub fn new(provider_models: Vec<(String, Vec<ModelInfo>)>) -> Self {
        Self {
            filter: String::new(),
            provider_models,
            selected_idx: 0,
            scroll_offset: 0,
        }
    }

    /// Set the filter text and reset selection.
    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter.to_lowercase();
        self.selected_idx = 0;
        self.scroll_offset = 0;
    }

    /// Get the flat list of filtered models with their provider names.
    fn get_filtered_models(&self) -> Vec<(String, ModelInfo)> {
        let mut result = Vec::new();

        for (provider_name, models) in &self.provider_models {
            for model in models {
                if self.filter.is_empty()
                    || model.id.to_lowercase().contains(&self.filter)
                    || provider_name.to_lowercase().contains(&self.filter)
                {
                    result.push((provider_name.clone(), model.clone()));
                }
            }
        }

        result
    }

    /// Move selection to the next item.
    pub fn select_next(&mut self) {
        let filtered = self.get_filtered_models();
        if filtered.is_empty() {
            return;
        }
        if self.selected_idx < filtered.len().saturating_sub(1) {
            self.selected_idx += 1;
        }
    }

    /// Move selection to the previous item.
    pub fn select_previous(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    /// Get the currently selected model, if any.
    pub fn get_selected_model(&self) -> Option<ModelInfo> {
        let filtered = self.get_filtered_models();
        filtered.get(self.selected_idx).map(|(_, m)| m.clone())
    }

    /// Adjust scroll offset to keep selection visible within viewport.
    fn adjust_scroll(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }

        // Account for provider headers in the display
        let filtered = self.get_filtered_models();
        if filtered.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        // Calculate the visual line of the selected item
        let mut visual_line = 0;
        let mut current_provider: Option<&str> = None;

        for (idx, (provider, _)) in filtered.iter().enumerate() {
            if current_provider != Some(provider.as_str()) {
                current_provider = Some(provider.as_str());
                if idx <= self.selected_idx {
                    visual_line += 1; // Provider header line
                }
            }
            if idx == self.selected_idx {
                break;
            }
            visual_line += 1; // Model line
        }

        // Ensure selected item is visible
        if visual_line < self.scroll_offset {
            self.scroll_offset = visual_line;
        } else if visual_line >= self.scroll_offset + viewport_height {
            self.scroll_offset = visual_line.saturating_sub(viewport_height) + 1;
        }
    }
}

pub fn render_model_selection_dialog(
    frame: &mut Frame,
    dialog: &mut ModelSelectionDialog,
    agent_mode: AgentMode,
) {
    let area = frame.area();
    let dialog_width = (area.width * 60) / 100;
    let dialog_height = (area.height * 70) / 100;
    let dialog_x = (area.width - dialog_width) / 2;
    let dialog_y = (area.height - dialog_height) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let border_color = match agent_mode {
        AgentMode::Build => BRAND_TEAL,
        AgentMode::Plan => PLAN_PURPLE,
    };

    let block = Block::default()
        .title(" Select Model ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(DIALOG_BG));

    frame.render_widget(block.clone(), dialog_area);
    let inner_area = block.inner(dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search box
            Constraint::Min(1),    // Model list
            Constraint::Length(1), // Hints
        ])
        .split(inner_area);

    // Render search box
    let search_text = if dialog.filter.is_empty() {
        "Search models..."
    } else {
        &dialog.filter
    };
    let search_style = if dialog.filter.is_empty() {
        Style::default().fg(DIMMED)
    } else {
        Style::default().fg(Color::White)
    };

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(INPUT_BG));

    let search_para = Paragraph::new(search_text)
        .block(search_block)
        .style(search_style);
    frame.render_widget(search_para, chunks[0]);

    // Build the visual list with provider headers
    let list_area = chunks[1];
    let viewport_height = list_area.height as usize;

    // Adjust scroll before rendering
    dialog.adjust_scroll(viewport_height);

    let filtered = dialog.get_filtered_models();
    let mut lines: Vec<Line> = Vec::new();
    let mut current_provider: Option<&str> = None;

    for (idx, (provider, model)) in filtered.iter().enumerate() {
        // Add provider header if new provider
        if current_provider != Some(provider.as_str()) {
            current_provider = Some(provider.as_str());
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("▼", Style::default().fg(DIMMED)),
                Span::raw(" "),
                Span::styled(provider.clone(), Style::default().fg(border_color)),
            ]));
        }

        // Add model line
        let is_selected = idx == dialog.selected_idx;
        let model_style = if is_selected {
            Style::default().fg(Color::Black).bg(border_color)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled(model.id.clone(), model_style),
        ]));
    }

    // Handle empty state
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No models match your search",
            Style::default().fg(DIMMED),
        )));
    }

    // Apply scroll offset - skip lines and take viewport_height
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(dialog.scroll_offset)
        .take(viewport_height)
        .collect();

    let list_para = Paragraph::new(visible_lines);
    frame.render_widget(list_para, list_area);

    // Render scroll indicator if needed
    let total_lines = {
        let filtered = dialog.get_filtered_models();
        let mut count = 0;
        let mut seen_providers = std::collections::HashSet::new();
        for (provider, _) in &filtered {
            if seen_providers.insert(provider) {
                count += 1; // Provider header
            }
            count += 1; // Model
        }
        count
    };

    if total_lines > viewport_height {
        let scroll_indicator = format!(
            " {}/{} ",
            dialog.scroll_offset + 1,
            total_lines.saturating_sub(viewport_height) + 1
        );
        let indicator_x = list_area.x + list_area.width - scroll_indicator.len() as u16 - 1;
        let indicator_area = Rect::new(indicator_x, list_area.y, scroll_indicator.len() as u16, 1);
        let indicator = Paragraph::new(scroll_indicator).style(Style::default().fg(DIMMED));
        frame.render_widget(indicator, indicator_area);
    }

    // Render hints
    let hints = Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Color::White)),
        Span::styled(" Navigate  ", Style::default().fg(DIMMED)),
        Span::styled("Enter", Style::default().fg(Color::White)),
        Span::styled(" Select  ", Style::default().fg(DIMMED)),
        Span::styled("Esc", Style::default().fg(Color::White)),
        Span::styled(" Cancel", Style::default().fg(DIMMED)),
    ]);

    let hints_para = Paragraph::new(hints).alignment(Alignment::Center);
    frame.render_widget(hints_para, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dialog() -> ModelSelectionDialog {
        let provider_models = vec![
            (
                "anthropic".to_string(),
                vec![
                    ModelInfo {
                        id: "claude-sonnet-4".to_string(),
                        provider: "anthropic".to_string(),
                    },
                    ModelInfo {
                        id: "claude-opus-4".to_string(),
                        provider: "anthropic".to_string(),
                    },
                ],
            ),
            (
                "openai".to_string(),
                vec![
                    ModelInfo {
                        id: "gpt-4o".to_string(),
                        provider: "openai".to_string(),
                    },
                    ModelInfo {
                        id: "gpt-4-turbo".to_string(),
                        provider: "openai".to_string(),
                    },
                ],
            ),
        ];
        ModelSelectionDialog::new(provider_models)
    }

    #[test]
    fn test_filter_reduces_visible_models() {
        let mut dialog = create_test_dialog();
        dialog.set_filter("claude".to_string());

        let filtered = dialog.get_filtered_models();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|(_, m)| m.id.contains("claude")));
    }

    #[test]
    fn test_navigation_moves_through_filtered_models() {
        let mut dialog = create_test_dialog();

        assert_eq!(dialog.selected_idx, 0);

        dialog.select_next();
        assert_eq!(dialog.selected_idx, 1);

        dialog.select_next();
        assert_eq!(dialog.selected_idx, 2);

        dialog.select_previous();
        assert_eq!(dialog.selected_idx, 1);
    }

    #[test]
    fn test_get_selected_model_returns_correct_model() {
        let mut dialog = create_test_dialog();

        let model = dialog.get_selected_model();
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "claude-sonnet-4");

        dialog.select_next();
        let model = dialog.get_selected_model();
        assert_eq!(model.unwrap().id, "claude-opus-4");

        dialog.select_next();
        let model = dialog.get_selected_model();
        assert_eq!(model.unwrap().id, "gpt-4o");
    }

    #[test]
    fn test_filter_resets_selection() {
        let mut dialog = create_test_dialog();

        dialog.select_next();
        dialog.select_next();
        assert_eq!(dialog.selected_idx, 2);

        dialog.set_filter("gpt".to_string());
        assert_eq!(dialog.selected_idx, 0);

        let model = dialog.get_selected_model();
        assert_eq!(model.unwrap().id, "gpt-4o");
    }

    #[test]
    fn test_filter_by_provider_name() {
        let mut dialog = create_test_dialog();
        dialog.set_filter("openai".to_string());

        let filtered = dialog.get_filtered_models();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|(p, _)| p == "openai"));
    }

    #[test]
    fn test_empty_filter_shows_all() {
        let dialog = create_test_dialog();
        let filtered = dialog.get_filtered_models();
        assert_eq!(filtered.len(), 4);
    }

    #[test]
    fn test_navigation_bounds() {
        let mut dialog = create_test_dialog();

        // Can't go below 0
        dialog.select_previous();
        assert_eq!(dialog.selected_idx, 0);

        // Go to end
        dialog.select_next();
        dialog.select_next();
        dialog.select_next();
        assert_eq!(dialog.selected_idx, 3);

        // Can't go past end
        dialog.select_next();
        assert_eq!(dialog.selected_idx, 3);
    }
}
