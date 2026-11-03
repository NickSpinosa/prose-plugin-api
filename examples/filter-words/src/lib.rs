//! Filter Words: underlines filter words ("just", "really", "suddenly")
//! with Prose's grammar squiggles, and offers to cut them.
//!
//! Shows the `lint` export: Prose hands it each paragraph, and what it
//! returns is drawn and offered like Prose's own grammar problems. The
//! words come from the plugin's settings, so a writer changes them in
//! `[plugins.filter-words]` in settings.toml.

use prose_plugin::{Context, Diagnostic, Lint, Plugin, settings};

#[derive(Default)]
struct FilterWords;

impl Plugin for FilterWords {
    fn run_command(
        &mut self,
        name: &str,
        _args: Vec<String>,
        _cx: Context,
    ) -> prose_plugin::Result {
        Err(format!("Filter Words has no command `{name}`"))
    }
}

impl Lint for FilterWords {
    fn lint(&mut self, paragraph: &str) -> Vec<Diagnostic> {
        let words = settings::text_list("words").unwrap_or_default();
        let in_dialogue = settings::bool("in_dialogue").unwrap_or(false);
        find(paragraph, &words, in_dialogue)
            .into_iter()
            .map(|(start, end)| diagnostic(paragraph, start, end))
            .collect()
    }
}

prose_plugin::export!(FilterWords, lint);

/// The diagnostic for the filter word at `start..end`: its range takes one
/// space beside the word too, so cutting it leaves the sentence spaced right.
fn diagnostic(text: &str, start: usize, end: usize) -> Diagnostic {
    let word = &text[start..end];
    let (start, end) = if text[end..].starts_with(' ') {
        (start, end + 1)
    } else if text[..start].ends_with(' ') {
        (start - 1, end)
    } else {
        (start, end)
    };
    Diagnostic::style(
        start,
        end,
        "filter_word",
        format!("\u{201c}{word}\u{201d} is a filter word: cut it, or say it more directly."),
    )
    .suggest("")
}

/// The byte ranges of `words` (whole words or phrases, any case) in `text`,
/// in order and apart, leaving out those inside quotation marks unless
/// `in_dialogue`.
fn find(text: &str, words: &[String], in_dialogue: bool) -> Vec<(usize, usize)> {
    let lower = text.to_lowercase();
    // Lower-casing can change lengths (rarely); then match nothing rather
    // than return ranges that don't fit the text.
    if lower.len() != text.len() {
        return Vec::new();
    }
    let speech = if in_dialogue {
        Vec::new()
    } else {
        dialogue(text)
    };
    let mut found = Vec::new();
    for word in words {
        let word = word.trim().to_lowercase();
        if word.is_empty() {
            continue;
        }
        for (at, _) in lower.match_indices(&word) {
            let end = at + word.len();
            let whole = !lower[..at].chars().next_back().is_some_and(is_word_char)
                && !lower[end..].chars().next().is_some_and(is_word_char);
            let spoken = speech.iter().any(|&(s, e)| s <= at && end <= e);
            if whole && !spoken {
                found.push((at, end));
            }
        }
    }
    found.sort_unstable();
    // Keep the first of any that overlap ("just" inside a phrase).
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for range in found {
        if kept.last().is_none_or(|last| last.1 <= range.0) {
            kept.push(range);
        }
    }
    kept
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}'
}

/// The byte ranges inside quotation marks: curly double quotes, or
/// straight ones taken in pairs. An unclosed quote runs to the end, as a
/// paragraph of speech that goes on into the next does.
fn dialogue(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<usize> = None;
    for (at, c) in text.char_indices() {
        match (c, open) {
            ('\u{201c}', None) | ('"', None) => open = Some(at),
            ('\u{201d}', Some(start)) | ('"', Some(start)) => {
                ranges.push((start, at + c.len_utf8()));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        ranges.push((start, text.len()));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    fn flagged<'a>(text: &'a str, list: &[&str], in_dialogue: bool) -> Vec<&'a str> {
        find(text, &words(list), in_dialogue)
            .into_iter()
            .map(|(s, e)| &text[s..e])
            .collect()
    }

    #[test]
    fn finds_whole_words_in_any_case() {
        assert_eq!(
            flagged(
                "Just then she really justified it.",
                &["just", "really"],
                false
            ),
            ["Just", "really"]
        );
        assert_eq!(
            flagged("He began to run. She began tonight.", &["began to"], false),
            ["began to"]
        );
    }

    #[test]
    fn leaves_dialogue_alone_unless_asked() {
        let text = "\u{201c}I just can\u{2019}t,\u{201d} she said, just once.";
        assert_eq!(flagged(text, &["just"], false), ["just"]);
        assert_eq!(flagged(text, &["just"], false).len(), 1);
        let (at, _) = find(text, &words(&["just"]), false)[0];
        assert!(at > text.find('\u{201d}').unwrap());
        assert_eq!(flagged(text, &["just"], true).len(), 2);
        assert!(flagged("\"Really,\" he said.", &["really"], false).is_empty());
    }

    #[test]
    fn cutting_keeps_one_space() {
        let text = "She just left.";
        let d = diagnostic(text, 4, 8);
        assert_eq!(&text[d.start as usize..d.end as usize], "just ");
        assert_eq!(d.suggestions, [""]);
        let text = "She left, just";
        let d = diagnostic(text, 10, 14);
        assert_eq!(&text[d.start as usize..d.end as usize], " just");
    }
}
