//! WordFreq: `:WordFreq [n]` lists the n most used words in the document.
//!
//! Shows how to register a command, read a document a page of blocks at a
//! time, read settings and show the result. It never edits, so its
//! `plugin.toml` asks for no permissions.

use std::collections::HashMap;

use prose_plugin::{Argument, Command, Context, Plugin, settings, ui};

#[derive(Default)]
struct WordFreq;

impl Plugin for WordFreq {
    fn activate(&mut self) -> prose_plugin::Result {
        Command::new("top_words", "Top words")
            .description("List the most used words in the document")
            .ex_name("WordFreq")
            .argument(Argument::Text)
            .register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, args: Vec<String>, cx: Context) -> prose_plugin::Result {
        match name {
            "top_words" => top_words(args, cx),
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

    let top = rank(counts, count);
    if top.is_empty() {
        ui::info(&format!("{} has no words to count", doc.title()));
        return Ok(());
    }
    let list: Vec<String> = top.iter().map(|(w, n)| format!("{w} {n}")).collect();
    ui::info(&format!("{}: {}", doc.title(), list.join(", ")));
    Ok(())
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
