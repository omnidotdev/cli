//! Input action definitions and keybinding system.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};

/// Actions that can be performed on the input buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum InputAction {
    // Movement (8)
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordLeft,
    MoveWordRight,
    MoveToStart,
    MoveToEnd,
    // Editing (4)
    DeleteCharBefore, // Backspace
    DeleteCharAfter,  // Delete
    InsertNewline,
    DeleteToStart, // Ctrl+U
    // Line operations (3)
    DeleteToEnd, // Ctrl+K
    DeleteWord,  // Ctrl+W
    InsertChar,  // Generic char insertion
}

/// A keybinding maps a key combination to an action
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
    pub action: InputAction,
}

impl KeyBinding {
    pub const fn new(key: KeyCode, modifiers: KeyModifiers, action: InputAction) -> Self {
        Self {
            key,
            modifiers,
            action,
        }
    }
}

/// Returns the default keybindings matching current TUI behavior
#[allow(dead_code)]
pub fn default_keybindings() -> Vec<KeyBinding> {
    vec![
        // Movement
        KeyBinding::new(KeyCode::Left, KeyModifiers::NONE, InputAction::MoveLeft),
        KeyBinding::new(KeyCode::Right, KeyModifiers::NONE, InputAction::MoveRight),
        KeyBinding::new(KeyCode::Up, KeyModifiers::NONE, InputAction::MoveUp),
        KeyBinding::new(KeyCode::Down, KeyModifiers::NONE, InputAction::MoveDown),
        KeyBinding::new(
            KeyCode::Left,
            KeyModifiers::CONTROL,
            InputAction::MoveWordLeft,
        ),
        KeyBinding::new(
            KeyCode::Right,
            KeyModifiers::CONTROL,
            InputAction::MoveWordRight,
        ),
        KeyBinding::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
            InputAction::MoveToStart,
        ),
        KeyBinding::new(
            KeyCode::Char('e'),
            KeyModifiers::CONTROL,
            InputAction::MoveToEnd,
        ),
        KeyBinding::new(KeyCode::Home, KeyModifiers::NONE, InputAction::MoveToStart),
        KeyBinding::new(KeyCode::End, KeyModifiers::NONE, InputAction::MoveToEnd),
        // Editing
        KeyBinding::new(
            KeyCode::Backspace,
            KeyModifiers::NONE,
            InputAction::DeleteCharBefore,
        ),
        KeyBinding::new(
            KeyCode::Delete,
            KeyModifiers::NONE,
            InputAction::DeleteCharAfter,
        ),
        KeyBinding::new(
            KeyCode::Enter,
            KeyModifiers::SHIFT,
            InputAction::InsertNewline,
        ),
        KeyBinding::new(
            KeyCode::Enter,
            KeyModifiers::ALT,
            InputAction::InsertNewline,
        ),
        // Line operations
        KeyBinding::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
            InputAction::DeleteToStart,
        ),
        KeyBinding::new(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
            InputAction::DeleteToEnd,
        ),
        KeyBinding::new(
            KeyCode::Char('w'),
            KeyModifiers::CONTROL,
            InputAction::DeleteWord,
        ),
    ]
}

/// Build a lookup map from key combinations to actions
#[allow(dead_code)]
pub fn build_keybinding_map(
    bindings: &[KeyBinding],
) -> HashMap<(KeyCode, KeyModifiers), InputAction> {
    bindings
        .iter()
        .map(|b| ((b.key, b.modifiers), b.action))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_keybindings_count() {
        let bindings = default_keybindings();
        assert!(
            bindings.len() >= 15,
            "Expected at least 15 keybindings, got {}",
            bindings.len()
        );
    }

    #[test]
    fn test_keybinding_map_lookup() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        // Test Ctrl+A -> MoveToStart
        assert_eq!(
            map.get(&(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            Some(&InputAction::MoveToStart)
        );

        // Test Ctrl+E -> MoveToEnd
        assert_eq!(
            map.get(&(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            Some(&InputAction::MoveToEnd)
        );
    }

    #[test]
    fn test_keybinding_map_movement_keys() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Left, KeyModifiers::NONE)),
            Some(&InputAction::MoveLeft)
        );
        assert_eq!(
            map.get(&(KeyCode::Right, KeyModifiers::NONE)),
            Some(&InputAction::MoveRight)
        );
        assert_eq!(
            map.get(&(KeyCode::Up, KeyModifiers::NONE)),
            Some(&InputAction::MoveUp)
        );
        assert_eq!(
            map.get(&(KeyCode::Down, KeyModifiers::NONE)),
            Some(&InputAction::MoveDown)
        );
    }

    #[test]
    fn test_keybinding_map_word_movement() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Left, KeyModifiers::CONTROL)),
            Some(&InputAction::MoveWordLeft)
        );
        assert_eq!(
            map.get(&(KeyCode::Right, KeyModifiers::CONTROL)),
            Some(&InputAction::MoveWordRight)
        );
    }

    #[test]
    fn test_keybinding_map_delete_operations() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(&InputAction::DeleteToStart)
        );
        assert_eq!(
            map.get(&(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            Some(&InputAction::DeleteToEnd)
        );
        assert_eq!(
            map.get(&(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            Some(&InputAction::DeleteWord)
        );
    }

    #[test]
    fn test_no_duplicate_keybindings() {
        let bindings = default_keybindings();
        let mut seen = std::collections::HashSet::new();

        for binding in &bindings {
            let key = (binding.key, binding.modifiers);
            assert!(
                !seen.contains(&key),
                "Duplicate keybinding: {:?} with modifiers {:?}",
                binding.key,
                binding.modifiers
            );
            seen.insert(key);
        }
    }

    #[test]
    fn test_keybinding_home_key() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Home, KeyModifiers::NONE)),
            Some(&InputAction::MoveToStart)
        );
    }

    #[test]
    fn test_keybinding_end_key() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::End, KeyModifiers::NONE)),
            Some(&InputAction::MoveToEnd)
        );
    }

    #[test]
    fn test_keybinding_newline_shift_enter() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Enter, KeyModifiers::SHIFT)),
            Some(&InputAction::InsertNewline)
        );
    }

    #[test]
    fn test_keybinding_newline_alt_enter() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Enter, KeyModifiers::ALT)),
            Some(&InputAction::InsertNewline)
        );
    }

    #[test]
    fn test_keybinding_backspace() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(&InputAction::DeleteCharBefore)
        );
    }

    #[test]
    fn test_keybinding_delete() {
        let bindings = default_keybindings();
        let map = build_keybinding_map(&bindings);

        assert_eq!(
            map.get(&(KeyCode::Delete, KeyModifiers::NONE)),
            Some(&InputAction::DeleteCharAfter)
        );
    }

    #[test]
    fn test_keybinding_struct_creation() {
        let binding = KeyBinding::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
            InputAction::MoveToStart,
        );

        assert_eq!(binding.key, KeyCode::Char('a'));
        assert_eq!(binding.modifiers, KeyModifiers::CONTROL);
        assert_eq!(binding.action, InputAction::MoveToStart);
    }
}
