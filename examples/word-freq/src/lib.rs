//! WordFreq: `:WordFreq [n]` lists the n most used words in the document,
//! `:WordFreqMark [n]` highlights every use of them, and `:WordFreqClear`
//! takes the highlights away.
//!
//! Shows how to register a command, read a document a page of blocks at a
//! time, read settings, show the result and decorate the text. It never
//! edits, so its `plugin.toml` asks for no permissions.

use std::collections::HashMap;

use prose_plugin::decorations::{Color, Decorations};
use prose_plugin::{Argument, Command, Context, Document, Plugin, Position, Span, settings, ui};

#[derive(Default)]
struct WordFreq;

impl Plugin for WordFreq {
    fn activate(&mut self) -> prose_plugin::Result {
        Command::new("top_words", "Top words")
            .description("List the most used words in the document")
            .ex_name("WordFreq")
            .argument(Argument::Text)
            .register()?;
        Command::new("mark_top", "Mark top words")
            .description("Highlight every use of the most used words")
            .ex_name("WordFreqMark")
            .argument(Argument::Text)
            .register()?;
        Command::new("clear_marks", "Clear word marks")
            .description("Take away WordFreq's highlights")
            .ex_name("WordFreqClear")
            .register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, args: Vec<String>, cx: Context) -> prose_plugin::Result {
        match name {
            "top_words" => top_words(args, cx),
            "mark_top" => mark_top(args, cx),
            "clear_marks" => {
                cx.document()
                    .ok_or("Open a document first")?
                    .clear_decorations();
                Ok(())
            }
            _ => Err(format!("WordFreq has no command `{name}`")),
        }
    }
}

/// How many words to take: the argument, the vim count, or `fallback`.
fn how_many(args: &[String], cx: &Context, fallback: usize) -> prose_plugin::Result<usize> {
    Ok(
        match args.first().map(|a| a.trim()).filter(|a| !a.is_empty()) {
            Some(arg) => arg
                .parse::<usize>()
                .map_err(|_| format!("`{arg}` is not a number"))?,
            None => cx.count.map(|n| n as usize).unwrap_or(fallback),
        },
    )
}

/// How many times each word that counts is used in `doc`.
fn count_words(doc: &Document) -> HashMap<String, u32> {
    let min_length = settings::integer("min_length").unwrap_or(3).max(1) as usize;
    let ignore: Vec<String> = settings::text_list("ignore")
        .unwrap_or_default()
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect();

    let mut counts: HashMap<String, u32> = HashMap::new();
    for block in doc.blocks() {
        for word in words(&block.text) {
            let word = word.to_lowercase();
            if word.chars().count() >= min_length && !ignore.contains(&word) {
                *counts.entry(word).or_default() += 1;
            }
        }
    }
    counts
}

fn top_words(args: Vec<String>, cx: Context) -> prose_plugin::Result {
    let doc = cx.document().ok_or("Open a document first")?;
    let fallback = settings::integer("count").map_or(10, |n| n as usize);
    let count = how_many(&args, &cx, fallback)?;
    let top = rank(count_words(&doc), count);
    if top.is_empty() {
        ui::info(&format!("{} has no words to count", doc.title()));
        return Ok(());
    }
    let list: Vec<String> = top.iter().map(|(w, n)| format!("{w} {n}")).collect();
    ui::info(&format!("{}: {}", doc.title(), list.join(", ")));
    Ok(())
}

fn mark_top(args: Vec<String>, cx: Context) -> prose_plugin::Result {
    let doc = cx.document().ok_or("Open a document first")?;
    let count = how_many(&args, &cx, 1)?;
    let top: Vec<String> = rank(count_words(&doc), count)
        .into_iter()
        .map(|(word, _)| word)
        .collect();
    let mut marks = Decorations::new();
    for block in doc.blocks() {
        let mut found = false;
        for (at, word) in word_offsets(&block.text) {
            if top.contains(&word.to_lowercase()) {
                let end = (at + word.len()) as u32;
                marks.highlight(Span::in_block(&block.id, at as u32, end), Color::Yellow);
                found = true;
            }
        }
        if found {
            marks.margin_mark(Position::new(&block.id, 0), Color::Muted, "•");
        }
    }
    marks.show(&doc).map_err(|e| e.to_string())?;
    ui::info(&format!("Marked {}", top.join(", ")));
    Ok(())
}

/// Each word of `text` (as [`words`] finds them) with its byte offset.
fn word_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let base = text.as_ptr() as usize;
    words(text).map(move |w| (w.as_ptr() as usize - base, w))
}

/// The words of `text`: letters, digits and apostrophes inside a word.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}'))
        .map(|w| w.trim_matches(|c| c == '\'' || c == '\u{2019}'))
        .filter(|w| !w.is_empty())
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
        let w: Vec<_> = words("Don't stop—'twas the night's end.").collect();
        assert_eq!(w, ["Don't", "stop", "twas", "the", "night's", "end"]);
    }

    #[test]
    fn ranks_by_count_then_word() {
        let counts = HashMap::from([("b".into(), 2), ("a".into(), 2), ("c".into(), 5)]);
        assert_eq!(
            rank(counts, 2),
            [("c".to_string(), 5), ("a".to_string(), 2)]
        );
    }
}
