//! Compile and import hooks: change what Compile makes, add export formats
//! and import formats. See [`Compile`].

use crate::{Document, Result};

pub use crate::bindings::compile::exports::prose::plugin::compile::{
    Align, BodyLayout, CompileInfo, Imported, Layout, Note, PageSetup, ParagraphLayout, Role,
    RunningHeader,
};

/// A plugin's compile and import hooks. Implement it beside [`Plugin`] and
/// export both with `prose_plugin::export!(MyPlugin, compile)`. Name what it
/// offers in `plugin.toml` and ask for the `compile` permission:
///
/// ```toml
/// permissions = ["compile"]
/// compile-transform = true         # Prose calls `transform` on every compile
///
/// [[export-format]]                # a preset writes it with format = "<id>.fountain"
/// name = "fountain"
/// title = "Fountain screenplay"
/// extension = "fountain"
///
/// [[import-format]]                # files with these extensions import through it
/// name = "fountain"
/// title = "Fountain screenplay"
/// extensions = ["fountain"]
/// ```
///
/// Prose only calls the hooks `plugin.toml` names, so leave the others as
/// they are. None of them reach the disk: Prose reads and writes the files.
///
/// [`Plugin`]: crate::Plugin
pub trait Compile: crate::Plugin {
    /// Change the manuscript Compile assembled, after the preset's
    /// replacements and before the format is written. Return ops to apply
    /// to `manuscript`, in order; build them from its blocks as for an
    /// [`Edit`](crate::Edit). The default changes nothing.
    fn transform(
        &mut self,
        info: CompileInfo,
        manuscript: Document,
        notes: Vec<Note>,
    ) -> Result<Vec<crate::Op>> {
        let _ = (info, manuscript, notes);
        Ok(Vec::new())
    }

    /// Write the manuscript as export format `format` and return the file.
    fn export_format(
        &mut self,
        format: &str,
        info: CompileInfo,
        manuscript: Document,
        notes: Vec<Note>,
        layout: Layout,
    ) -> Result<Vec<u8>> {
        let _ = (info, manuscript, notes, layout);
        Err(format!("this plugin has no export format {format}"))
    }

    /// Read file `file_name`, holding `bytes`, as import format `format`,
    /// and return the documents it makes.
    fn import_format(
        &mut self,
        format: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<Vec<Imported>> {
        let _ = (file_name, bytes);
        Err(format!("this plugin has no import format {format}"))
    }
}
