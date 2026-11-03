//! Fountain: a compile format and an import format for
//! [Fountain](https://fountain.io), the plain-text screenplay format.
//!
//! Shows the `compile` hooks. A preset with `format = "fountain.fountain"`
//! compiles the book to a `.fountain` file, and importing a `.fountain` file
//! makes a document of each of its top-level sections. Its `plugin.toml`
//! asks for the `compile` permission and names both formats.
//!
//! How the book maps onto a screenplay:
//!
//! | Prose | Fountain |
//! |---|---|
//! | A heading of level n | A section, n `#`s |
//! | A paragraph starting `INT.`, `EXT.`, `EST.`, `INT./EXT.` or `I/E` | A scene heading |
//! | A paragraph `NAME: words` (the name in capitals) | A character and their dialogue |
//! | A scene break | A page break, `===` |
//! | Any other paragraph | Action (forced with `!` where Fountain would read it as something else) |
//! | Italic, bold, underline | `*`, `**`, `_` |
//! | A footnote | A note, `[[…]]` |
//!
//! Importing reads the same shapes back, and a synopsis (`= …`) under a
//! section becomes its document's synopsis.

use prose_plugin::compile::{CompileInfo, Imported, Layout, Note};
use prose_plugin::{
    AnnotationKind, Block, BlockKind, Compile, Context, Document, Marks, NewBlock, Plugin, Run,
};

#[derive(Default)]
struct Fountain;

impl Plugin for Fountain {
    fn run_command(
        &mut self,
        name: &str,
        _args: Vec<String>,
        _cx: Context,
    ) -> prose_plugin::Result {
        Err(format!("Fountain has no command `{name}`"))
    }
}

impl Compile for Fountain {
    fn export_format(
        &mut self,
        format: &str,
        info: CompileInfo,
        manuscript: Document,
        notes: Vec<Note>,
        _layout: Layout,
    ) -> prose_plugin::Result<Vec<u8>> {
        if format != "fountain" {
            return Err(format!("Fountain has no export format {format}"));
        }
        Ok(write(&info, manuscript.blocks(), &notes).into_bytes())
    }

    fn import_format(
        &mut self,
        format: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> prose_plugin::Result<Vec<Imported>> {
        if format != "fountain" {
            return Err(format!("Fountain has no import format {format}"));
        }
        let text = String::from_utf8(bytes).map_err(|_| format!("{file_name} isn't UTF-8 text"))?;
        let stem = file_name
            .rsplit_once('.')
            .map_or(file_name, |(stem, _)| stem);
        Ok(read(&text, stem))
    }
}

prose_plugin::export!(Fountain, compile);

// Writing

/// The screenplay for the blocks of a manuscript.
pub fn write(
    info: &CompileInfo,
    blocks: impl IntoIterator<Item = Block>,
    notes: &[Note],
) -> String {
    let mut out = String::new();
    if !info.title.is_empty() {
        out.push_str(&format!("Title: {}\n", info.title));
    }
    if !info.author.is_empty() {
        out.push_str(&format!("Credit: written by\nAuthor: {}\n", info.author));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    for block in blocks {
        let element = match &block.kind {
            BlockKind::Heading(level) => {
                format!("{} {}", "#".repeat(*level as usize), block.text.trim())
            }
            BlockKind::SceneBreak => "===".to_owned(),
            BlockKind::Image => continue,
            _ => paragraph(&block, notes),
        };
        if element.trim().is_empty() {
            continue;
        }
        out.push_str(&element);
        out.push_str("\n\n");
    }
    out
}

/// A paragraph as a scene heading, a character's dialogue, or action.
fn paragraph(block: &Block, notes: &[Note]) -> String {
    let plain = block.text.trim();
    if is_scene_heading(plain) {
        return plain.to_uppercase();
    }
    if let Some((name, speech)) = dialogue(plain) {
        let at = block.text.find(':').map_or(0, |colon| colon + 1);
        let at = at + (block.text[at..].len() - block.text[at..].trim_start().len());
        let speech = if speech.is_empty() {
            String::new()
        } else {
            styled(block, at, notes)
        };
        return format!("{name}\n{}", speech.trim());
    }
    let text = styled(block, 0, notes);
    let text = text.trim();
    let forced = text.starts_with(['!', '@', '#', '~', '.', '>', '=', '['])
        || (text.chars().any(char::is_alphabetic) && !text.chars().any(char::is_lowercase))
        || is_scene_heading(text);
    if forced {
        format!("!{text}")
    } else {
        text.to_owned()
    }
}

fn is_scene_heading(text: &str) -> bool {
    let upper = text.to_uppercase();
    ["INT.", "EXT.", "EST.", "INT./EXT.", "INT/EXT.", "I/E "]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
}

/// `NAME: words` with the name in capitals: the name and the words.
fn dialogue(text: &str) -> Option<(&str, &str)> {
    let (name, speech) = text.split_once(':')?;
    let name = name.trim();
    let is_name = name.chars().any(char::is_alphabetic)
        && !name.chars().any(char::is_lowercase)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || " .'-()".contains(c));
    is_name.then(|| (name, speech.trim()))
}

/// Block `block`'s text from byte `from` on, with its emphasis in Fountain's
/// marks and its footnotes as notes.
fn styled(block: &Block, from: usize, notes: &[Note]) -> String {
    let mut out = String::new();
    let mut at = 0usize;
    let runs: Vec<Run> = if block.runs.is_empty() {
        vec![Run {
            len: block.text.len() as u32,
            marks: Marks::empty(),
            style: None,
        }]
    } else {
        block.runs.clone()
    };
    for run in runs {
        let (start, end) = (at, at + run.len as usize);
        at = end;
        if end <= from {
            continue;
        }
        let start = start.max(from);
        let mut text = String::new();
        for (offset, c) in block.text[start..end].char_indices() {
            text.push_str(&footnotes(block, start + offset, notes));
            text.push(c);
        }
        let (open, close) = marks(run.marks);
        // Emphasis can't start or end on a space in Fountain.
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[text.trim_end().len()..];
        let core = text.trim();
        if core.is_empty() {
            out.push_str(&text);
        } else {
            out.push_str(&format!("{lead}{open}{core}{close}{trail}"));
        }
    }
    out.push_str(&footnotes(block, block.text.len(), notes));
    out
}

fn marks(marks: Marks) -> (&'static str, &'static str) {
    match (
        marks.contains(Marks::BOLD),
        marks.contains(Marks::ITALIC),
        marks.contains(Marks::UNDERLINE),
    ) {
        (true, true, true) => ("_***", "***_"),
        (true, true, false) => ("***", "***"),
        (true, false, true) => ("_**", "**_"),
        (false, true, true) => ("_*", "*_"),
        (true, false, false) => ("**", "**"),
        (false, true, false) => ("*", "*"),
        (false, false, true) => ("_", "_"),
        (false, false, false) => ("", ""),
    }
}

/// The notes of the footnotes at byte `at` of `block`.
fn footnotes(block: &Block, at: usize, notes: &[Note]) -> String {
    block
        .annotations
        .iter()
        .filter(|a| a.kind == AnnotationKind::Footnote && a.start as usize == at)
        .filter_map(|a| notes.iter().find(|n| n.id == a.id))
        .map(|note| {
            let text: Vec<&str> = note.blocks.iter().map(|b| b.text.trim()).collect();
            format!("[[{}]]", text.join(" "))
        })
        .collect()
}

// Reading

/// The documents a screenplay makes: one per top-level section, and one
/// before the first for what comes before it, named for the title page's
/// title or the file.
pub fn read(text: &str, file_stem: &str) -> Vec<Imported> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = text.lines().collect();
    let mut title = file_stem.to_owned();
    // A title page: `Key: value` lines up to the first blank line.
    if lines
        .first()
        .is_some_and(|l| l.split_once(':').is_some_and(|(k, _)| is_key(k)))
    {
        let end = lines
            .iter()
            .position(|l| l.trim().is_empty())
            .unwrap_or(lines.len());
        for line in &lines[..end] {
            if let Some((key, value)) = line.split_once(':')
                && key.trim().eq_ignore_ascii_case("title")
                && !value.trim().is_empty()
            {
                title = value.trim().to_owned();
            }
        }
        lines.drain(..end);
    }
    let mut documents = vec![Imported {
        title,
        synopsis: String::new(),
        blocks: Vec::new(),
    }];
    // Elements are separated by blank lines; a section or synopsis line is
    // one on its own.
    let mut elements: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        let alone =
            trimmed.starts_with('#') || (trimmed.starts_with('=') && !trimmed.starts_with("==="));
        if trimmed.is_empty() || alone {
            if !current.is_empty() {
                elements.push(std::mem::take(&mut current));
            }
            if alone {
                elements.push(vec![line]);
            }
            continue;
        }
        current.push(line);
    }
    if !current.is_empty() {
        elements.push(current);
    }
    for element in elements {
        let Some(first) = element.first() else {
            continue;
        };
        let first = first.trim();
        let doc = documents.last_mut().expect("one document at least");
        if first.starts_with('#') {
            let level = first.chars().take_while(|&c| c == '#').count();
            let heading = first[level..].trim().to_owned();
            if level == 1 {
                documents.push(Imported {
                    title: heading,
                    synopsis: String::new(),
                    blocks: Vec::new(),
                });
            } else {
                doc.blocks
                    .push(block(BlockKind::Heading(level.min(6) as u8), &heading));
            }
        } else if let Some(synopsis) = first.strip_prefix('=').filter(|s| !s.starts_with("==")) {
            if !doc.synopsis.is_empty() {
                doc.synopsis.push(' ');
            }
            doc.synopsis.push_str(synopsis.trim());
        } else if first.starts_with("===") {
            doc.blocks.push(block(BlockKind::SceneBreak, ""));
        } else if is_cue(&element) {
            let name = first.trim_start_matches('@').trim();
            let speech: Vec<&str> = element[1..].iter().map(|l| l.trim()).collect();
            doc.blocks.push(block(
                BlockKind::Paragraph,
                &format!("{name}: {}", speech.join(" ")),
            ));
        } else {
            let lines: Vec<&str> = element
                .iter()
                .map(|l| {
                    let l = l.trim();
                    let l = l.strip_prefix('!').unwrap_or(l);
                    let l = l.strip_prefix('>').map_or(l, |l| l.trim());
                    let l = l.strip_suffix('<').map_or(l, |l| l.trim());
                    l.strip_prefix('.')
                        .filter(|_| !l.starts_with(".."))
                        .unwrap_or(l)
                })
                .collect();
            doc.blocks
                .push(block(BlockKind::Paragraph, &lines.join(" ")));
        }
    }
    if documents.len() > 1 && documents[0].blocks.is_empty() && documents[0].synopsis.is_empty() {
        documents.remove(0);
    }
    documents
}

fn is_key(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == ' ')
}

/// A character's name over at least one line of dialogue.
fn is_cue(element: &[&str]) -> bool {
    let first = element[0].trim();
    if element.len() < 2 || first.starts_with('!') {
        return false;
    }
    if first.starts_with('@') {
        return true;
    }
    let name = first.split('(').next().unwrap_or(first);
    name.chars().any(char::is_alphabetic)
        && !name.chars().any(char::is_lowercase)
        && !is_scene_heading(first)
}

/// A block of `kind` from Fountain text, its emphasis read into marks and
/// its notes left out.
fn block(kind: BlockKind, source: &str) -> NewBlock {
    let (text, runs) = emphasis(&strip_notes(source));
    NewBlock {
        id: prose_plugin::new_id(),
        kind,
        text,
        runs,
    }
}

fn strip_notes(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let Some(end) = rest[start..].find("]]") else {
            break;
        };
        out.push_str(&rest[..start]);
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

/// Fountain's `*italic*`, `**bold**` and `_underline_` read into runs. A
/// mark with no closing one later is kept as text.
fn emphasis(source: &str) -> (String, Vec<Run>) {
    let mut text = String::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut marks = Marks::empty();
    let mut rest = source;
    let mut push = |text: &mut String, piece: &str, marks: Marks| {
        if piece.is_empty() {
            return;
        }
        text.push_str(piece);
        match runs.last_mut() {
            Some(run) if run.marks == marks => run.len += piece.len() as u32,
            _ => runs.push(Run {
                len: piece.len() as u32,
                marks,
                style: None,
            }),
        }
    };
    while !rest.is_empty() {
        let next = rest.find(['*', '_', '\\']).unwrap_or(rest.len());
        push(&mut text, &rest[..next], marks);
        rest = &rest[next..];
        if rest.is_empty() {
            break;
        }
        if let Some(escaped) = rest.strip_prefix('\\') {
            let len = escaped.chars().next().map_or(0, char::len_utf8);
            push(&mut text, &escaped[..len], marks);
            rest = &escaped[len..];
            continue;
        }
        let (delimiter, mark) = if rest.starts_with("**") {
            ("**", Marks::BOLD)
        } else if rest.starts_with('*') {
            ("*", Marks::ITALIC)
        } else {
            ("_", Marks::UNDERLINE)
        };
        let after = &rest[delimiter.len()..];
        let closes = marks.contains(mark);
        let opens = !closes && closing(after, delimiter);
        if closes || opens {
            marks.toggle(mark);
        } else {
            push(&mut text, delimiter, marks);
        }
        rest = after;
    }
    if runs.len() == 1 && runs[0].marks.is_empty() {
        runs.clear();
    }
    (text, runs)
}

/// Whether `delimiter` closes later in `rest`, by itself (`*` not as part
/// of `**`).
fn closing(rest: &str, delimiter: &str) -> bool {
    let mut at = 0;
    while let Some(found) = rest[at..].find(delimiter) {
        let i = at + found;
        let doubled = delimiter == "*" && rest[i + 1..].starts_with('*');
        if !doubled {
            return i > 0;
        }
        at = i + 2;
    }
    false
}
