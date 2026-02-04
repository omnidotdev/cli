//! TUI components for rendering different views.

mod command_palette;
mod markdown;
mod messages;
mod model_selection;
mod prompt;
mod session;
mod session_list;
mod text_layout;
mod welcome;

pub use command_palette::{
    DropdownMode, dropdown_mode, filter_commands, filter_models, render_command_dropdown,
    render_model_dropdown, should_show_dropdown,
};
pub use model_selection::{ModelSelectionDialog, render_model_selection_dialog};
pub use prompt::PLACEHOLDERS;
pub use session::{MESSAGE_PADDING_X, calculate_content_height, render_session};
pub use session_list::{SessionListDialog, render_session_list};
pub use text_layout::TextLayout;
pub use welcome::render_welcome;
