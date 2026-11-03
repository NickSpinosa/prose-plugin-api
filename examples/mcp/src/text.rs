//! Documents as the assistant reads them: one numbered line per paragraph,
//! in a Markdown-like shape, and the small conversions the tools need.

use prose_plugin::project::{FieldValue, Item, ItemKind, RootFolder};
use prose_plugin::{Block, BlockKind, ListMarker, Marks};

/// A paragraph as one line: `[12] ## A heading`. Newlines inside it (line
/// breaks) are kept as `\n`, so a paragraph stays one line.
pub fn paragraph(index: u32, block: &Block, formatting: bool) -> String {
    let body = if formatting {
        with_marks(block)
    } else {
        block.text.clone()
    };
    let body = body.replace('\n', "\\n");
    let prefix = match &block.kind {
        BlockKind::Paragraph => String::new(),
        BlockKind::Heading(level) => format!("{} ", "#".repeat((*level).clamp(1, 6) as usize)),
        BlockKind::BlockQuote => "> ".into(),
        BlockKind::ListItem(item) => {
            let indent = "  ".repeat(item.depth as usize);
            match item.marker {
                ListMarker::Bullet => format!("{indent}- "),
                ListMarker::Ordered => format!("{indent}1. "),
            }
        }
        BlockKind::SceneBreak => return format!("[{index}] * * *"),
        BlockKind::Image => "[image] ".into(),
        BlockKind::Raw => "[raw] ".into(),
    };
    format!("[{index}] {prefix}{body}")
}

/// The block's text with bold, italic, strikethrough and code marked as
/// Markdown marks them.
fn with_marks(block: &Block) -> String {
    let mut out = String::new();
    let mut at = 0usize;
    for run in &block.runs {
        let end = (at + run.len as usize).min(block.text.len());
        let Some(piece) = block.text.get(at..end) else {
            return block.text.clone();
        };
        at = end;
        let mut marks = Vec::new();
        if run.marks.contains(Marks::CODE) {
            marks.push("`");
        }
        if run.marks.contains(Marks::BOLD) {
            marks.push("**");
        }
        if run.marks.contains(Marks::ITALIC) {
            marks.push("*");
        }
        if run.marks.contains(Marks::STRIKETHROUGH) {
            marks.push("~~");
        }
        // Marks go inside the run's spaces, as Markdown wants them.
        let trimmed = piece.trim();
        if marks.is_empty() || trimmed.is_empty() {
            out.push_str(piece);
            continue;
        }
        let lead = &piece[..piece.len() - piece.trim_start().len()];
        let tail = &piece[piece.trim_end().len()..];
        out.push_str(lead);
        for m in &marks {
            out.push_str(m);
        }
        out.push_str(trimmed);
        for m in marks.iter().rev() {
            out.push_str(m);
        }
        out.push_str(tail);
    }
    if at < block.text.len() {
        out.push_str(&block.text[at..]);
    }
    out
}

/// The kinds a tool may make.
pub const KINDS: [&str; 10] = [
    "paragraph",
    "heading-1",
    "heading-2",
    "heading-3",
    "heading-4",
    "heading-5",
    "heading-6",
    "quote",
    "bullet",
    "scene-break",
];

pub fn parse_kind(name: &str) -> Option<BlockKind> {
    Some(match name {
        "paragraph" => BlockKind::Paragraph,
        "quote" => BlockKind::BlockQuote,
        "bullet" => BlockKind::ListItem(prose_plugin::ListItem {
            marker: ListMarker::Bullet,
            depth: 0,
        }),
        "numbered" => BlockKind::ListItem(prose_plugin::ListItem {
            marker: ListMarker::Ordered,
            depth: 0,
        }),
        "scene-break" => BlockKind::SceneBreak,
        _ => {
            let level: u8 = name.strip_prefix("heading-")?.parse().ok()?;
            if !(1..=6).contains(&level) {
                return None;
            }
            BlockKind::Heading(level)
        }
    })
}

pub fn folder_name(folder: RootFolder) -> &'static str {
    match folder {
        RootFolder::Manuscript => "manuscript",
        RootFolder::Research => "research",
        RootFolder::Templates => "templates",
        RootFolder::Trash => "trash",
    }
}

pub fn parse_folder(name: &str) -> Option<RootFolder> {
    Some(match name {
        "manuscript" | "draft" => RootFolder::Manuscript,
        "research" => RootFolder::Research,
        "templates" => RootFolder::Templates,
        "trash" => RootFolder::Trash,
        _ => return None,
    })
}

pub fn item_kind(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Document => "document",
        ItemKind::Folder => "folder",
    }
}

pub fn field_value(value: &FieldValue) -> serde_json::Value {
    match value {
        FieldValue::Text(text) => text.clone().into(),
        FieldValue::Flag(flag) => (*flag).into(),
        FieldValue::Date(date) => date.clone().into(),
    }
}

/// `true` if the item is in the trash.
pub fn trashed(item: &Item) -> bool {
    item.root == RootFolder::Trash
}

/// Seconds since the Unix epoch, at a UTC offset in seconds, as
/// `2026-11-03 14:05`.
pub fn time(seconds: i64, offset: i32) -> String {
    let local = seconds + offset as i64;
    let days = local.div_euclid(86_400);
    let of_day = local.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        of_day / 3600,
        (of_day % 3600) / 60
    )
}

/// Days since 1970-01-01 as a calendar date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prose_plugin::Run;

    fn block(kind: BlockKind, text: &str, runs: Vec<Run>) -> Block {
        Block {
            id: "1".into(),
            kind,
            text: text.into(),
            runs,
            annotations: Vec::new(),
        }
    }

    fn run(len: u32, marks: Marks) -> Run {
        Run {
            len,
            marks,
            style: None,
        }
    }

    #[test]
    fn paragraphs_read_like_markdown() {
        let b = block(BlockKind::Heading(2), "Storm", vec![run(5, Marks::empty())]);
        assert_eq!(paragraph(3, &b, false), "[3] ## Storm");
        let b = block(
            BlockKind::Paragraph,
            "a very big deal",
            vec![
                run(2, Marks::empty()),
                run(5, Marks::ITALIC),
                run(8, Marks::empty()),
            ],
        );
        assert_eq!(paragraph(0, &b, true), "[0] a *very* big deal");
        assert_eq!(paragraph(0, &b, false), "[0] a very big deal");
        let b = block(BlockKind::SceneBreak, "", vec![]);
        assert_eq!(paragraph(9, &b, false), "[9] * * *");
    }

    #[test]
    fn kinds_round_trip() {
        for name in KINDS {
            assert!(parse_kind(name).is_some(), "{name}");
        }
        assert_eq!(parse_kind("heading-2"), Some(BlockKind::Heading(2)));
        assert!(parse_kind("heading-7").is_none());
    }

    #[test]
    fn times_are_dates() {
        assert_eq!(time(0, 0), "1970-01-01 00:00");
        assert_eq!(time(1_793_714_700, 3600), "2026-11-03 15:05");
    }
}
