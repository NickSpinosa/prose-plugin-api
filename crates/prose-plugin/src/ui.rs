//! Telling the writer things and asking them things.

pub use crate::bindings::prose::plugin::ui::{
    Level, PickHow, PickItem, Picked, Picker, log, message, notify, open_picker, prompt_choice,
    prompt_text,
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

/// Shows a picker of `items` titled `title` and waits for the writer's
/// choice. For a long list, use [`open_picker`] and [`Picker::add`] to
/// stream it in while the writer looks.
pub fn pick(title: &str, items: Vec<PickItem>) -> Option<Picked> {
    open_picker(title, &items).wait()
}

impl PickItem {
    /// An item showing `title`, with no detail or preview.
    pub fn new(title: impl Into<String>) -> Self {
        PickItem {
            title: title.into(),
            detail: None,
            preview: None,
        }
    }

    /// With `detail` beside the title.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// With `preview` beside the list while it's selected, a paragraph per
    /// line. Any preview makes the picker a finder.
    pub fn preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self
    }
}

impl Picked {
    /// Whether the writer asked for a split (Ctrl-V or Ctrl-X).
    pub fn in_split(&self) -> bool {
        self.how != PickHow::Enter
    }
}
