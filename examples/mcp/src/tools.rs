//! The tools: what an assistant can ask of Prose. Reading needs only
//! `read-project`; the tools that change something are listed only while
//! the writer hasn't turned on `read_only`, and each change is one undo
//! step (an edit in its pane, a binder change in the binder's history).

use std::time::{Duration, Instant};

use prose_plugin::binder::{self, BinderError, MetaChange};
use prose_plugin::project::{self, Item, ItemKind, Place, Query, RootFolder, SearchError};
use prose_plugin::workspace::{self, OpenError, Placement};
use prose_plugin::{Document, EditError, history, settings};
use serde_json::{Map, Value, json};

use crate::edit::{self, Change};
use crate::text;

/// How long an edit waits for Prose to open a document no pane shows.
const OPENING: Duration = Duration::from_secs(3);
/// The most paragraphs `read_document` gives at once.
const MAX_PARAGRAPHS: u32 = 1000;
/// The most hits `search` gives.
const MAX_HITS: u32 = 500;

type Args = Map<String, Value>;

/// A tool: its name, whether it changes anything, its description and the
/// JSON schema of its arguments.
struct Tool {
    name: &'static str,
    title: &'static str,
    changes: bool,
    description: &'static str,
    schema: fn() -> Value,
    run: fn(&Args) -> Result<String, String>,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "get_project",
        title: "Project overview",
        changes: false,
        description: "The open project: its name, the documents open in panes (and which is active), \
            the ids of its top-level folders, its labels, statuses and custom fields, the manuscript's \
            word count and this session's words. Start here.",
        schema: no_arguments,
        run: get_project,
    },
    Tool {
        name: "list_binder",
        title: "List the binder",
        changes: false,
        description: "Every item in the binder (documents and folders), in binder order, with id, title, \
            parent, depth, synopsis, label, status, keywords and word count. Leaves out the trash \
            unless asked.",
        schema: list_binder_schema,
        run: list_binder,
    },
    Tool {
        name: "get_item",
        title: "Item details",
        changes: false,
        description: "One binder item's metadata (synopsis, label, status, keywords, word target, \
            custom fields, whether compile includes it) and its notes.",
        schema: id_schema,
        run: get_item,
    },
    Tool {
        name: "read_document",
        title: "Read a document",
        changes: false,
        description: "A document's text, one paragraph per line, numbered from 0: `[12] text`. Headings \
            start with #, quotes with >, list items with -, scene breaks are `* * *`. Long documents \
            come in pages: pass start and count. Also gives the version to pass to edit_document.",
        schema: read_document_schema,
        run: read_document,
    },
    Tool {
        name: "get_selection",
        title: "What the writer selected",
        changes: false,
        description: "The document in the writer's active pane and what is selected in it: the \
            selected text and the paragraphs it covers, or where the caret is.",
        schema: no_arguments,
        run: get_selection,
    },
    Tool {
        name: "search",
        title: "Search the project",
        changes: false,
        description: "Search every document as Prose's project search does: titles, synopses, text, \
            footnotes and comments. Gives each hit's document, paragraph number and a snippet.",
        schema: search_schema,
        run: search,
    },
    Tool {
        name: "writing_stats",
        title: "Writing statistics",
        changes: false,
        description: "Words written this session (and the session target), and net words written \
            each day.",
        schema: writing_stats_schema,
        run: writing_stats,
    },
    Tool {
        name: "list_history",
        title: "List revisions",
        changes: false,
        description: "The project's history. With a document id: the revisions that changed it and its \
            snapshots. Without: the drafts and the current draft's latest revisions. Read one with \
            read_revision.",
        schema: list_history_schema,
        run: list_history,
    },
    Tool {
        name: "read_revision",
        title: "Read an old version",
        changes: false,
        description: "A document as a revision kept it, numbered like read_document. Read only.",
        schema: read_revision_schema,
        run: read_revision,
    },
    Tool {
        name: "open_document",
        title: "Show a document",
        changes: false,
        description: "Show a document to the writer in Prose: in the active pane, or in a new pane \
            beside or below it.",
        schema: open_document_schema,
        run: open_document,
    },
    Tool {
        name: "edit_document",
        title: "Edit a document",
        changes: true,
        description: "Change a document's text as one undo step. Opens it in Prose if no pane shows it, \
            so the writer sees the change. Each edit is one of: {\"find\": \"old\", \"replace\": \"new\", \
            \"paragraph\": n} (exactly one match, in paragraph n or anywhere if paragraph is left out; \
            all: true for every match); {\"paragraph\": n, \"text\": \"...\"} (rewrite a paragraph; \\n \
            starts another); {\"paragraph\": n, \"kind\": \"heading-2\"}; {\"insert_after\": n, \
            \"paragraphs\": [\"...\"], \"kind\": \"paragraph\"} (-1 inserts first); {\"remove\": n, \
            \"count\": 1}. Paragraph numbers are as read_document gave them; pass its version so \
            Prose refuses the edit if the writer typed since.",
        schema: edit_document_schema,
        run: edit_document,
    },
    Tool {
        name: "create_document",
        title: "Add a document",
        changes: true,
        description: "Add a new document (or folder) to the binder, with optional text, at the end of \
            a folder (the manuscript by default) or at an index in it. Gives its id.",
        schema: create_document_schema,
        run: create_document,
    },
    Tool {
        name: "update_item",
        title: "Change an item's metadata",
        changes: true,
        description: "Retitle a binder item or change its synopsis, label, status, keywords, word \
            target, whether compile includes it, or a custom field. Labels and statuses go by id or \
            title (\"\" clears).",
        schema: update_item_schema,
        run: update_item,
    },
    Tool {
        name: "move_item",
        title: "Move an item",
        changes: true,
        description: "Move a binder item into a folder, at an index among its children (the end if \
            left out). Use trash_item to delete.",
        schema: move_item_schema,
        run: move_item,
    },
    Tool {
        name: "trash_item",
        title: "Move an item to the trash",
        changes: true,
        description: "Move a binder item to the trash. Nothing is deleted: the writer can take it back \
            or empty the trash.",
        schema: id_schema,
        run: trash_item,
    },
    Tool {
        name: "replace_in_project",
        title: "Replace across the project",
        changes: true,
        description: "Replace every match of some text across the project (as Prose's project replace \
            does), as one change the writer undoes at once. Gives how many were replaced. Search \
            first to check what matches.",
        schema: replace_schema,
        run: replace_in_project,
    },
];

/// The writer turned off the tools that change things.
fn read_only() -> bool {
    settings::bool("read_only").unwrap_or(false)
}

fn offered() -> impl Iterator<Item = &'static Tool> {
    let read_only = read_only();
    TOOLS.iter().filter(move |t| !(read_only && t.changes))
}

/// `tools/list`'s tools.
pub fn list() -> Vec<Value> {
    offered()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                "annotations": {
                    "title": tool.title,
                    "readOnlyHint": !tool.changes,
                    // Every change can be undone in Prose, but an assistant
                    // should still ask before it rewrites a writer's work.
                    "destructiveHint": tool.changes,
                    "idempotentHint": !tool.changes,
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

pub fn exists(name: &str) -> bool {
    TOOLS.iter().any(|t| t.name == name)
}

/// Run tool `name`. `Err` is a tool error, shown to the assistant.
pub fn call(name: &str, args: &Args) -> Result<String, String> {
    let tool = TOOLS
        .iter()
        .find(|t| t.name == name)
        .ok_or("no such tool")?;
    if tool.changes && read_only() {
        return Err(
            "The writer set this server to read only ([plugins.mcp] read_only in Prose's \
             settings.toml), so it can't change anything"
                .into(),
        );
    }
    (tool.run)(args)
}

// Arguments.

fn text_arg(args: &Args, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{key} must be text")),
    }
}

fn required(args: &Args, key: &str) -> Result<String, String> {
    text_arg(args, key)?.ok_or_else(|| format!("{key} is missing"))
}

fn number_arg(args: &Args, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a whole number, 0 or more")),
    }
}

fn bool_arg(args: &Args, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("{key} must be true or false")),
    }
}

fn list_arg(args: &Args, key: &str) -> Result<Option<Vec<String>>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| i.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a list of text")),
        Some(_) => Err(format!("{key} must be a list of text")),
    }
}

fn pretty(value: Value) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

// Schemas.

fn no_arguments() -> Value {
    json!({"type": "object", "properties": {}})
}

fn id_property() -> Value {
    json!({"type": "string", "description": "A binder id, from list_binder, search or get_project"})
}

fn id_schema() -> Value {
    json!({"type": "object", "properties": {"id": id_property()}, "required": ["id"]})
}

fn folder_names() -> Value {
    json!(["manuscript", "research", "templates", "trash"])
}

fn list_binder_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "folder": {
                "type": "string",
                "enum": ["manuscript", "research", "templates", "trash", "all"],
                "description": "Only this top-level folder. Leave out for everything but the trash.",
            },
            "synopses": {"type": "boolean", "description": "Include synopses (default true)"},
        },
    })
}

fn read_document_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": id_property(),
            "start": {"type": "integer", "minimum": 0, "description": "First paragraph (default 0)"},
            "count": {"type": "integer", "minimum": 1, "maximum": MAX_PARAGRAPHS,
                "description": "How many paragraphs (default 400)"},
            "formatting": {"type": "boolean",
                "description": "Show bold, italic, strikethrough and code as Markdown (default false). \
                    Leave it off when you'll edit: find matches the plain text."},
        },
        "required": ["id"],
    })
}

fn search_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "description": "What to find"},
            "regex": {"type": "boolean", "description": "text is a regular expression (Rust syntax)"},
            "case_sensitive": {"type": "boolean"},
            "whole_word": {"type": "boolean"},
            "places": {"type": "array", "items": {"type": "string",
                "enum": ["title", "synopsis", "text", "footnote", "comment"]},
                "description": "Where to look in each document (default everywhere)"},
            "folders": {"type": "array", "items": {"type": "string", "enum": folder_names()},
                "description": "Which top-level folders (default all but the trash)"},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_HITS,
                "description": "The most hits (default 50)"},
        },
        "required": ["text"],
    })
}

fn writing_stats_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "days": {"type": "integer", "minimum": 1, "description": "The last how many days (default 30)"},
        },
    })
}

fn list_history_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": {"type": "string", "description": "A document's id, for its versions and snapshots"},
            "count": {"type": "integer", "minimum": 1, "maximum": 200, "description": "How many (default 20)"},
        },
    })
}

fn read_revision_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": id_property(),
            "revision": {"type": "string", "description": "A revision id from list_history"},
            "start": {"type": "integer", "minimum": 0},
            "count": {"type": "integer", "minimum": 1, "maximum": MAX_PARAGRAPHS},
        },
        "required": ["document", "revision"],
    })
}

fn placement_property() -> Value {
    json!({"type": "string", "enum": ["active-pane", "split-right", "split-down"],
        "description": "Where (default active-pane)"})
}

fn open_document_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"id": id_property(), "where": placement_property()},
        "required": ["id"],
    })
}

fn edit_document_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": id_property(),
            "version": {"type": "integer", "minimum": 0,
                "description": "The version read_document gave; the edit is refused if the document changed since"},
            "edits": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "find": {"type": "string"},
                        "replace": {"type": "string"},
                        "all": {"type": "boolean"},
                        "paragraph": {"type": "integer", "minimum": 0},
                        "text": {"type": "string"},
                        "kind": {"type": "string", "enum": text::KINDS},
                        "insert_after": {"type": "integer", "minimum": -1},
                        "paragraphs": {"type": "array", "items": {"type": "string"}},
                        "remove": {"type": "integer", "minimum": 0},
                        "count": {"type": "integer", "minimum": 1},
                    },
                },
                "description": "Applied together, as one undo step",
            },
        },
        "required": ["id", "edits"],
    })
}

fn create_document_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": {"type": "string"},
            "parent": {"type": "string", "description": "The folder's id (default the manuscript)"},
            "index": {"type": "integer", "minimum": 0, "description": "Its place among the folder's items (default last)"},
            "folder": {"type": "boolean", "description": "Make a folder instead of a document"},
            "text": {"type": "string", "description": "The document's text; \\n between paragraphs"},
            "synopsis": {"type": "string"},
        },
        "required": ["title"],
    })
}

fn update_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": id_property(),
            "title": {"type": "string"},
            "synopsis": {"type": "string"},
            "label": {"type": "string", "description": "A label's id or title; \"\" clears it"},
            "status": {"type": "string", "description": "A status's id or title; \"\" clears it"},
            "keywords": {"type": "array", "items": {"type": "string"}},
            "target_words": {"type": ["integer", "null"], "minimum": 0, "description": "null clears it"},
            "include_in_compile": {"type": "boolean"},
            "custom": {"type": "object", "description": "Custom field values by field id (text, true/false, or a date like 2026-11-03); null clears one",
                "additionalProperties": true},
        },
        "required": ["id"],
    })
}

fn move_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": id_property(),
            "parent": {"type": "string", "description": "The folder to move it into"},
            "index": {"type": "integer", "minimum": 0},
        },
        "required": ["id", "parent"],
    })
}

fn replace_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "find": {"type": "string"},
            "replace": {"type": "string", "description": "With regex, $1 and ${name} expand"},
            "regex": {"type": "boolean"},
            "case_sensitive": {"type": "boolean"},
            "whole_word": {"type": "boolean"},
            "folders": {"type": "array", "items": {"type": "string", "enum": folder_names()}},
        },
        "required": ["find", "replace"],
    })
}

// Reading.

fn find_item(id: &str) -> Result<Item, String> {
    project::find(id).ok_or_else(|| format!("No item has the id {id:?}; list_binder gives them"))
}

fn label_title(id: &Option<String>, definitions: &[project::Definition]) -> Value {
    match id {
        None => Value::Null,
        Some(id) => definitions
            .iter()
            .find(|d| &d.id == id)
            .map_or(Value::String(id.clone()), |d| {
                Value::String(d.title.clone())
            }),
    }
}

fn item_json(
    item: &Item,
    synopsis: bool,
    labels: &[project::Definition],
    statuses: &[project::Definition],
) -> Value {
    let mut object = json!({
        "id": item.id,
        "kind": text::item_kind(item.kind),
        "title": item.title,
        "parent": item.parent,
        "folder": text::folder_name(item.root),
        "depth": item.depth,
    });
    let o = object.as_object_mut().expect("an object");
    if synopsis && !item.synopsis.is_empty() {
        o.insert("synopsis".into(), item.synopsis.clone().into());
    }
    if item.label.is_some() {
        o.insert("label".into(), label_title(&item.label, labels));
    }
    if item.status.is_some() {
        o.insert("status".into(), label_title(&item.status, statuses));
    }
    if !item.keywords.is_empty() {
        o.insert("keywords".into(), json!(item.keywords));
    }
    if let Some(words) = project::word_count(&item.id) {
        o.insert("words".into(), words.into());
    }
    if let Some(target) = item.target_words {
        o.insert("target_words".into(), target.into());
    }
    if !item.include_in_compile {
        o.insert("include_in_compile".into(), false.into());
    }
    if item.missing {
        o.insert("missing".into(), true.into());
    }
    object
}

fn get_project(_: &Args) -> Result<String, String> {
    let name = workspace::project_name().ok_or("No project is open in Prose")?;
    let roots: Map<String, Value> = [
        RootFolder::Manuscript,
        RootFolder::Research,
        RootFolder::Templates,
        RootFolder::Trash,
    ]
    .into_iter()
    .filter_map(|f| Some((text::folder_name(f).to_owned(), project::root(f)?.into())))
    .collect();
    let manuscript_words =
        project::root(RootFolder::Manuscript).and_then(|id| project::word_count(&id));
    let open: Vec<Value> = workspace::open_documents()
        .into_iter()
        .map(|d| json!({"id": d.id, "title": d.title, "active": d.active}))
        .collect();
    let definitions = |list: Vec<project::Definition>| -> Vec<Value> {
        list.into_iter()
            .map(|d| json!({"id": d.id, "title": d.title, "color": d.color}))
            .collect()
    };
    let fields: Vec<Value> = project::fields()
        .into_iter()
        .map(|f| {
            json!({"id": f.id, "title": f.title, "kind": format!("{:?}", f.kind).to_lowercase(), "options": f.options})
        })
        .collect();
    pretty(json!({
        "name": name,
        "folders": roots,
        "open_documents": open,
        "manuscript_words": manuscript_words,
        "session_words": project::session_words(),
        "session_target": project::session_target(),
        "labels": definitions(project::labels()),
        "statuses": definitions(project::statuses()),
        "custom_fields": fields,
        "read_only": read_only(),
    }))
}

fn list_binder(args: &Args) -> Result<String, String> {
    let folder = text_arg(args, "folder")?;
    let wanted = match folder.as_deref() {
        None => None,
        Some("all") => Some(None),
        Some(name) => {
            Some(Some(text::parse_folder(name).ok_or_else(|| {
                format!("folder {name:?} isn't a top-level folder")
            })?))
        }
    };
    let synopses = bool_arg(args, "synopses")?.unwrap_or(true);
    let (labels, statuses) = (project::labels(), project::statuses());
    let items: Vec<Value> = project::items()
        .iter()
        .filter(|item| match wanted {
            None => !text::trashed(item),
            Some(None) => true,
            Some(Some(folder)) => item.root == folder,
        })
        .map(|item| item_json(item, synopses, &labels, &statuses))
        .collect();
    if items.is_empty() && workspace::project_name().is_none() {
        return Err("No project is open in Prose".into());
    }
    pretty(Value::Array(items))
}

fn get_item(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let item = find_item(&id)?;
    let mut value = item_json(&item, true, &project::labels(), &project::statuses());
    let o = value.as_object_mut().expect("an object");
    o.insert("include_in_compile".into(), item.include_in_compile.into());
    if !item.custom.is_empty() {
        let custom: Map<String, Value> = item
            .custom
            .iter()
            .map(|c| (c.field.clone(), text::field_value(&c.value)))
            .collect();
        o.insert("custom".into(), custom.into());
    }
    if let Some(notes) = project::notes(&id) {
        o.insert("notes".into(), notes.into());
    }
    pretty(value)
}

/// `doc`'s paragraphs `start..start + count`, numbered, under a line
/// saying what they are.
fn page(doc: &Document, args: &Args, what: &str) -> Result<String, String> {
    let total = doc.block_count();
    let start = number_arg(args, "start")?.unwrap_or(0).min(u32::MAX as u64) as u32;
    let count = number_arg(args, "count")?
        .unwrap_or(400)
        .clamp(1, MAX_PARAGRAPHS as u64) as u32;
    let formatting = bool_arg(args, "formatting")?.unwrap_or(false);
    if start >= total && total > 0 {
        return Err(format!("It has {total} paragraphs (0 to {})", total - 1));
    }
    let end = start.saturating_add(count).min(total);
    let mut out = format!(
        "{what} {:?} (id {}), version {}, {} words, paragraphs {start}-{} of {total}\n",
        doc.title(),
        doc.id(),
        doc.version(),
        doc.word_count(),
        end.saturating_sub(1),
    );
    for (n, block) in (start..end).zip(doc.blocks_from(start)) {
        out.push_str(&text::paragraph(n, &block, formatting));
        out.push('\n');
    }
    if end < total {
        out.push_str(&format!("(more: read on with start {end})\n"));
    }
    Ok(out)
}

fn open_for_reading(id: &str) -> Result<Document, String> {
    let item = find_item(id)?;
    if item.kind == ItemKind::Folder {
        return Err(format!(
            "{:?} is a folder; list_binder shows what is in it",
            item.title
        ));
    }
    project::open(id).ok_or_else(|| format!("Prose couldn't read {:?}", item.title))
}

fn read_document(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let doc = open_for_reading(&id)?;
    page(&doc, args, "Document")
}

fn get_selection(_: &Args) -> Result<String, String> {
    let id = workspace::active_document().ok_or("No document is open in the active pane")?;
    let doc = project::open(&id).ok_or("Prose couldn't read the active document")?;
    let Some(selection) = doc.selection() else {
        return pretty(json!({"document": id, "title": doc.title(), "selection": null}));
    };
    let span = selection.span_in(&doc);
    let first = doc.block_index(&span.start.block).unwrap_or(0);
    let last = doc.block_index(&span.end.block).unwrap_or(first);
    let mut selected = String::new();
    for (n, block) in (first..=last).zip(doc.blocks_from(first)) {
        let from = if n == first {
            span.start.offset as usize
        } else {
            0
        };
        let to = if n == last {
            span.end.offset as usize
        } else {
            block.text.len()
        };
        if n > first {
            selected.push('\n');
        }
        selected.push_str(block.text.get(from..to).unwrap_or_default());
    }
    pretty(json!({
        "document": id,
        "title": doc.title(),
        "version": doc.version(),
        "first_paragraph": first,
        "last_paragraph": last,
        "caret_only": selection.is_empty(),
        "selected_text": selected,
    }))
}

fn search(args: &Args) -> Result<String, String> {
    let mut query = Query::new(required(args, "text")?);
    query.regex = bool_arg(args, "regex")?.unwrap_or(false);
    query.case_sensitive = bool_arg(args, "case_sensitive")?.unwrap_or(false);
    query.whole_word = bool_arg(args, "whole_word")?.unwrap_or(false);
    for place in list_arg(args, "places")?.unwrap_or_default() {
        query.places.push(match place.as_str() {
            "title" => Place::Title,
            "synopsis" => Place::Synopsis,
            "text" => Place::Text,
            "footnote" => Place::Footnote,
            "comment" => Place::Comment,
            other => return Err(format!("{other:?} isn't a place to search")),
        });
    }
    for folder in list_arg(args, "folders")?.unwrap_or_default() {
        query.folders.push(
            text::parse_folder(&folder)
                .ok_or_else(|| format!("{folder:?} isn't a top-level folder"))?,
        );
    }
    let limit = number_arg(args, "limit")?
        .unwrap_or(50)
        .clamp(1, MAX_HITS as u64) as u32;
    let hits = project::search(&query, limit).map_err(|e| match e {
        SearchError::BadQuery(why) => format!("The regular expression doesn't parse: {why}"),
        SearchError::NoProject => "No project is open in Prose".into(),
    })?;
    let mut titles: std::collections::HashMap<String, String> = Default::default();
    let hits: Vec<Value> = hits
        .into_iter()
        .map(|hit| {
            let title = titles
                .entry(hit.document.clone())
                .or_insert_with(|| {
                    project::find(&hit.document)
                        .map(|i| i.title)
                        .unwrap_or_default()
                })
                .clone();
            json!({
                "document": hit.document,
                "title": title,
                "place": format!("{:?}", hit.place).to_lowercase(),
                "paragraph": hit.paragraph,
                "snippet": hit.snippet,
            })
        })
        .collect();
    let full = hits.len() as u32 == limit;
    pretty(json!({"hits": hits, "stopped_at_limit": full}))
}

fn writing_stats(args: &Args) -> Result<String, String> {
    let days = number_arg(args, "days")?.unwrap_or(30).max(1) as usize;
    let all = project::daily_words();
    let recent: Vec<Value> = all
        .iter()
        .skip(all.len().saturating_sub(days))
        .map(|d| json!({"date": d.date, "words": d.words}))
        .collect();
    pretty(json!({
        "session_words": project::session_words(),
        "session_target": project::session_target(),
        "days": recent,
    }))
}

fn revision_json(r: &history::Revision) -> Value {
    json!({
        "id": r.id,
        "message": r.message,
        "kind": format!("{:?}", r.kind).to_lowercase(),
        "author": r.author,
        "time": text::time(r.time, r.utc_offset),
    })
}

fn list_history(args: &Args) -> Result<String, String> {
    let count = number_arg(args, "count")?.unwrap_or(20).clamp(1, 200) as u32;
    match text_arg(args, "document")? {
        Some(id) => {
            let item = find_item(&id)?;
            let versions: Vec<Value> = history::versions(&id, 0, count)
                .iter()
                .map(|v| {
                    let mut value = revision_json(&v.revision);
                    value["change"] = format!("{:?}", v.change).to_lowercase().into();
                    value
                })
                .collect();
            let snapshots: Vec<Value> = history::snapshots(&id)
                .iter()
                .map(|s| {
                    json!({"number": s.number, "title": s.title, "revision": s.revision,
                        "time": text::time(s.time, s.utc_offset)})
                })
                .collect();
            pretty(
                json!({"document": id, "title": item.title, "versions": versions, "snapshots": snapshots}),
            )
        }
        None => {
            let drafts: Vec<Value> = history::drafts()
                .iter()
                .map(|d| json!({"id": d.id, "title": d.title, "current": d.current, "set_aside": d.set_aside}))
                .collect();
            let revisions: Vec<Value> = history::revisions(None, 0, count)
                .iter()
                .map(revision_json)
                .collect();
            pretty(json!({"drafts": drafts, "revisions": revisions}))
        }
    }
}

fn read_revision(args: &Args) -> Result<String, String> {
    let id = required(args, "document")?;
    let revision = required(args, "revision")?;
    let doc = history::read(&id, &revision)
        .ok_or("That document didn't exist in that revision, or there is no such revision")?;
    page(&doc, args, &format!("Revision {revision} of"))
}

fn placement(name: Option<&str>) -> Result<Placement, String> {
    Ok(match name {
        None | Some("active-pane") => Placement::ActivePane,
        Some("split-right") => Placement::SplitRight,
        Some("split-down") => Placement::SplitDown,
        Some(other) => return Err(format!("{other:?} isn't a place to open it")),
    })
}

fn open_error(error: OpenError, title: &str) -> String {
    match error {
        OpenError::NoSuchDocument => format!("Prose has no document {title:?}"),
        OpenError::Unsaved => format!(
            "The writer's active pane has unsaved changes, so Prose didn't open {title:?} over it. \
             Ask them to save, or open it split-right"
        ),
    }
}

fn open_document(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let item = find_item(&id)?;
    if item.kind == ItemKind::Folder {
        return Err(format!("{:?} is a folder", item.title));
    }
    let placement = placement(text_arg(args, "where")?.as_deref())?;
    workspace::open(&id, placement).map_err(|e| open_error(e, &item.title))?;
    Ok(format!("Showing {:?} in Prose", item.title))
}

// Changing.

/// Document `id` in a pane, for editing: opened (where `open_in` says) if
/// no pane shows it, waiting for Prose to read it.
fn editable(id: &str) -> Result<Document, String> {
    let item = find_item(id)?;
    if item.kind == ItemKind::Folder {
        return Err(format!("{:?} is a folder", item.title));
    }
    let shown = |id: &str| workspace::open_documents().iter().any(|d| d.id == id);
    if !shown(id) {
        let where_ = settings::text("open_in");
        let placement = placement(where_.as_deref()).unwrap_or(Placement::ActivePane);
        workspace::open(id, placement).map_err(|e| open_error(e, &item.title))?;
        let started = Instant::now();
        while !shown(id) {
            if started.elapsed() > OPENING {
                return Err(format!(
                    "Prose is still opening {:?}; try again in a moment",
                    item.title
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    project::open(id).ok_or_else(|| format!("Prose couldn't read {:?}", item.title))
}

fn edit_failed(error: EditError) -> String {
    match error {
        EditError::Stale => {
            "The writer changed the document while you were editing; read it again and redo the edit"
                .into()
        }
        EditError::InvalidOp(op) => format!("Prose refused edit {}: {}", op.index, op.message),
        EditError::Gone => "The document was closed or deleted".into(),
    }
}

fn edit_document(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let changes: Vec<Change> = match args.get("edits") {
        Some(Value::Array(edits)) if !edits.is_empty() => edits
            .iter()
            .enumerate()
            .map(|(n, e)| Change::parse(e).map_err(|why| format!("edit {n}: {why}")))
            .collect::<Result<_, _>>()?,
        _ => return Err("edits must be a list of at least one edit".into()),
    };
    let version = number_arg(args, "version")?;
    let doc = editable(&id)?;
    if let Some(version) = version
        && version != doc.version()
    {
        return Err(format!(
            "The document changed since you read it (it's version {} now, you read {version}); \
             read it again",
            doc.version()
        ));
    }
    let blocks = edit::blocks(&doc);
    let (ops, summary) = edit::build(&blocks, &changes, prose_plugin::new_id)?;
    let mut built = doc.edit();
    for op in ops {
        built.push(op);
    }
    let new_version = built.apply().map_err(edit_failed)?;
    Ok(format!(
        "Edited {:?}: {}. It's version {new_version} now; the writer can undo this with u.",
        doc.title(),
        summary.describe()
    ))
}

fn binder_failed(error: BinderError) -> String {
    match error {
        BinderError::NoSuchItem(id) => format!("No item has the id {id:?}"),
        BinderError::Refused(why) => format!("Prose refused: {why}"),
        BinderError::ReadOnly => "The project is read only (or no project is open)".into(),
        BinderError::Busy => "Prose is saving another binder change; try again in a moment".into(),
        BinderError::Failed(why) => format!("Writing it failed: {why}"),
    }
}

fn create_document(args: &Args) -> Result<String, String> {
    let title = required(args, "title")?;
    let parent = match text_arg(args, "parent")? {
        Some(parent) => parent,
        None => project::root(RootFolder::Manuscript).ok_or("No project is open in Prose")?,
    };
    let index = number_arg(args, "index")?.map(|n| n.min(u32::MAX as u64) as u32);
    let folder = bool_arg(args, "folder")?.unwrap_or(false);
    let kind = if folder {
        ItemKind::Folder
    } else {
        ItemKind::Document
    };
    let text = text_arg(args, "text")?;
    if folder && text.is_some() {
        return Err("A folder has no text".into());
    }
    let id = binder::create(&parent, index, kind, &title).map_err(binder_failed)?;
    if let Some(synopsis) = text_arg(args, "synopsis")? {
        binder::set_meta(&id, &[MetaChange::Synopsis(synopsis)]).map_err(binder_failed)?;
    }
    let what = if folder { "folder" } else { "document" };
    if let Some(text) = text.filter(|t| !t.is_empty()) {
        let doc = editable(&id)?;
        let blocks = edit::blocks(&doc);
        let mut built = doc.edit();
        match (blocks.first(), blocks.last()) {
            (Some(first), Some(last)) => {
                let span = prose_plugin::Span::new(
                    prose_plugin::Position::new(&first.id, 0),
                    prose_plugin::Position::new(&last.id, last.text.len() as u32),
                );
                built.replace_text(span, text);
            }
            _ => {
                built.insert_blocks(
                    None,
                    text.split('\n')
                        .map(prose_plugin::NewBlock::paragraph)
                        .collect(),
                );
            }
        }
        built.apply().map_err(edit_failed)?;
    }
    Ok(format!("Made the {what} {title:?}, id {id}"))
}

/// A label or status by id or title, `None` for "".
fn definition(
    given: &str,
    list: &[project::Definition],
    what: &str,
) -> Result<Option<String>, String> {
    if given.is_empty() {
        return Ok(None);
    }
    list.iter()
        .find(|d| d.id == given || d.title.eq_ignore_ascii_case(given))
        .map(|d| Some(d.id.clone()))
        .ok_or_else(|| {
            let known: Vec<&str> = list.iter().map(|d| d.title.as_str()).collect();
            format!(
                "There is no {what} {given:?}; the project has {}",
                known.join(", ")
            )
        })
}

fn update_item(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let item = find_item(&id)?;
    let mut changes = Vec::new();
    if let Some(synopsis) = text_arg(args, "synopsis")? {
        changes.push(MetaChange::Synopsis(synopsis));
    }
    if let Some(label) = text_arg(args, "label")? {
        changes.push(MetaChange::Label(definition(
            &label,
            &project::labels(),
            "label",
        )?));
    }
    if let Some(status) = text_arg(args, "status")? {
        changes.push(MetaChange::Status(definition(
            &status,
            &project::statuses(),
            "status",
        )?));
    }
    if let Some(keywords) = list_arg(args, "keywords")? {
        changes.push(MetaChange::Keywords(keywords));
    }
    match args.get("target_words") {
        None => {}
        Some(Value::Null) => changes.push(MetaChange::TargetWords(None)),
        Some(value) => changes.push(MetaChange::TargetWords(Some(
            value
                .as_u64()
                .ok_or("target_words must be a whole number or null")?,
        ))),
    }
    if let Some(include) = bool_arg(args, "include_in_compile")? {
        changes.push(MetaChange::IncludeInCompile(include));
    }
    if let Some(custom) = args.get("custom") {
        let Value::Object(custom) = custom else {
            return Err("custom must be an object of field id to value".into());
        };
        let fields = project::fields();
        for (field, value) in custom {
            let def = fields
                .iter()
                .find(|f| &f.id == field || f.title.eq_ignore_ascii_case(field))
                .ok_or_else(|| format!("There is no custom field {field:?}"))?;
            let value = match (value, def.kind) {
                (Value::Null, _) => None,
                (Value::Bool(b), _) => Some(project::FieldValue::Flag(*b)),
                (Value::String(s), project::FieldKind::Date) => {
                    Some(project::FieldValue::Date(s.clone()))
                }
                (Value::String(s), _) => Some(project::FieldValue::Text(s.clone())),
                _ => return Err(format!("{field} must be text, true/false, a date or null")),
            };
            changes.push(MetaChange::Custom(binder::CustomChange {
                field: def.id.clone(),
                value,
            }));
        }
    }
    let title = text_arg(args, "title")?;
    if changes.is_empty() && title.is_none() {
        return Err(
            "Nothing to change: give a title, synopsis, label, status, keywords, \
                    target_words, include_in_compile or custom"
                .into(),
        );
    }
    if !changes.is_empty() {
        binder::set_meta(&id, &changes).map_err(binder_failed)?;
    }
    if let Some(title) = &title {
        binder::retitle(&id, title).map_err(binder_failed)?;
    }
    Ok(format!(
        "Changed {:?}{}",
        item.title,
        title
            .map(|t| format!(", now titled {t:?}"))
            .unwrap_or_default()
    ))
}

fn move_item(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let parent = required(args, "parent")?;
    let item = find_item(&id)?;
    let folder = find_item(&parent)?;
    let index = number_arg(args, "index")?.map(|n| n.min(u32::MAX as u64) as u32);
    binder::move_(&id, &parent, index).map_err(binder_failed)?;
    Ok(format!("Moved {:?} into {:?}", item.title, folder.title))
}

fn trash_item(args: &Args) -> Result<String, String> {
    let id = required(args, "id")?;
    let item = find_item(&id)?;
    binder::trash(&id).map_err(binder_failed)?;
    Ok(format!(
        "Moved {:?} to the trash; the writer can take it back",
        item.title
    ))
}

fn replace_in_project(args: &Args) -> Result<String, String> {
    let mut query = Query::new(required(args, "find")?);
    let replacement = required(args, "replace")?;
    query.regex = bool_arg(args, "regex")?.unwrap_or(false);
    query.case_sensitive = bool_arg(args, "case_sensitive")?.unwrap_or(false);
    query.whole_word = bool_arg(args, "whole_word")?.unwrap_or(false);
    for folder in list_arg(args, "folders")?.unwrap_or_default() {
        query.folders.push(
            text::parse_folder(&folder)
                .ok_or_else(|| format!("{folder:?} isn't a top-level folder"))?,
        );
    }
    let count = binder::replace(&query, &replacement).map_err(binder_failed)?;
    Ok(match count {
        0 => "Nothing matched; nothing changed".into(),
        1 => "Replaced 1 match; the writer can undo it from the binder".into(),
        n => format!("Replaced {n} matches as one change; the writer can undo it from the binder"),
    })
}
