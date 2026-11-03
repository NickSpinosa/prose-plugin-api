//! A fake Prose: just enough host to load a plugin component in Wasmtime,
//! run its commands against an in-memory document and see what it did. It
//! tests the WIT and the SDK end to end; it is not how Prose implements the
//! host, and it is not published.

use std::collections::HashMap;

use wasmtime::Result;
use wasmtime::component::{Component, HasSelf, Linker, Resource, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../wit/v0.1.0",
    world: "prose:plugin/plugin@0.1.0",
    with: {
        "prose:plugin/documents.document": DocHandle,
    },
    imports: { default: trappable },
});

/// The `lint` export of the `linter` world, for plugins that check text.
/// Its imports are the `plugin` world's, so only the export is new.
pub mod linter {
    wasmtime::component::bindgen!({
        path: "../../wit/v0.1.0",
        world: "prose:plugin/linter@0.1.0",
        with: {
            "prose:plugin/types": super::prose::plugin::types,
            "prose:plugin/documents": super::prose::plugin::documents,
            "prose:plugin/edits": super::prose::plugin::edits,
            "prose:plugin/commands": super::prose::plugin::commands,
            "prose:plugin/ui": super::prose::plugin::ui,
            "prose:plugin/settings": super::prose::plugin::settings,
            "prose:plugin/workspace": super::prose::plugin::workspace,
            "prose:plugin/project": super::prose::plugin::project,
            "prose:plugin/history": super::prose::plugin::history,
            "prose:plugin/binder": super::prose::plugin::binder,
        },
        imports: { default: trappable },
    });
}

pub use linter::exports::prose::plugin::lint::{Diagnostic, DiagnosticKind};

/// The `compile` export of the `compile-plugin` world, for plugins with
/// compile and import hooks. Its imports are the `plugin` world's.
pub mod compile_plugin {
    wasmtime::component::bindgen!({
        path: "../../wit/v0.1.0",
        world: "prose:plugin/compile-plugin@0.1.0",
        with: {
            "prose:plugin/types": super::prose::plugin::types,
            "prose:plugin/documents": super::prose::plugin::documents,
            "prose:plugin/edits": super::prose::plugin::edits,
            "prose:plugin/commands": super::prose::plugin::commands,
            "prose:plugin/ui": super::prose::plugin::ui,
            "prose:plugin/settings": super::prose::plugin::settings,
            "prose:plugin/workspace": super::prose::plugin::workspace,
            "prose:plugin/project": super::prose::plugin::project,
            "prose:plugin/history": super::prose::plugin::history,
            "prose:plugin/binder": super::prose::plugin::binder,
        },
        imports: { default: trappable },
    });
}

pub use compile_plugin::exports::prose::plugin::compile;
pub use prose::plugin::types::{Block, BlockKind};
pub use prose::plugin::ui::Level;
use prose::plugin::{commands, documents, edits, settings, types, ui, workspace};

/// A handle the plugin holds on the one fake document.
pub struct DocHandle;

/// The fake document.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub id: String,
    pub title: String,
    pub version: u64,
    pub blocks: Vec<Block>,
    pub selection: Option<types::Selection>,
}

impl Doc {
    /// A document of plain paragraphs with ids `b0`, `b1`, ...
    pub fn paragraphs(texts: &[&str]) -> Doc {
        Doc {
            id: "doc-1".into(),
            title: "Chapter One".into(),
            version: 1,
            blocks: texts
                .iter()
                .enumerate()
                .map(|(i, t)| Block {
                    id: format!("b{i}"),
                    kind: BlockKind::Paragraph,
                    text: t.to_string(),
                    runs: Vec::new(),
                    annotations: Vec::new(),
                })
                .collect(),
            selection: None,
        }
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    /// Applies the ops this fake understands: `replace` and `replace-text`
    /// within one block. Enough for the examples.
    fn apply(&mut self, ops: &[Op]) -> std::result::Result<(), edits::EditError> {
        let mut blocks = self.blocks.clone();
        for (index, op) in ops.iter().enumerate() {
            let bad = |message: &str| {
                edits::EditError::InvalidOp(edits::InvalidOp {
                    index: index as u32,
                    message: message.into(),
                })
            };
            let (block, start, end, text) = match op {
                Op::Replace(r) => (&r.block, r.start, r.end, &r.text),
                Op::ReplaceText(r) if r.span.start.block == r.span.end.block => (
                    &r.span.start.block,
                    r.span.start.offset,
                    r.span.end.offset,
                    &r.text,
                ),
                _ => return Err(bad("not supported by the test host")),
            };
            let b = blocks
                .iter_mut()
                .find(|b| &b.id == block)
                .ok_or_else(|| bad("no such block"))?;
            let (start, end) = (start as usize, end as usize);
            if start > end
                || end > b.text.len()
                || !b.text.is_char_boundary(start)
                || !b.text.is_char_boundary(end)
            {
                return Err(bad("out of bounds"));
            }
            b.text.replace_range(start..end, text);
        }
        self.blocks = blocks;
        self.version += 1;
        Ok(())
    }
}

/// Everything the fake records.
#[derive(Default)]
pub struct Prose {
    pub doc: Doc,
    /// Whether `current()` returns the document (true while a command with a
    /// document runs).
    pub in_command: bool,
    pub commands: Vec<commands::Command>,
    pub messages: Vec<(String, Level)>,
    pub log: Vec<String>,
    pub settings: HashMap<String, settings::Value>,
    /// Bumps the document's version once, before the next apply, as if the
    /// writer typed while the plugin worked.
    pub writer_types_once: bool,
    pub undo_steps: usize,
    pub next_id: u32,
    pub table: ResourceTable,
    pub wasi: WasiCtx,
}

impl WasiView for Prose {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl types::Host for Prose {}

impl documents::Host for Prose {
    fn current(&mut self) -> wasmtime::Result<Option<Resource<DocHandle>>> {
        if !self.in_command {
            return Ok(None);
        }
        Ok(Some(self.table.push(DocHandle)?))
    }

    fn new_id(&mut self) -> wasmtime::Result<String> {
        self.next_id += 1;
        Ok(format!("new-{}", self.next_id))
    }
}

impl documents::HostDocument for Prose {
    fn id(&mut self, _: Resource<DocHandle>) -> wasmtime::Result<String> {
        Ok(self.doc.id.clone())
    }
    fn title(&mut self, _: Resource<DocHandle>) -> wasmtime::Result<String> {
        Ok(self.doc.title.clone())
    }
    fn version(&mut self, _: Resource<DocHandle>) -> wasmtime::Result<u64> {
        Ok(self.doc.version)
    }
    fn block_count(&mut self, _: Resource<DocHandle>) -> wasmtime::Result<u32> {
        Ok(self.doc.blocks.len() as u32)
    }
    fn blocks(
        &mut self,
        _: Resource<DocHandle>,
        start: u32,
        count: u32,
    ) -> wasmtime::Result<Vec<Block>> {
        Ok(self
            .doc
            .blocks
            .iter()
            .skip(start as usize)
            .take(count as usize)
            .cloned()
            .collect())
    }
    fn block(&mut self, _: Resource<DocHandle>, id: String) -> wasmtime::Result<Option<Block>> {
        Ok(self.doc.index(&id).map(|i| self.doc.blocks[i].clone()))
    }
    fn block_index(&mut self, _: Resource<DocHandle>, id: String) -> wasmtime::Result<Option<u32>> {
        Ok(self.doc.index(&id).map(|i| i as u32))
    }
    fn word_count(&mut self, _: Resource<DocHandle>) -> wasmtime::Result<u32> {
        Ok(self
            .doc
            .blocks
            .iter()
            .map(|b| b.text.split_whitespace().count() as u32)
            .sum())
    }
    fn selection(&mut self, _: Resource<DocHandle>) -> wasmtime::Result<Option<types::Selection>> {
        Ok(self.doc.selection.clone())
    }
    fn utf16_offset(
        &mut self,
        _: Resource<DocHandle>,
        block: String,
        offset: u32,
    ) -> wasmtime::Result<Option<u32>> {
        Ok(self.doc.index(&block).and_then(|i| {
            let text = &self.doc.blocks[i].text;
            let prefix = text.get(..offset as usize)?;
            Some(prefix.encode_utf16().count() as u32)
        }))
    }
    fn byte_offset(
        &mut self,
        _: Resource<DocHandle>,
        block: String,
        units: u32,
    ) -> wasmtime::Result<Option<u32>> {
        Ok(self.doc.index(&block).and_then(|i| {
            let text = &self.doc.blocks[i].text;
            let mut counted = 0;
            for (byte, c) in text.char_indices() {
                if counted == units {
                    return Some(byte as u32);
                }
                counted += c.len_utf16() as u32;
            }
            (counted == units).then_some(text.len() as u32)
        }))
    }
    fn drop(&mut self, handle: Resource<DocHandle>) -> wasmtime::Result<()> {
        self.table.delete(handle)?;
        Ok(())
    }
}

impl edits::Host for Prose {
    fn apply(
        &mut self,
        _: Resource<DocHandle>,
        edit: types::Transaction,
    ) -> wasmtime::Result<std::result::Result<u64, edits::EditError>> {
        if std::mem::take(&mut self.writer_types_once) {
            self.doc.version += 1;
        }
        if edit.base_version != self.doc.version {
            return Ok(Err(edits::EditError::Stale));
        }
        Ok(self.doc.apply(&edit.ops).map(|()| {
            self.undo_steps += 1;
            self.doc.version
        }))
    }
}

impl commands::Host for Prose {
    fn register(
        &mut self,
        command: commands::Command,
    ) -> wasmtime::Result<std::result::Result<(), commands::RegisterError>> {
        if self.commands.iter().any(|c| c.name == command.name) {
            return Ok(Err(commands::RegisterError::Taken(command.name)));
        }
        if let Some(bad) = command
            .ex_names
            .iter()
            .find(|n| !n.starts_with(|c: char| c.is_ascii_uppercase()))
        {
            return Ok(Err(commands::RegisterError::BadName(bad.clone())));
        }
        self.commands.push(command);
        Ok(Ok(()))
    }
}

impl ui::Host for Prose {
    fn message(&mut self, text: String, level: Level) -> wasmtime::Result<()> {
        self.messages.push((text, level));
        Ok(())
    }
    fn notify(
        &mut self,
        text: String,
        level: Level,
        _buttons: Vec<String>,
    ) -> wasmtime::Result<Option<u32>> {
        self.messages.push((text, level));
        Ok(Some(0))
    }
    fn prompt_text(
        &mut self,
        _prompt: String,
        initial: Option<String>,
    ) -> wasmtime::Result<Option<String>> {
        Ok(initial)
    }
    fn prompt_choice(
        &mut self,
        _prompt: String,
        _choices: Vec<String>,
    ) -> wasmtime::Result<Option<u32>> {
        Ok(Some(0))
    }
    fn log(&mut self, _level: Level, text: String) -> wasmtime::Result<()> {
        self.log.push(text);
        Ok(())
    }
}

impl settings::Host for Prose {
    fn get(&mut self, key: String) -> wasmtime::Result<Option<settings::Value>> {
        Ok(self.settings.get(&key).cloned())
    }
    fn keys(&mut self) -> wasmtime::Result<Vec<String>> {
        Ok(self.settings.keys().cloned().collect())
    }
}

impl workspace::Host for Prose {
    fn project_name(&mut self) -> wasmtime::Result<Option<String>> {
        Ok(Some("The Lighthouse".into()))
    }
    fn open_documents(&mut self) -> wasmtime::Result<Vec<workspace::OpenDocument>> {
        Ok(vec![workspace::OpenDocument {
            id: self.doc.id.clone(),
            title: self.doc.title.clone(),
            active: true,
        }])
    }
    fn active_document(&mut self) -> wasmtime::Result<Option<String>> {
        Ok(Some(self.doc.id.clone()))
    }
    fn open(
        &mut self,
        document: String,
        _placement: workspace::Placement,
    ) -> wasmtime::Result<std::result::Result<(), workspace::OpenError>> {
        if document == self.doc.id {
            Ok(Ok(()))
        } else {
            Ok(Err(workspace::OpenError::NoSuchDocument))
        }
    }
}

/// What a plugin is granted, for linking.
#[derive(Clone, Copy, Debug, Default)]
pub struct Grants {
    pub edit_documents: bool,
}

/// A loaded plugin and its fake Prose.
pub struct Loaded {
    pub store: Store<Prose>,
    pub plugin: Plugin,
    /// Its `lint` export, if it has one (it targets the `linter` world).
    pub lint: Option<linter::exports::prose::plugin::lint::Guest>,
    /// Its compile hooks, if it exports them.
    pub compile: Option<compile::Guest>,
}

/// Loads the component in `wasm`, linking only what `grants` allows, as
/// Prose does, and calls `activate`.
pub fn load(wasm: &[u8], grants: Grants, prose: Prose) -> Result<Loaded> {
    let engine = Engine::default();
    let component = Component::new(&engine, wasm)?;
    let mut linker: Linker<Prose> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    types::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    documents::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    commands::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    ui::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    settings::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    workspace::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    if grants.edit_documents {
        edits::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
    }
    let mut store = Store::new(&engine, prose);
    let pre = linker.instantiate_pre(&component)?;
    let instance = pre.instantiate(&mut store)?;
    let plugin = Plugin::new(&mut store, &instance)?;
    // As Prose does: the export is there or it isn't, never required.
    let lint = match linter::exports::prose::plugin::lint::GuestIndices::new(&pre) {
        Ok(indices) => Some(indices.load(&mut store, &instance)?),
        Err(_) => None,
    };
    let compile = match compile::GuestIndices::new(&pre) {
        Ok(indices) => Some(indices.load(&mut store, &instance)?),
        Err(_) => None,
    };
    plugin
        .call_activate(&mut store)?
        .map_err(|e| wasmtime::Error::msg(format!("activate failed: {e}")))?;
    Ok(Loaded {
        store,
        plugin,
        lint,
        compile,
    })
}

impl Loaded {
    /// Runs command `name` on the fake document, as the writer would.
    pub fn run(&mut self, name: &str, args: &[&str]) -> Result<std::result::Result<(), String>> {
        let doc = &self.store.data().doc;
        let context = Context {
            document: Some(doc.id.clone()),
            selection: doc.selection.clone(),
            span: None,
            count: None,
        };
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        self.store.data_mut().in_command = true;
        let result = self
            .plugin
            .call_run_command(&mut self.store, name, &args, &context);
        self.store.data_mut().in_command = false;
        result
    }

    /// Asks the plugin's `lint` export about `paragraph`.
    pub fn lint(&mut self, paragraph: &str) -> Result<Vec<Diagnostic>> {
        let lint = self
            .lint
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("the plugin has no lint export"))?;
        lint.call_lint(&mut self.store, paragraph)
    }

    /// What compiling the fake document as the manuscript tells the
    /// plugin.
    pub fn info(&self, format: &str) -> compile::CompileInfo {
        compile::CompileInfo {
            preset: "Screenplay".into(),
            format: format.into(),
            title: self.prose().doc.title.clone(),
            author: "Ada Lovelace".into(),
            store: None,
        }
    }

    fn manuscript(&mut self) -> Result<Resource<DocHandle>> {
        Ok(self.store.data_mut().table.push(DocHandle)?)
    }

    fn hooks(&self) -> Result<&compile::Guest> {
        self.compile
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("the plugin exports no compile hooks"))
    }

    /// Runs the plugin's compile transform on the fake document and
    /// applies the ops it returns, as compile does.
    pub fn transform(&mut self) -> Result<std::result::Result<(), String>> {
        let info = self.info("docx");
        let manuscript = self.manuscript()?;
        let hooks = self.hooks()?.clone();
        let ops = match hooks.call_transform(&mut self.store, &info, manuscript, &[])? {
            Ok(ops) => ops,
            Err(error) => return Ok(Err(error)),
        };
        Ok(self
            .store
            .data_mut()
            .doc
            .apply(&ops)
            .map_err(|e| format!("{e:?}")))
    }

    /// Writes the fake document as the plugin's export format `format`.
    pub fn export(
        &mut self,
        format: &str,
        notes: &[compile::Note],
    ) -> Result<std::result::Result<Vec<u8>, String>> {
        let info = self.info(format);
        let manuscript = self.manuscript()?;
        let layout = compile::Layout {
            body: None,
            page: None,
            paragraphs: Vec::new(),
        };
        let hooks = self.hooks()?.clone();
        hooks.call_export_format(&mut self.store, format, &info, manuscript, notes, &layout)
    }

    /// Reads `bytes` as the plugin's import format `format`.
    pub fn import(
        &mut self,
        format: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<std::result::Result<Vec<compile::Imported>, String>> {
        let hooks = self.hooks()?.clone();
        hooks.call_import_format(&mut self.store, format, file_name, bytes)
    }

    pub fn prose(&self) -> &Prose {
        self.store.data()
    }
}
