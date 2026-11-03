//! Names: `:Name` opens a picker of character names and puts the one the
//! writer chooses at the caret (in place of the selection, if any).
//!
//! Shows how to use a picker: open it with the first few items so it
//! shows at once, stream the rest in with `add` (stopping when the writer
//! has already closed it), then `wait` for the choice and act on it. Each
//! name has a detail (where the given name comes from) and a preview (the
//! name in a line of dialogue), so Prose draws it as the finder. Its
//! `plugin.toml` asks for `edit-documents`, to write the name.

use prose_plugin::ui::{self, PickItem};
use prose_plugin::{Command, Context, Plugin, settings};

/// Given names and where they come from.
const GIVEN: &[(&str, &str)] = &[
    ("Ada", "English"),
    ("Agnes", "Greek"),
    ("Alasdair", "Scottish"),
    ("Anouk", "Dutch"),
    ("Beatrix", "Latin"),
    ("Bram", "Dutch"),
    ("Cassius", "Latin"),
    ("Clementine", "French"),
    ("Dagny", "Norse"),
    ("Delphine", "French"),
    ("Edmund", "English"),
    ("Elspeth", "Scottish"),
    ("Esme", "French"),
    ("Ezra", "Hebrew"),
    ("Fenna", "Frisian"),
    ("Florian", "Latin"),
    ("Greer", "Scottish"),
    ("Hollis", "English"),
    ("Ines", "Spanish"),
    ("Isaak", "Hebrew"),
    ("Juno", "Latin"),
    ("Kasimir", "Polish"),
    ("Linnea", "Swedish"),
    ("Lorcan", "Irish"),
    ("Mara", "Hebrew"),
    ("Matthias", "Greek"),
    ("Niamh", "Irish"),
    ("Oskar", "Norse"),
    ("Perpetua", "Latin"),
    ("Quentin", "French"),
    ("Rosalind", "German"),
    ("Silas", "Latin"),
    ("Tamsin", "Cornish"),
    ("Teodor", "Greek"),
    ("Ursula", "Latin"),
    ("Viggo", "Norse"),
    ("Wren", "English"),
    ("Yusuf", "Arabic"),
    ("Zofia", "Polish"),
    ("Zeno", "Greek"),
];

/// Surnames.
const FAMILY: &[&str] = &[
    "Ashdown",
    "Blackwood",
    "Calloway",
    "Darrow",
    "Ellery",
    "Fairweather",
    "Gale",
    "Hartigan",
    "Ingram",
    "Jessop",
    "Kestrel",
    "Lowell",
    "Marchbanks",
    "Nettleford",
    "Oakes",
    "Pembroke",
    "Quill",
    "Ravensworth",
    "Sallow",
    "Thorne",
    "Underhill",
    "Vance",
    "Whitlock",
    "Yarrow",
    "Abernathy",
    "Brightwater",
    "Corwin",
    "Dunmore",
    "Elmswood",
    "Farrier",
    "Greaves",
    "Holloway",
    "Ivers",
    "Kincaid",
    "Larkspur",
    "Morrow",
    "Nightingale",
    "Ormsby",
    "Penhallow",
    "Rook",
    "Stroud",
    "Tennant",
    "Vale",
    "Winslow",
    "Ashcombe",
    "Bellamy",
    "Crane",
    "Dorsey",
    "Esterly",
    "Fenwick",
];

/// How many names go to the picker at a time.
const BATCH: usize = 250;

#[derive(Default)]
struct Names;

impl Plugin for Names {
    fn activate(&mut self) -> prose_plugin::Result {
        Command::new("name", "Insert a character name")
            .description("Pick a generated name and put it at the caret")
            .ex_name("Name")
            .register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, _args: Vec<String>, cx: Context) -> prose_plugin::Result {
        match name {
            "name" => insert_name(cx),
            _ => Err(format!("Names has no command `{name}`")),
        }
    }
}

fn insert_name(cx: Context) -> prose_plugin::Result {
    let doc = cx.document().ok_or("Open a document first")?;
    let count = settings::integer("count").unwrap_or(2000).clamp(1, 20_000) as usize;
    let mut names = (0..count).map(name);
    // The first batch shows at once; the rest stream in behind it.
    let first: Vec<PickItem> = names.by_ref().take(BATCH).map(item).collect();
    let picker = ui::open_picker("Pick a name", &first);
    let mut all: Vec<String> = first.into_iter().map(|item| item.title).collect();
    loop {
        let batch: Vec<PickItem> = names.by_ref().take(BATCH).map(item).collect();
        if batch.is_empty() {
            break;
        }
        if !picker.add(&batch) {
            // Already chosen or dismissed: `wait` says which.
            break;
        }
        all.extend(batch.into_iter().map(|item| item.title));
    }
    let Some(picked) = picker.wait() else {
        return Ok(());
    };
    // There is nothing to split here: Ctrl-V and Ctrl-X insert too.
    let chosen = all
        .get(picked.index as usize)
        .ok_or("The picker chose a name it was never given")?
        .clone();
    doc.edit_with_retry(|doc, edit| match doc.selection() {
        // In place of the selection, or at a bare caret.
        Some(selection) => {
            edit.replace_text(selection.span_in(doc), chosen.clone());
        }
        None => {
            // Not in the active pane: at the end of its last block.
            if let Some(last) = doc.block_count().checked_sub(1)
                && let Some(block) = doc.blocks_from(last).next()
            {
                let end = block.text.len() as u32;
                edit.replace(&block.id, end, end, &chosen);
            }
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The `n`th name: every given name with a first surname, then each with a
/// second, and so on, so the list starts varied.
fn name(n: usize) -> (String, &'static str) {
    let (given, origin) = GIVEN[n % GIVEN.len()];
    // Step through surnames by a number coprime to their count, so
    // neighbours in the list don't share one.
    let family = FAMILY[(n / GIVEN.len() * 7 + n) % FAMILY.len()];
    let round = n / (GIVEN.len() * FAMILY.len());
    let full = match round {
        0 => format!("{given} {family}"),
        // Past every pairing, a middle initial keeps them apart.
        r => {
            let initial = char::from(b'A' + ((r - 1) % 26) as u8);
            format!("{given} {initial}. {family}")
        }
    };
    (full, origin)
}

fn item((full, origin): (String, &'static str)) -> PickItem {
    let given = full.split(' ').next().unwrap_or(&full).to_string();
    PickItem::new(full.clone())
        .detail(origin)
        .preview(format!(
            "\u{201C}{full},\u{201D} the clerk read aloud.\n\u{201C}Just {given},\u{201D} came the answer."
        ))
}

prose_plugin::export!(Names);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_all_different() {
        let names: HashSet<String> = (0..20_000).map(|n| name(n).0).collect();
        assert_eq!(names.len(), 20_000);
    }

    #[test]
    fn neighbours_differ_in_both_names() {
        let a = name(0).0;
        let b = name(1).0;
        assert_ne!(a.split(' ').next_back(), b.split(' ').next_back());
        assert_ne!(a.split(' ').next(), b.split(' ').next());
    }

    #[test]
    fn items_have_a_detail_and_a_preview() {
        let item = item(name(0));
        assert_eq!(item.title, "Ada Ashdown");
        assert_eq!(item.detail.as_deref(), Some("English"));
        assert!(item.preview.unwrap().contains("Just Ada"));
    }
}
