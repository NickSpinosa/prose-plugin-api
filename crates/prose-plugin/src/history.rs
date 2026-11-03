//! The project's history, read only: drafts, revisions, a document's
//! versions and snapshots, and their text. Needs `read-project` in
//! `plugin.toml`'s permissions.

pub use crate::bindings::prose::plugin::history::{
    Change, Draft, Revision, RevisionKind, Snapshot, Version, drafts, revisions, snapshots,
    versions,
};

use crate::Document;

/// Document `document` as revision `revision` kept it, read only.
pub fn read(document: &str, revision: &str) -> Option<Document> {
    crate::bindings::prose::plugin::history::read(document, revision).map(Document::from_raw)
}
