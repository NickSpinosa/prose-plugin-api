//! Telling the writer things and asking them things.

pub use crate::bindings::prose::plugin::ui::{
    Level, log, message, notify, prompt_choice, prompt_text,
};

/// Shows `text` in the status message area.
pub fn info(text: &str) {
    message(text, Level::Info);
}

/// Shows `text` as a warning.
pub fn warn(text: &str) {
    message(text, Level::Warning);
}

/// Shows `text` as an error.
pub fn error(text: &str) {
    message(text, Level::Error);
}

/// Asks a yes-or-no question with a notification. `true` if the writer
/// chose `yes`.
pub fn confirm(text: &str, yes: &str, no: &str) -> bool {
    notify(text, Level::Info, &[yes.to_string(), no.to_string()]) == Some(0)
}
