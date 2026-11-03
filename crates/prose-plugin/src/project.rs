//! The whole project: the binder and every item's metadata and notes, any
//! document, project search, word counts and the writing history. Needs
//! `read-project` in `plugin.toml`'s permissions.
//!
//! ```ignore
//! use prose_plugin::project::{self, Query};
//!
//! for hit in project::search(&Query::new("very"), 1000)? {
//!     // ...
//! }
//! ```

pub use crate::bindings::prose::plugin::project::{
    CustomValue, Day, Definition, Field, FieldKind, FieldValue, Hit, Item, ItemKind, Place, Query,
    RootFolder, SearchError, daily_words, fields, find, items, labels, notes, root, search,
    session_target, session_words, statuses, word_count,
};

use crate::Document;

/// Any document in the project, open in a pane or not: the pane's text when
/// one shows it, else its file. Only a document a pane shows can be edited.
pub fn open(id: &str) -> Option<Document> {
    crate::bindings::prose::plugin::project::open(id).map(Document::from_raw)
}

/// Every document in the manuscript, in reading order.
pub fn manuscript_documents() -> Vec<Item> {
    items()
        .into_iter()
        .filter(|item| item.root == RootFolder::Manuscript && item.kind == ItemKind::Document)
        .collect()
}

impl Query {
    /// `text`, matched as the search panel matches by default: ignoring
    /// case, anywhere in a word, everywhere but the trash.
    pub fn new(text: impl Into<String>) -> Self {
        Query {
            text: text.into(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            places: Vec::new(),
            folders: Vec::new(),
        }
    }

    /// Match case.
    pub fn case_sensitive(mut self) -> Self {
        self.case_sensitive = true;
        self
    }

    /// Match whole words only.
    pub fn whole_word(mut self) -> Self {
        self.whole_word = true;
        self
    }

    /// The text is a regular expression.
    pub fn regex(mut self) -> Self {
        self.regex = true;
        self
    }

    /// Look only in `places` of each document.
    pub fn only_in(mut self, places: &[Place]) -> Self {
        self.places = places.to_vec();
        self
    }

    /// Look only in these top-level folders.
    pub fn folders(mut self, folders: &[RootFolder]) -> Self {
        self.folders = folders.to_vec();
        self
    }
}
