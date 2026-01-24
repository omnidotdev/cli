//! TUI view state.

/// The current view state of the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewState {
    /// Welcome screen with centered logo and prompt.
    #[default]
    Welcome,
    /// Active session with conversation history.
    Session,
}
