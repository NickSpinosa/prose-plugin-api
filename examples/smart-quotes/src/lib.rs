//! Smart Quotes: `:SmartQuotes` turns straight quotes into curly ones in the
//! selection, or the whole document when nothing is selected.
//!
//! Shows how to edit: build ops against the version you read, apply them as
//! one undo step, and retry if the writer typed in between. Its
//! `plugin.toml` asks for `edit-documents`.
//!
//! It also curls the quotes of every compile, as a compile transform: the
//! same ops, built against the manuscript Compile assembled and handed back
//! instead of applied. For that it asks for `compile` and sets
//! `compile-transform = true`.

use prose_plugin::compile::{CompileInfo, Note};
use prose_plugin::{Block, Command, Compile, Context, Document, Edit, Op, Plugin, ui};

#[derive(Default)]
struct SmartQuotes;

impl Plugin for SmartQuotes {
    fn activate(&mut self) -> prose_plugin::Result {
        Command::new("curl", "Curl quotes")
            .description("Turn straight quotes into curly ones")
            .ex_name("SmartQuotes")
            .register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, _args: Vec<String>, cx: Context) -> prose_plugin::Result {
        match name {
            "curl" => curl(cx),
            _ => Err(format!("Smart Quotes has no command `{name}`")),
        }
    }
}

fn curl(cx: Context) -> prose_plugin::Result {
    let doc = cx.document().ok_or("Open a document first")?;
    let mut changed = 0;
    doc.edit_with_retry(|doc, edit| changed = build(doc, edit))
        .map_err(|e| e.to_string())?;
    match changed {
        0 => ui::info("No straight quotes to curl"),
        1 => ui::info("Curled 1 quote"),
        n => ui::info(&format!("Curled {n} quotes")),
    }
    Ok(())
}

/// Adds a replace for every straight quote in the selection (or the whole
/// document) to `edit` and returns how many.
fn build(doc: &Document, edit: &mut Edit<'_>) -> usize {
    let span = doc.selection_span();
    let (first, last) = match &span {
        Some(span) => (
            doc.block_index(&span.start.block).unwrap_or(0),
            doc.block_index(&span.end.block).unwrap_or(u32::MAX),
        ),
        None => (0, u32::MAX),
    };
    let mut changed = 0;
    for (i, block) in doc.blocks_from(first).enumerate() {
        let index = first + i as u32;
        if index > last {
            break;
        }
        let start = match &span {
            Some(s) if s.start.block == block.id => s.start.offset as usize,
            _ => 0,
        };
        let end = match &span {
            Some(s) if s.end.block == block.id => s.end.offset as usize,
            _ => block.text.len(),
        };
        changed += curl_block(&block, start, end, edit);
    }
    changed
}

/// Ops apply in order, so go from the end of the block back: a replacement
/// then never moves the offsets of the ones still to come.
fn curl_block(block: &Block, start: usize, end: usize, edit: &mut Edit<'_>) -> usize {
    let quotes = curly(&block.text, start, end);
    for &(at, curly) in quotes.iter().rev() {
        edit.replace(&block.id, at as u32, at as u32 + 1, curly);
    }
    quotes.len()
}

/// The byte offset of each straight quote in `text[start..end]` and the
/// curly quote for it: opening after a space, an opening bracket or dash, or
/// at the start; closing otherwise (so `don't` takes an apostrophe).
fn curly(text: &str, start: usize, end: usize) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    let mut before: Option<char> = text[..start].chars().next_back();
    for (i, c) in text[start..end].char_indices() {
        let opens = match before {
            None => true,
            Some(b) => b.is_whitespace() || "([{\u{2014}\u{2013}-".contains(b),
        };
        match c {
            '"' => out.push((start + i, if opens { "\u{201C}" } else { "\u{201D}" })),
            '\'' => out.push((start + i, if opens { "\u{2018}" } else { "\u{2019}" })),
            _ => {}
        }
        before = Some(c);
    }
    out
}

impl Compile for SmartQuotes {
    fn transform(
        &mut self,
        _info: CompileInfo,
        manuscript: Document,
        _notes: Vec<Note>,
    ) -> prose_plugin::Result<Vec<Op>> {
        let mut edit = manuscript.edit();
        build(&manuscript, &mut edit);
        Ok(edit.ops().to_vec())
    }
}

prose_plugin::export!(SmartQuotes, compile);

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str) -> String {
        let mut s = text.to_string();
        for (at, q) in curly(text, 0, text.len()).into_iter().rev() {
            s.replace_range(at..at + 1, q);
        }
        s
    }

    #[test]
    fn curls_open_close_and_apostrophes() {
        assert_eq!(
            apply(r#""Don't," she said. 'Fine.'"#),
            "\u{201C}Don\u{2019}t,\u{201D} she said. \u{2018}Fine.\u{2019}"
        );
    }

    #[test]
    fn respects_the_range() {
        let text = r#"a "b" "c""#;
        assert_eq!(curly(text, 6, text.len()).len(), 2);
    }
}
