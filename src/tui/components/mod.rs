//! TUI components for rendering different views.

mod command_palette;
mod markdown;
mod messages;
mod prompt;
mod session;
mod session_list;
mod welcome;

pub use command_palette::{
    DropdownMode, dropdown_mode, filter_commands, filter_models, render_command_dropdown,
    render_model_dropdown, should_show_dropdown,
};
pub use prompt::PLACEHOLDERS;
pub use session::{MESSAGE_PADDING_X, calculate_content_height, render_session};
pub use session_list::{SessionListDialog, render_session_list};
pub use welcome::render_welcome;
