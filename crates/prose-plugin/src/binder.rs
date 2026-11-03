//! Changing the binder: making, retitling, moving, duplicating and
//! trashing items, their metadata and custom fields, and replacing text
//! across the project. Needs `edit-binder` in `plugin.toml`'s permissions.
//!
//! Each call is one change the writer can undo from the binder. Items go to
//! the trash, never away.

pub use crate::bindings::prose::plugin::binder::{
    BinderError, CustomChange, MetaChange, create, duplicate, from_template, move_, replace,
    retitle, set_meta, trash,
};
