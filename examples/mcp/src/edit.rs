//! `edit_document`: the assistant's changes, by paragraph number, turned
//! into one transaction, so the writer undoes them with one `u`.
//!
//! Ops apply in order, each to the document as the ones before left it, so
//! text replacements inside a block go from its end to its start, then
//! whole paragraphs, kinds and insertions, and removals last. Changes that
//! would step on each other (two replacements overlapping, an edit to a
//! paragraph that's also removed) are refused rather than guessed at.

use std::collections::BTreeMap;

use prose_plugin::{Block, Document, NewBlock, Op, Span, types};
use serde_json::{Map, Value};

use crate::text;

/// One change, as the tool's `edits` list gives it.
#[derive(Debug, PartialEq)]
pub enum Change {
    /// Replace `find` with `replace` in paragraph `paragraph`, or anywhere
    /// with `None`: exactly once, or every time with `all`.
    Find {
        paragraph: Option<u32>,
        find: String,
        replace: String,
        all: bool,
    },
    /// Replace a paragraph's text. `\n` starts new paragraphs.
    Text { paragraph: u32, text: String },
    /// Make a paragraph a heading, a quote, ...
    Kind {
        paragraph: u32,
        kind: prose_plugin::BlockKind,
    },
    /// Insert paragraphs after paragraph `after`, or first with `None`.
    Insert {
        after: Option<u32>,
        paragraphs: Vec<String>,
        kind: prose_plugin::BlockKind,
    },
    /// Remove `count` paragraphs from `first`.
    Remove { first: u32, count: u32 },
}

fn number(object: &Map<String, Value>, key: &str) -> Result<Option<u32>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("{key} must be a paragraph number, from 0")),
    }
}

fn string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{key} must be text")),
    }
}

impl Change {
    /// Read one entry of `edits`.
    pub fn parse(value: &Value) -> Result<Change, String> {
        let Value::Object(o) = value else {
            return Err("each edit must be an object".into());
        };
        let kind = |o: &Map<String, Value>| -> Result<Option<prose_plugin::BlockKind>, String> {
            match string(o, "kind")? {
                None => Ok(None),
                Some(name) => text::parse_kind(&name).map(Some).ok_or_else(|| {
                    format!("kind {name:?} must be one of {}", text::KINDS.join(", "))
                }),
            }
        };
        if let Some(find) = string(o, "find")? {
            if find.is_empty() {
                return Err("find must not be empty".into());
            }
            let replace = string(o, "replace")?.ok_or("find needs replace (\"\" deletes)")?;
            return Ok(Change::Find {
                paragraph: number(o, "paragraph")?,
                find,
                replace,
                all: o.get("all").and_then(Value::as_bool).unwrap_or(false),
            });
        }
        if o.contains_key("insert_after") {
            let after = match o.get("insert_after") {
                Some(Value::Number(n)) if n.as_i64() == Some(-1) => None,
                _ => Some(number(o, "insert_after")?.ok_or("insert_after must be a number")?),
            };
            let paragraphs: Vec<String> = match o.get("paragraphs") {
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|p| p.as_str().map(str::to_owned))
                    .collect::<Option<_>>()
                    .ok_or("paragraphs must be a list of text")?,
                Some(Value::String(s)) => vec![s.clone()],
                _ => return Err("insert_after needs paragraphs, a list of text".into()),
            };
            if paragraphs.is_empty() {
                return Err("paragraphs is empty".into());
            }
            return Ok(Change::Insert {
                after,
                paragraphs,
                kind: kind(o)?.unwrap_or(prose_plugin::BlockKind::Paragraph),
            });
        }
        if let Some(first) = number(o, "remove")? {
            let count = number(o, "count")?.unwrap_or(1);
            if count == 0 {
                return Err("count must be 1 or more".into());
            }
            return Ok(Change::Remove { first, count });
        }
        let paragraph = number(o, "paragraph")?
            .ok_or("each edit needs find, insert_after, remove, or paragraph with text or kind")?;
        match (string(o, "text")?, kind(o)?) {
            (Some(text), None) => Ok(Change::Text { paragraph, text }),
            (None, Some(kind)) => Ok(Change::Kind { paragraph, kind }),
            (Some(_), Some(_)) => Err("give text or kind for a paragraph, in two edits".into()),
            (None, None) => Err(format!("paragraph {paragraph} needs text or kind")),
        }
    }
}

/// What a set of changes did, for the reply.
#[derive(Debug, Default, PartialEq)]
pub struct Summary {
    pub replaced: usize,
    pub rewritten: usize,
    pub restyled: usize,
    pub inserted: usize,
    pub removed: usize,
}

impl Summary {
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        let mut say = |n: usize, one: &str, many: &str| {
            if n == 1 {
                parts.push(one.to_owned());
            } else if n > 1 {
                parts.push(format!("{n} {many}"));
            }
        };
        say(self.replaced, "1 replacement", "replacements");
        say(
            self.rewritten,
            "1 paragraph rewritten",
            "paragraphs rewritten",
        );
        say(self.restyled, "1 paragraph restyled", "paragraphs restyled");
        say(self.inserted, "1 paragraph inserted", "paragraphs inserted");
        say(self.removed, "1 paragraph removed", "paragraphs removed");
        if parts.is_empty() {
            "nothing changed".into()
        } else {
            parts.join(", ")
        }
    }
}

/// The ops for `changes`, read against `blocks` (the document's, in
/// order), in the order to apply them. New paragraphs get ids from `mint`.
pub fn build(
    blocks: &[Block],
    changes: &[Change],
    mut mint: impl FnMut() -> String,
) -> Result<(Vec<Op>, Summary), String> {
    let block = |n: u32| -> Result<&Block, String> {
        blocks.get(n as usize).ok_or_else(|| {
            format!(
                "there is no paragraph {n}: the document has {} (0 to {})",
                blocks.len(),
                blocks.len().saturating_sub(1)
            )
        })
    };
    let mut summary = Summary::default();
    // Byte ranges to replace, by paragraph.
    let mut ranges: BTreeMap<u32, Vec<(usize, usize, String)>> = BTreeMap::new();
    let mut rewritten: BTreeMap<u32, String> = BTreeMap::new();
    let mut kinds = Vec::new();
    let mut inserts = Vec::new();
    let mut removed: Vec<(u32, u32)> = Vec::new();
    for change in changes {
        match change {
            Change::Find {
                paragraph,
                find,
                replace,
                all,
            } => {
                let search: Vec<u32> = match paragraph {
                    Some(n) => {
                        block(*n)?;
                        vec![*n]
                    }
                    None => (0..blocks.len() as u32).collect(),
                };
                let mut found = Vec::new();
                for n in search {
                    for (at, _) in blocks[n as usize].text.match_indices(find.as_str()) {
                        found.push((n, at));
                    }
                }
                let place =
                    paragraph.map_or("the document".to_owned(), |n| format!("paragraph {n}"));
                match found.len() {
                    0 => return Err(format!("{find:?} isn't in {place}")),
                    1 => {}
                    n if !all => {
                        let at: Vec<String> = found.iter().map(|(p, _)| p.to_string()).collect();
                        return Err(format!(
                            "{find:?} is in {place} {n} times (paragraphs {}): give a paragraph, \
                             more of the text around it, or all: true",
                            at.join(", ")
                        ));
                    }
                    _ => {}
                }
                for (n, at) in found {
                    ranges
                        .entry(n)
                        .or_default()
                        .push((at, at + find.len(), replace.clone()));
                    summary.replaced += 1;
                }
            }
            Change::Text { paragraph, text } => {
                block(*paragraph)?;
                if rewritten.insert(*paragraph, text.clone()).is_some() {
                    return Err(format!("paragraph {paragraph} is rewritten twice"));
                }
                summary.rewritten += 1;
            }
            Change::Kind { paragraph, kind } => {
                block(*paragraph)?;
                kinds.push((*paragraph, *kind));
                summary.restyled += 1;
            }
            Change::Insert {
                after,
                paragraphs,
                kind,
            } => {
                if let Some(n) = after {
                    block(*n)?;
                }
                inserts.push((*after, paragraphs.clone(), *kind));
                summary.inserted += paragraphs.len();
            }
            Change::Remove { first, count } => {
                let last = first + count - 1;
                block(*first)?;
                block(last)?;
                removed.push((*first, last));
                summary.removed += *count as usize;
            }
        }
    }
    let is_removed = |n: u32| removed.iter().any(|(a, b)| (*a..=*b).contains(&n));
    for (n, list) in &mut ranges {
        if rewritten.contains_key(n) {
            return Err(format!(
                "paragraph {n} is both rewritten and edited: do one or the other"
            ));
        }
        if is_removed(*n) {
            return Err(format!("paragraph {n} is both removed and edited"));
        }
        list.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        if list.windows(2).any(|w| w[1].1 > w[0].0) {
            return Err(format!("two replacements overlap in paragraph {n}"));
        }
    }
    for n in rewritten.keys().chain(kinds.iter().map(|(n, _)| n)) {
        if is_removed(*n) {
            return Err(format!("paragraph {n} is both removed and changed"));
        }
    }
    for (after, _, _) in &inserts {
        if after.is_some_and(is_removed) {
            return Err(format!(
                "paragraphs are inserted after paragraph {}, which is removed",
                after.unwrap_or_default()
            ));
        }
    }
    let mut sorted = removed.clone();
    sorted.sort();
    if sorted.windows(2).any(|w| w[1].0 <= w[0].1) {
        return Err("two removals overlap".into());
    }
    if !sorted.is_empty()
        && sorted.iter().map(|(a, b)| b - a + 1).sum::<u32>() as usize == blocks.len()
    {
        return Err("that would remove every paragraph; rewrite one instead".into());
    }

    let mut ops = Vec::new();
    for (n, list) in &ranges {
        let id = &blocks[*n as usize].id;
        for (start, end, text) in list {
            ops.push(Op::Replace(types::Replace {
                block: id.clone(),
                start: *start as u32,
                end: *end as u32,
                text: text.clone(),
                runs: None,
            }));
        }
    }
    for (n, text) in &rewritten {
        ops.push(Op::ReplaceText(types::ReplaceText {
            span: Span::whole_block(&blocks[*n as usize]),
            text: text.clone(),
        }));
    }
    for (n, kind) in kinds {
        ops.push(Op::SetBlockKind(types::SetBlockKind {
            block: blocks[n as usize].id.clone(),
            kind,
        }));
    }
    for (after, paragraphs, kind) in inserts {
        let new: Vec<NewBlock> = paragraphs
            .into_iter()
            .map(|text| NewBlock {
                id: mint(),
                kind,
                text: if kind == prose_plugin::BlockKind::SceneBreak {
                    String::new()
                } else {
                    text
                },
                runs: Vec::new(),
            })
            .collect();
        ops.push(Op::InsertBlocks(types::InsertBlocks {
            after: after.map(|n| blocks[n as usize].id.clone()),
            blocks: new,
        }));
    }
    for (first, last) in removed {
        ops.push(Op::RemoveBlocks(types::RemoveBlocks {
            first: blocks[first as usize].id.clone(),
            count: last - first + 1,
        }));
    }
    Ok((ops, summary))
}

/// Every block of `doc`, in order.
pub fn blocks(doc: &Document) -> Vec<Block> {
    doc.blocks().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prose_plugin::{BlockKind, Marks, Run};
    use serde_json::json;

    fn doc(texts: &[&str]) -> Vec<Block> {
        texts
            .iter()
            .enumerate()
            .map(|(n, text)| Block {
                id: format!("b{n}"),
                kind: BlockKind::Paragraph,
                text: (*text).into(),
                runs: vec![Run {
                    len: text.len() as u32,
                    marks: Marks::empty(),
                    style: None,
                }],
                annotations: Vec::new(),
            })
            .collect()
    }

    fn changes(edits: serde_json::Value) -> Vec<Change> {
        edits
            .as_array()
            .unwrap()
            .iter()
            .map(|e| Change::parse(e).unwrap())
            .collect()
    }

    fn ops(blocks: &[Block], edits: serde_json::Value) -> Result<Vec<Op>, String> {
        let mut n = 0;
        build(blocks, &changes(edits), || {
            n += 1;
            format!("new{n}")
        })
        .map(|(ops, _)| ops)
    }

    #[test]
    fn replacements_in_one_paragraph_go_from_its_end() {
        let blocks = doc(&["the cat and the dog", "a bird"]);
        let ops = ops(
            &blocks,
            json!([{"find": "the", "replace": "a", "paragraph": 0, "all": true},
                   {"find": "bird", "replace": "fish"}]),
        )
        .unwrap();
        let starts: Vec<(String, u32)> = ops
            .iter()
            .map(|op| match op {
                Op::Replace(r) => (r.block.clone(), r.start),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            starts,
            [("b0".into(), 12), ("b0".into(), 0), ("b1".into(), 2)]
        );
    }

    #[test]
    fn an_ambiguous_find_says_where() {
        let blocks = doc(&["the cat", "the dog"]);
        let error = ops(&blocks, json!([{"find": "the", "replace": "a"}])).unwrap_err();
        assert!(error.contains("2 times (paragraphs 0, 1)"), "{error}");
        let error = ops(&blocks, json!([{"find": "owl", "replace": "a"}])).unwrap_err();
        assert!(error.contains("isn't in the document"), "{error}");
    }

    #[test]
    fn inserts_and_removes_name_blocks_by_id() {
        let blocks = doc(&["one", "two", "three"]);
        let ops = ops(
            &blocks,
            json!([{"insert_after": -1, "paragraphs": ["Title"], "kind": "heading-1"},
                   {"insert_after": 0, "paragraphs": ["a", "b"]},
                   {"remove": 1, "count": 2}]),
        )
        .unwrap();
        match &ops[0] {
            Op::InsertBlocks(i) => {
                assert_eq!(i.after, None);
                assert_eq!(i.blocks[0].kind, BlockKind::Heading(1));
            }
            other => panic!("{other:?}"),
        }
        match &ops[1] {
            Op::InsertBlocks(i) => {
                assert_eq!(i.after.as_deref(), Some("b0"));
                assert_eq!(i.blocks.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        match &ops[2] {
            Op::RemoveBlocks(r) => assert_eq!((r.first.as_str(), r.count), ("b1", 2)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn conflicting_edits_are_refused() {
        let blocks = doc(&["one", "two"]);
        for edits in [
            json!([{"paragraph": 0, "text": "x"}, {"find": "one", "replace": "y"}]),
            json!([{"remove": 0}, {"paragraph": 0, "kind": "quote"}]),
            json!([{"remove": 0, "count": 2}]),
            json!([{"insert_after": 1, "paragraphs": ["x"]}, {"remove": 1}]),
            json!([{"paragraph": 5, "text": "x"}]),
        ] {
            assert!(ops(&blocks, edits.clone()).is_err(), "{edits}");
        }
        assert!(Change::parse(&json!({"paragraph": 0})).is_err());
        assert!(Change::parse(&json!({"find": "a"})).is_err());
    }
}
