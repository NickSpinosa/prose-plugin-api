use crate::bindings::prose::plugin::{documents, edits};
use crate::{Block, NewBlock, Op, Position, Selection, Span, Transaction, types};

pub use edits::{EditError, InvalidOp};

/// How many blocks [`Document::blocks`] asks for at a time.
const PAGE: u32 = 256;

/// A document the plugin can read: the one a command was run on.
pub struct Document {
    raw: documents::Document,
}

pub(crate) fn current() -> Option<Document> {
    documents::current().map(|raw| Document { raw })
}

/// A fresh id for a block or annotation the plugin is about to create.
pub fn new_id() -> String {
    documents::new_id()
}

impl Document {
    /// The document the running command was run on. The same as
    /// [`Context::document`](crate::Context::document).
    pub fn current() -> Option<Document> {
        current()
    }

    /// The handle the generated bindings use, for calls this type does not
    /// wrap.
    pub fn raw(&self) -> &documents::Document {
        &self.raw
    }

    /// Its binder id.
    pub fn id(&self) -> String {
        self.raw.id()
    }

    /// Its title in the binder.
    pub fn title(&self) -> String {
        self.raw.title()
    }

    /// Goes up with every change.
    pub fn version(&self) -> u64 {
        self.raw.version()
    }

    /// How many blocks it has.
    pub fn block_count(&self) -> u32 {
        self.raw.block_count()
    }

    /// Its words, counted as Prose's status bar counts them.
    pub fn word_count(&self) -> u32 {
        self.raw.word_count()
    }

    /// One block by id.
    pub fn block(&self, id: &str) -> Option<Block> {
        self.raw.block(id)
    }

    /// The index of a block.
    pub fn block_index(&self, id: &str) -> Option<u32> {
        self.raw.block_index(id)
    }

    /// Every block, read from Prose a page at a time as you iterate.
    pub fn blocks(&self) -> Blocks<'_> {
        self.blocks_from(0)
    }

    /// The blocks from block number `start` on.
    pub fn blocks_from(&self, start: u32) -> Blocks<'_> {
        Blocks {
            doc: self,
            next: start,
            page: Vec::new().into_iter(),
            done: false,
        }
    }

    /// The selection, if the document is in the active pane.
    pub fn selection(&self) -> Option<Selection> {
        self.raw.selection()
    }

    /// The selected span, start first, or `None` for no selection or a bare
    /// caret.
    pub fn selection_span(&self) -> Option<Span> {
        self.selection()
            .filter(|s| !s.is_empty())
            .map(|s| s.span_in(self))
    }

    /// `a` and `b` as a span, whichever comes first in the document first.
    pub fn order(&self, a: Position, b: Position) -> Span {
        let index = |p: &Position| self.block_index(&p.block).unwrap_or(0);
        if (index(&a), a.offset) <= (index(&b), b.offset) {
            Span::new(a, b)
        } else {
            Span::new(b, a)
        }
    }

    /// Starts an edit against the document as it is now.
    pub fn edit(&self) -> Edit<'_> {
        Edit {
            doc: self,
            base_version: self.version(),
            ops: Vec::new(),
        }
    }

    /// Builds an edit with `build` and applies it, building it again from a
    /// fresh read if the document changed in between, up to three times.
    /// `build` may return no ops to make no change.
    pub fn edit_with_retry(
        &self,
        mut build: impl FnMut(&Document, &mut Edit<'_>),
    ) -> Result<u64, EditError> {
        let mut tries = 0;
        loop {
            let mut edit = self.edit();
            build(self, &mut edit);
            match edit.apply() {
                Err(EditError::Stale) if tries < 2 => tries += 1,
                result => return result,
            }
        }
    }
}

/// An iterator over a document's blocks. See [`Document::blocks`].
pub struct Blocks<'a> {
    doc: &'a Document,
    next: u32,
    page: std::vec::IntoIter<Block>,
    done: bool,
}

impl Iterator for Blocks<'_> {
    type Item = Block;

    fn next(&mut self) -> Option<Block> {
        if let Some(block) = self.page.next() {
            return Some(block);
        }
        if self.done {
            return None;
        }
        let page = self.doc.raw.blocks(self.next, PAGE);
        self.done = (page.len() as u32) < PAGE;
        self.next += page.len() as u32;
        self.page = page.into_iter();
        self.page.next()
    }
}

/// Ops to apply to a document as one undo step. Needs the plugin's
/// `edit-documents` permission. Ops apply in order, each to the document as
/// the ones before it left it, so when making several changes in one block,
/// go from its end to its start.
pub struct Edit<'a> {
    doc: &'a Document,
    base_version: u64,
    ops: Vec<Op>,
}

impl Edit<'_> {
    /// Adds any op.
    pub fn push(&mut self, op: Op) -> &mut Self {
        self.ops.push(op);
        self
    }

    /// Replaces `span` with `text`, where `\n` starts a new paragraph.
    pub fn replace_text(&mut self, span: Span, text: impl Into<String>) -> &mut Self {
        self.push(Op::ReplaceText(types::ReplaceText {
            span,
            text: text.into(),
        }))
    }

    /// Inserts `text` at `at`.
    pub fn insert_text(&mut self, at: Position, text: impl Into<String>) -> &mut Self {
        self.replace_text(Span::new(at.clone(), at), text)
    }

    /// Replaces byte range `start..end` of one block, keeping the format
    /// around it.
    pub fn replace(
        &mut self,
        block: &str,
        start: u32,
        end: u32,
        text: impl Into<String>,
    ) -> &mut Self {
        self.push(Op::Replace(types::Replace {
            block: block.into(),
            start,
            end,
            text: text.into(),
            runs: None,
        }))
    }

    /// Turns `marks` on or off over a byte range of one block.
    pub fn set_marks(
        &mut self,
        block: &str,
        start: u32,
        end: u32,
        marks: crate::Marks,
        on: bool,
    ) -> &mut Self {
        self.push(Op::SetMarks(types::SetMarks {
            block: block.into(),
            start,
            end,
            marks,
            on,
        }))
    }

    /// Inserts blocks after `after`, or first in the document.
    pub fn insert_blocks(&mut self, after: Option<&str>, blocks: Vec<NewBlock>) -> &mut Self {
        self.push(Op::InsertBlocks(types::InsertBlocks {
            after: after.map(Into::into),
            blocks,
        }))
    }

    /// Whether no ops were added.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// The ops so far.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Applies the ops as one undo step and returns the new version. An
    /// empty edit changes nothing and returns the current version.
    pub fn apply(self) -> Result<u64, EditError> {
        if self.ops.is_empty() {
            return Ok(self.base_version);
        }
        edits::apply(
            &self.doc.raw,
            &Transaction {
                base_version: self.base_version,
                ops: self.ops,
            },
        )
    }
}
