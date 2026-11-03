//! WordFreq: `:WordFreq [n]` lists the n most used words in the document,
//! and `:WordFreqFix` fixes doubled words ("the the") in it.
//!
//! Shows how to register commands, read a document a page of blocks at a
//! time, read settings, show a result, and edit: `:WordFreqFix` builds its
//! deletions against the version it read and applies them as one undo step,
//! so `u` puts every doubled word back at once. Because it edits, its
//! `plugin.toml` asks for `edit-documents`.

use std::collections::HashMap;

use prose_plugin::{Argument, Block, Command, Context, Document, Edit, Plugin, settings, ui};

#[derive(Default)]
struct WordFreq;

impl Plugin for WordFreq {
    fn activate(&mut self) -> prose_plugin::Result {
        Command::new("top_words", "Top words")
            .description("List the most used words in the document")
            .ex_name("WordFreq")
            .argument(Argument::Text)
            .register()?;
        Command::new("fix_doubled", "Fix doubled words")
            .description("Delete the second of two words repeated in a row, like \"the the\"")
            .ex_name("WordFreqFix")
            .register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, args: Vec<String>, cx: Context) -> prose_plugin::Result {
        match name {
            "top_words" => top_words(args, cx),
            "fix_doubled" => fix_doubled(cx),
            _ => Err(format!("WordFreq has no command `{name}`")),
        }
    }
}

fn top_words(args: Vec<String>, cx: Context) -> prose_plugin::Result {
    let doc = cx.document().ok_or("Open a document first")?;
    let count = match args.first().map(|a| a.trim()).filter(|a| !a.is_empty()) {
        Some(arg) => arg
            .parse::<usize>()
            .map_err(|_| format!("`{arg}` is not a number"))?,
        None => cx
            .count
            .map(|n| n as usize)
            .or_else(|| settings::integer("count").map(|n| n as usize))
            .unwrap_or(10),
    };
    let min_length = settings::integer("min_length").unwrap_or(3).max(1) as usize;
    let ignore = lowercase_list("ignore");

    let mut counts: HashMap<String, u32> = HashMap::new();
    for block in doc.blocks() {
        for (_, word) in words(&block.text) {
            let word = word.to_lowercase();
            if word.chars().count() >= min_length && !ignore.contains(&word) {
                *counts.entry(word).or_default() += 1;
            }
        }
    }

    let top = rank(counts, count);
    if top.is_empty() {
        ui::info(&format!("{} has no words to count", doc.title()));
        return Ok(());
    }
    let list: Vec<String> = top.iter().map(|(w, n)| format!("{w} {n}")).collect();
    ui::info(&format!("{}: {}", doc.title(), list.join(", ")));
    Ok(())
}

fn fix_doubled(cx: Context) -> prose_plugin::Result {
    let doc = cx.document().ok_or("Open a document first")?;
    let allowed = lowercase_list("allow_doubled");
    let mut fixed = 0;
    doc.edit_with_retry(|doc, edit| fixed = build_fixes(doc, edit, &allowed))
        .map_err(|e| e.to_string())?;
    match fixed {
        0 => ui::info("No doubled words"),
        1 => ui::info("Fixed 1 doubled word"),
        n => ui::info(&format!("Fixed {n} doubled words")),
    }
    Ok(())
}

/// Adds a deletion for every doubled word in `doc` to `edit` and returns how
/// many.
fn build_fixes(doc: &Document, edit: &mut Edit<'_>, allowed: &[String]) -> usize {
    doc.blocks()
        .map(|block| fix_block(&block, edit, allowed))
        .sum()
}

/// Ops apply in order, so delete from the end of the block back: a deletion
/// then never moves the offsets of the ones still to come.
fn fix_block(block: &Block, edit: &mut Edit<'_>, allowed: &[String]) -> usize {
    let doubled = doubled(&block.text, allowed);
    for range in doubled.iter().rev() {
        edit.replace(&block.id, range.start as u32, range.end as u32, "");
    }
    doubled.len()
}

/// The byte ranges to delete from `text` to undo its doubled words: the
/// space and the second word of each pair of the same word (ignoring case)
/// with only spaces between. Punctuation between them ("Well, well") keeps
/// both, and so do words in `allowed` ("had had", "that that"), which a
/// writer often means. Three in a row lose two.
fn doubled(text: &str, allowed: &[String]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    // The word a repeat would double, and where the text after it starts
    // (after the last word deleted, when it already doubled).
    let mut last: Option<(&str, usize)> = None;
    for (at, word) in words(text) {
        let end = at + word.len();
        if let Some((before, from)) = last {
            let gap = &text[from..at];
            if !gap.is_empty()
                && gap.chars().all(char::is_whitespace)
                && before.to_lowercase() == word.to_lowercase()
                && !allowed.contains(&word.to_lowercase())
            {
                out.push(from..end);
                last = Some((before, end));
                continue;
            }
        }
        last = Some((word, end));
    }
    out
}

/// A setting's text list, lowercased.
fn lowercase_list(key: &str) -> Vec<String> {
    settings::text_list(key)
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect()
}

/// The words of `text` and their byte offsets: letters, digits and
/// apostrophes inside a word.
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let is_quote = |c: char| c == '\'' || c == '\u{2019}';
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        let inside = c.is_alphanumeric() || is_quote(c);
        match (start, inside) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                let word = &text[s..i];
                let trimmed = word.trim_start_matches(is_quote);
                let at = s + (word.len() - trimmed.len());
                let trimmed = trimmed.trim_end_matches(is_quote);
                if !trimmed.is_empty() {
                    words.push((at, trimmed));
                }
                start = None;
            }
            _ => {}
        }
    }
    words.into_iter()
}

/// The `n` most counted, most first, ties alphabetically.
fn rank(counts: HashMap<String, u32>, n: usize) -> Vec<(String, u32)> {
    let mut all: Vec<_> = counts.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    all.truncate(n);
    all
}

prose_plugin::export!(WordFreq);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_and_keeps_apostrophes() {
        let w: Vec<_> = words("Don't stop—'twas the night's end.")
            .map(|(_, w)| w)
            .collect();
        assert_eq!(w, ["Don't", "stop", "twas", "the", "night's", "end"]);
        let at: Vec<_> = words("a 'bc' d").map(|(at, _)| at).collect();
        assert_eq!(at, [0, 3, 7]);
    }

    #[test]
    fn ranks_by_count_then_word() {
        let counts = HashMap::from([("b".into(), 2), ("a".into(), 2), ("c".into(), 5)]);
        assert_eq!(
            rank(counts, 2),
            [("c".to_string(), 5), ("a".to_string(), 2)]
        );
    }

    fn fixed(text: &str, allowed: &[&str]) -> String {
        let allowed: Vec<String> = allowed.iter().map(|w| w.to_string()).collect();
        let mut out = text.to_string();
        for range in doubled(text, &allowed).into_iter().rev() {
            out.replace_range(range, "");
        }
        out
    }

    #[test]
    fn deletes_the_second_of_a_doubled_word() {
        assert_eq!(fixed("The the cat sat.", &[]), "The cat sat.");
        assert_eq!(fixed("on the\u{a0}the mat", &[]), "on the mat");
        assert_eq!(fixed("a a a b", &[]), "a b");
        assert_eq!(fixed("end end.", &[]), "end.");
    }

    #[test]
    fn keeps_punctuated_allowed_and_different_words() {
        assert_eq!(fixed("Well, well.", &[]), "Well, well.");
        assert_eq!(fixed("She had had enough.", &["had"]), "She had had enough.");
        assert_eq!(fixed("the then", &[]), "the then");
        assert_eq!(fixed("can't can't", &[]), "can't");
    }
}
