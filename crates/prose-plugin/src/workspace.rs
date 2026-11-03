//! The open project and the panes showing its documents.

pub use crate::bindings::prose::plugin::workspace::{
    OpenDocument, OpenError, Placement, active_document, open, open_documents, project_name,
};

/// Shows document `id` in the active pane.
pub fn show(id: &str) -> Result<(), OpenError> {
    open(id, Placement::ActivePane)
}

/// Shows document `id` in a new pane beside the active one.
pub fn show_beside(id: &str) -> Result<(), OpenError> {
    open(id, Placement::SplitRight)
}
