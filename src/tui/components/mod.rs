//! TUI components for rendering different views.

mod command_palette;
pub mod diff;
mod edit_buffer;
mod editor_view;
pub mod file_picker;
pub mod highlighting;
mod input_action;
mod markdown;
mod messages;
mod model_selection;
mod prompt;
mod session;
mod session_list;
mod text_layout;
mod welcome;

pub use command_palette::{
    dropdown_mode, filter_commands, filter_models, render_command_dropdown, render_model_dropdown,
    should_show_dropdown, DropdownMode,
};
pub use edit_buffer::EditBuffer;
#[allow(unused_imports)]
pub use editor_view::{EditorView, VisualCursor};

#[allow(unused_imports)]
pub use input_action::{build_keybinding_map, default_keybindings, InputAction, KeyBinding};
pub use model_selection::{render_model_selection_dialog, ModelSelectionDialog};
pub use prompt::{find_at_mention_spans, PLACEHOLDERS};
pub use session::{calculate_content_height, render_session, MESSAGE_PADDING_X};
pub use session_list::{render_session_list, SessionListDialog};
pub use text_layout::TextLayout;
pub use welcome::render_welcome;
