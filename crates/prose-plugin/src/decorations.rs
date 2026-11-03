//! Highlights, underlines, margin marks and hints over the writer's text,
//! drawn by Prose in the theme and never saved.
//!
//! A plugin has one set of decorations per document. [`Decorations`]
//! collects them and [`Decorations::show`] replaces the plugin's set with
//! them:
//!
//! ```ignore
//! use prose_plugin::decorations::{Color, Decorations};
//!
//! let mut marks = Decorations::new();
//! for block in doc.blocks() {
//!     if let Some(at) = block.text.find("very") {
//!         let span = Span::in_block(&block.id, at as u32, at as u32 + 4);
//!         marks.underline(span, Color::Warning, true);
//!     }
//! }
//! marks.show(&doc).map_err(|e| e.to_string())?;
//! ```

pub use crate::bindings::prose::plugin::decorations::{
    Color, Decoration, DecorationError, Highlight, Hint, InvalidDecoration, MarginMark, Underline,
    clear, set,
};
use crate::{Document, Position, Span};

/// A list of decorations to show on one document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decorations {
    list: Vec<Decoration>,
}

impl Decorations {
    /// No decorations yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A background behind `span`, which must not be empty.
    pub fn highlight(&mut self, span: Span, color: Color) -> &mut Self {
        self.push(Decoration::Highlight(Highlight { span, color }))
    }

    /// A line under `span`, which must not be empty; `wavy` for a squiggle.
    pub fn underline(&mut self, span: Span, color: Color, wavy: bool) -> &mut Self {
        self.push(Decoration::Underline(Underline { span, color, wavy }))
    }

    /// A one- or two-character mark in the margin beside the line `at` is
    /// on.
    pub fn margin_mark(&mut self, at: Position, color: Color, text: &str) -> &mut Self {
        self.push(Decoration::MarginMark(MarginMark {
            at,
            color,
            text: text.into(),
        }))
    }

    /// A short line of text after the end of the line `at` is on.
    pub fn hint(&mut self, at: Position, color: Color, text: &str) -> &mut Self {
        self.push(Decoration::Hint(Hint {
            at,
            color,
            text: text.into(),
        }))
    }

    /// Adds any decoration.
    pub fn push(&mut self, decoration: Decoration) -> &mut Self {
        self.list.push(decoration);
        self
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The decorations, in the order they were added.
    pub fn as_slice(&self) -> &[Decoration] {
        &self.list
    }

    /// Show these on `doc` in place of the plugin's decorations there, as
    /// placed in `doc` as the plugin read it.
    pub fn show(&self, doc: &Document) -> Result<(), DecorationError> {
        set(&doc.id(), doc.version(), &self.list)
    }

    /// Show these on document `id` at `version`, for a document the plugin
    /// didn't get a [`Document`] for (from an event, say).
    pub fn show_on(&self, id: &str, version: u64) -> Result<(), DecorationError> {
        set(id, version, &self.list)
    }
}

impl From<Vec<Decoration>> for Decorations {
    fn from(list: Vec<Decoration>) -> Self {
        Self { list }
    }
}

impl Extend<Decoration> for Decorations {
    fn extend<T: IntoIterator<Item = Decoration>>(&mut self, iter: T) {
        self.list.extend(iter);
    }
}

impl Document {
    /// Remove the plugin's decorations from this document.
    pub fn clear_decorations(&self) {
        clear(&self.id());
    }
}
