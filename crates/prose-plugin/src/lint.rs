//! Checking paragraphs for Prose's spelling and grammar squiggles: the
//! `lint` export (WIT `prose:plugin/lint`).

pub use crate::bindings::lint::exports::prose::plugin::lint::{Diagnostic, DiagnosticKind};

/// A plugin that checks paragraphs. Implement it beside [`Plugin`] and
/// export with `prose_plugin::export!(MyPlugin, lint)`.
///
/// Prose asks about each paragraph on screen, and again once typing in it
/// pauses, and keeps the answer until the paragraph changes. So the answer
/// should depend only on the text and the plugin's settings.
///
/// ```ignore
/// impl Lint for FilterWords {
///     fn lint(&mut self, paragraph: &str) -> Vec<Diagnostic> {
///         paragraph
///             .match_indices("just")
///             .map(|(at, word)| {
///                 Diagnostic::style(at, at + word.len(), "filter_word", "\"just\" is a filter word")
///                     .suggest("")
///             })
///             .collect()
///     }
/// }
/// ```
///
/// [`Plugin`]: crate::Plugin
pub trait Lint: crate::Plugin {
    /// What is wrong with `paragraph` (one block's text), in any order.
    /// Ranges are byte offsets into it.
    fn lint(&mut self, paragraph: &str) -> Vec<Diagnostic>;
}

impl Diagnostic {
    /// A diagnostic of `kind` over bytes `start..end`, found by `rule`.
    pub fn new(
        kind: DiagnosticKind,
        start: usize,
        end: usize,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Diagnostic {
            start: start as u32,
            end: end as u32,
            message: message.into(),
            suggestions: Vec::new(),
            kind,
            rule: rule.into(),
        }
    }

    /// A misspelling.
    pub fn spelling(
        start: usize,
        end: usize,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(DiagnosticKind::Spelling, start, end, rule, message)
    }

    /// A grammar mistake.
    pub fn grammar(
        start: usize,
        end: usize,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(DiagnosticKind::Grammar, start, end, rule, message)
    }

    /// A matter of style.
    pub fn style(
        start: usize,
        end: usize,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(DiagnosticKind::Style, start, end, rule, message)
    }

    /// Offer `text` in place of the range (after any offered already). An
    /// empty one deletes it.
    pub fn suggest(mut self, text: impl Into<String>) -> Self {
        self.suggestions.push(text.into());
        self
    }
}
