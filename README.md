# Prose plugin API

Everything you need to write a plugin for Prose, the writing app: the API, written in [WIT](https://component-model.bytecodealliance.org/design/wit.html), a Rust SDK, a project template and example plugins.

A Prose plugin is a WebAssembly component plus a `plugin.toml`. Because the API is WIT, a plugin can be written in any language that builds components (Rust, C, Go, JavaScript, Python, C#). This repository's SDK is for Rust; for other languages, generate bindings from [`wit/v0.1.0`](wit/v0.1.0) with your language's tool.

Prose's own source is not here. The WIT is the whole contract between Prose and a plugin.

> **Status:** API `0.1.0` is a draft. Prose's development builds load plugins; the API may still change until the first Prose release with plugin support, when it is tagged `v0.1.0` and frozen.

## What a plugin can do (0.1)

| Interface | What it gives | Permission |
|---|---|---|
| `commands` | Add commands. They show in the `:` line, the command palette and the Shortcuts settings, and writers bind them in `keymap.toml` as `<plugin id>.<command>`. | none |
| `documents` | Read the document a command was run on: its blocks (text, formatting, kind, annotations), a page at a time, its selection and word count, and UTF-16 offsets for languages that count strings that way. | none |
| `edits` | Change that document. Everything one command or event applies is one undo step, so `u` undoes a whole plugin action. | `edit-documents` |
| `ui` | Status messages, notifications with buttons, text and choice prompts, pickers (a list the writer filters by typing, drawn by the command palette or, with previews, quick open's finder), a log. | none |
| `settings` | The plugin's own `[plugins.<id>]` table from the writer's `settings.toml`. | none |
| `workspace` | The open project's name, the documents open in panes and the active one, and opening a document in a pane or a split. | none |
| `project` | The whole project: the binder and every item's metadata (synopsis, label, status, keywords, target, custom fields) and notes, any document's text, project search, word counts, the session's words and the words written each day. | `read-project` |
| `history` | The project's drafts and revisions, a document's versions and snapshots, and a document as any of them kept it. Read only. | `read-project` |
| `binder` | Making, retitling, moving, duplicating and trashing binder items, setting their metadata and custom fields, and replacing text across the project. Each call is one change the writer can undo from the binder; nothing empties the trash. | `edit-binder` |
| `lint` (an export) | Check paragraphs: what the plugin finds is underlined and offered in the same menu as Prose's own spelling and grammar problems. | none |
| `compile` (exported) | Hooks Prose calls: change the manuscript every compile makes, write a new export format, read a new import format. See [Compile and import hooks](#compile-and-import-hooks). | `compile` |
| `serve` (exported) | Answer apps on this computer, such as an AI assistant speaking MCP: Prose listens on 127.0.0.1 and hands the plugin each message. See [Answering apps on this computer](#answering-apps-on-this-computer-mcp). | `serve` |
| `decorations` | Highlights, underlines (straight or wavy), marks in the margin and hints after a line, over a document's text, in the writer's theme. Each plugin replaces or clears its own set per document; they move with edits and are never saved. | none |

Prose calls three functions the plugin exports: `activate` (register commands), `run-command`, and `on-event` (for the events the plugin's `plugin.toml` lists: document opened, saved or changed, idle, timers and so on). A plugin may export more, for what only some plugins do: the `linter` world adds the `lint` interface (below), and Prose calls it only on a plugin that exports it.

Plugins never draw their own UI. They describe it, and Prose draws it in the writer's theme, the same on desktop and phone. Later API versions add panels.

### Permissions

A plugin asks for permissions in `plugin.toml`, and the writer grants them when installing it. Prose links only the interfaces a plugin was granted, so a plugin that imports `edits` without `edit-documents` (or `project` without `read-project`, or `binder` without `edit-binder`) doesn't load at all. No permission reaches git remotes, credentials, Prose's license, other plugins' data, `settings.toml` beyond the plugin's own table, or the shell: the API takes binder ids and revision ids, never paths. A component only imports the interfaces it actually calls, so a plugin that never edits doesn't need the permission.

`serve` is shown to the writer as a warning, like a network site: an app the plugin answers, such as an AI assistant, may send what it reads to its company's servers.

Plugins run in a sandbox: no files, no network, no clock beyond WASI's monotonic clock, and no other process. Prose stops a call that runs too long and turns off a plugin that keeps failing.

## Writing a plugin

This walks through a plugin from an empty folder to running it in Prose. It uses the Rust SDK; the steps after "Build it" are the same in any language.

### 1. Set up

You need Rust 1.90 or newer and the WebAssembly target:

```sh
rustup target add wasm32-wasip2
```

### 2. Start from the template

With [cargo-generate](https://github.com/cargo-generate/cargo-generate):

```sh
cargo install cargo-generate
cargo generate --git https://github.com/NickSpinosa/prose-plugin-api template --name my-plugin
cd my-plugin
```

It asks for the name writers see, the `:` name of its first command (a capital letter first, like `WordCount`) and a one-line description, and gives you:

| File | What |
|---|---|
| `plugin.toml` | What Prose reads before it runs anything: id, name, version, permissions, events, settings ([below](#plugintoml)) |
| `src/lib.rs` | The plugin: one command that says how many words the document has |
| `.cargo/config.toml` | Builds for `wasm32-wasip2` |

To start without the template, make a `cdylib` crate that depends on the SDK:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
prose-plugin = { git = "https://github.com/NickSpinosa/prose-plugin-api", branch = "main" }
```

### 3. Add commands

A plugin is a type implementing `Plugin`, exported with `export!`. Prose calls `activate` once when the plugin loads, where it registers its commands, and `run_command` when the writer runs one:

```rust
use prose_plugin::{Command, Context, Plugin, ui};

#[derive(Default)]
struct Hello;

impl Plugin for Hello {
    fn activate(&mut self) -> prose_plugin::Result {
        Command::new("hello", "Say hello").ex_name("Hello").register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, _args: Vec<String>, cx: Context) -> prose_plugin::Result {
        let doc = cx.document().ok_or("Open a document first")?;
        ui::info(&format!("Hello from {}", doc.title()));
        Ok(())
    }
}

prose_plugin::export!(Hello);
```

A command has two names. `hello` is its id: writers bind keys to it in `keymap.toml` as `<plugin id>.hello`. `Hello` is what they type on the `:` line, and it must start with a capital letter so it never clashes with Prose's own commands. Add `.argument(Argument::Text)` for a command that takes words after it (`:WordFreq 20`), and `.suggested_keys("secondary-k secondary-w")` to offer keys, which Prose uses only when nothing else has them and the writer's `keymap.toml` always overrides. `cx.count` is the count typed before it in vim mode, if any.

What `run_command` returns is what the writer sees: `Ok` shows nothing more, and `Err(text)` shows `text` as an error on the message line. Use `ui::info`, `ui::warn` and `ui::error` to say anything else, and `ui::confirm` to ask a yes-or-no question (the deadline below doesn't run while the writer thinks).

### 4. Read the document

`cx.document()` is the document the command was run on. `blocks()` reads it a page of blocks at a time, so a long book is never copied all at once:

```rust
let mut words = 0;
for block in doc.blocks() {
    words += block.text.split_whitespace().count();
}
```

A block has an `id`, a `kind` (paragraph, heading, quote, …), its `text`, and the runs and annotations on it. Offsets are bytes into `text`; `doc.utf16_offset` and `doc.byte_offset` convert for languages that count UTF-16. `doc.selection_span()` is the selection, if there is one.

### 5. Edit it

Changing the text needs the `edit-documents` permission, so add it to `plugin.toml`:

```toml
permissions = ["edit-documents"]
```

Build ops against the version of the document you read, then apply them. Everything one command applies is one undo step, so `u` undoes the whole command. If the writer typed while your plugin worked, Prose refuses the edit rather than guessing; `edit_with_retry` reads again and calls your builder again:

```rust
doc.edit_with_retry(|doc, edit| {
    for block in doc.blocks() {
        if let Some(at) = block.text.find("  ") {
            edit.replace(&block.id, at as u32, at as u32 + 2, " ");
        }
    }
})
.map_err(|e| e.to_string())?;
```

Ops apply in order, each to the document as the earlier ones left it, so when you make several changes in one block, go from its end to its start. An empty edit changes nothing and makes no undo step. [`examples/word-freq`](examples/word-freq)'s `:WordFreqFix` is a whole command built this way.

### 6. Give it settings

Declare each setting in `plugin.toml` with a kind and a default; Prose shows them on its settings screen and the writer sets them under `[plugins.<id>]` in `settings.toml`. Read them with `prose_plugin::settings`:

```toml
[settings.min_length]
kind = "integer"
default = 3
min = 1
help = "Words shorter than this are left out."
```

```rust
let min = prose_plugin::settings::integer("min_length").unwrap_or(3);
```

Prose fills in your defaults, but keep a fallback in code: a test host may not.

### 7. Build it

```sh
cargo build --release
```

The component is `target/wasm32-wasip2/release/my_plugin.wasm`. A release build of a small plugin is about 100 KB.

### 8. Try it in Prose

An installed plugin is a folder holding `plugin.toml` and the component renamed `plugin.wasm`. Make one and install it:

```sh
mkdir -p dist/my-plugin
cp plugin.toml dist/my-plugin/
cp target/wasm32-wasip2/release/my_plugin.wasm dist/my-plugin/plugin.wasm
```

then in Prose run `:PluginInstall /path/to/dist/my-plugin` (or Settings > Plugins > Install from File… with the folder zipped as `my-plugin.prose-plugin`). Prose checks the manifest and the component's imports, asks the writer for any permissions, copies the folder into `plugins/my-plugin/` beside `settings.toml`, and turns it on. Your commands are then on the `:` line, in the command palette (the Plugins menu) and in the Shortcuts settings.

After a rebuild, run `:PluginInstall` again: Prose sees the new `plugin.wasm` and reloads the plugin without a restart. `:PluginDisable my-plugin` turns it off and `:PluginRemove my-plugin` deletes it.

### 9. Test it without Prose

Keep the logic in plain functions and unit-test them with `cargo test` on your own machine; only the calls into Prose need the component. To run the component itself, copy [`tests/host`](tests/host): a fake Prose on Wasmtime that loads a `.wasm`, gives it a document and settings, runs commands and records what it said, what it changed and how many undo steps it made. This repository's own tests (`tests/host/tests/examples.rs`) run every example that way.

### 10. Know the limits

Prose runs each plugin on its own thread, in a sandbox:

- **Time**: a command or `activate` may run 5 s, an event or a lint call 50 ms. A call that runs longer is stopped, and the plugin is restarted (activated again).
- **Memory**: 64 MB over all of the plugin's memories.
- **Crashes**: a panic, a trap or an overrun is a crash; the writer sees "<name> crashed (…) and was restarted". After three, the plugin is off until Prose restarts. An `Err` from `run_command` is not a crash.
- **Nothing else**: no files (unless granted `files`, and then only its own `data` folder), no network, no processes, no environment. Standard output and errors go to Prose's log, not the screen, so show what the writer needs with `ui`.

Prose's own costs are small: loading a plugin the first time compiles it (well under 100 ms for one this size), later loads come from a compile cache (under 10 ms), and a command's round trip is under 5 ms plus your plugin's own work.

### 11. Share it

- **A file**: zip the folder (`plugin.toml` and `plugin.wasm`) and rename it `<id>.prose-plugin`.
- **A git repository**: commit `plugin.toml` and the built `plugin.wasm` at the top of the repository on a release tag (`v1.0.0`). Writers install it with `:PluginInstall github.com/you/my-plugin` and update it with `:PluginUpdate`, which takes your newest release tag.

Bump `version` in `plugin.toml` with every release, and keep `api` at the WIT version you built against.

## More the API can do

### Checking text

A plugin that checks writing (filter words, adverbs, repeated words, house style) implements `Lint` too and exports it with `lint`. Prose hands it each paragraph on screen, again once typing in it pauses, and keeps the answer until the paragraph changes, so the answer should depend only on the text and the plugin's settings. What it returns is drawn with Prose's spelling or grammar squiggles and offered in the same menu, with its suggestions as the fixes:

```rust
use prose_plugin::{Context, Diagnostic, Lint, Plugin};

#[derive(Default)]
struct NoVery;

impl Plugin for NoVery {
    fn run_command(&mut self, name: &str, _: Vec<String>, _: Context) -> prose_plugin::Result {
        Err(format!("no command {name}"))
    }
}

impl Lint for NoVery {
    fn lint(&mut self, paragraph: &str) -> Vec<Diagnostic> {
        paragraph
            .match_indices("very ")
            .map(|(at, s)| Diagnostic::style(at, at + s.len(), "very", "Try a stronger word").suggest(""))
            .collect()
    }
}

prose_plugin::export!(NoVery, lint);
```

Ranges are byte offsets into the paragraph; an empty suggestion deletes the range. Each call runs under the event deadline (50 ms by default). The writer turns a plugin's checks off with `lint = false` in its `[plugins.<id>]` table, or from the menu on one of its squiggles.

### Decorations

Decorations mark up the text without changing it. Collect them and show them; each call replaces the plugin's decorations on that document:

```rust
use prose_plugin::decorations::{Color, Decorations};

let mut marks = Decorations::new();
for block in doc.blocks() {
    for (at, _) in block.text.match_indices("suddenly") {
        let span = Span::in_block(&block.id, at as u32, (at + 8) as u32);
        marks.underline(span, Color::Warning, true);
    }
}
marks.show(&doc).map_err(|e| e.to_string())?;
```

Like edits, they are placed against the version you read, and refused as stale if the writer typed since. They follow the writer's edits after that, and are gone when the document closes.

### Pickers

A picker is a list the writer filters by typing, with the command palette's fuzzy matching and keys (vim's normal mode too). Each item has a title, an optional detail beside it, and an optional preview: give any item a preview and Prose draws the picker as quick open's finder, with the selected item's preview beside the list. A long list can stream in while the writer looks:

```rust
use prose_plugin::ui::{self, PickItem};

let picker = ui::open_picker("Pick a name", &first_batch);
for batch in rest {
    if !picker.add(&batch) {
        break; // the writer already chose, or closed it
    }
}
if let Some(picked) = picker.wait() {
    // picked.index counts every item added; picked.how is Enter, or
    // Ctrl-V / Ctrl-X for "in a split"
}
```

`ui::pick(title, items)` does it in one call for a short list. Waiting for the writer doesn't count against the command's deadline; building the list does.

### Examples

- [`examples/word-freq`](examples/word-freq): `:WordFreq [n]` lists the most used words in the document, and `:WordFreqFix` deletes doubled words ("the the") as one undo step, leaving the ones in its `allow_doubled` setting ("had had"). `:WordFreqMark [n]` highlights every use of the top words with a mark in the margin, and `:WordFreqClear` takes the highlights away. Needs `edit-documents`.
- [`examples/smart-quotes`](examples/smart-quotes): `:SmartQuotes` curls straight quotes in the selection or the whole document, as one undo step, and curls every compile's quotes as a compile transform. Needs `edit-documents` and `compile`.
- [`examples/filter-words`](examples/filter-words): underlines filter words ("just", "really", "suddenly") and offers to cut them, leaving dialogue alone unless asked. Exports `lint`; the words are a setting.
- [`examples/names`](examples/names): `:Name` streams 2,000 generated character names into a picker, each with its origin and a line of dialogue as its preview, and puts the chosen one at the caret. Needs `edit-documents`.
- [`examples/mcp`](examples/mcp): an [MCP](https://modelcontextprotocol.io) server, so an AI assistant on the writer's computer (Claude Desktop, Claude Code, or any MCP client) can read the project (binder, documents, search, history, stats) and, unless the writer sets `read_only`, edit it: find and replace, rewrite, insert and remove paragraphs, add and retitle documents, set metadata, trash, replace across the project. Every change is one undo step in Prose. Needs `serve`, `read-project`, `edit-documents` and `edit-binder`. [Set it up](#answering-apps-on-this-computer-mcp).
- [`examples/fountain`](examples/fountain): compiles the book to a [Fountain](https://fountain.io) screenplay and imports `.fountain` files, a document per top-level section. Needs `compile`.

### Compile and import hooks

A plugin can hook into Compile and File > Import by exporting the `compile` interface (the `compile-plugin` world) and implementing [`Compile`](crates/prose-plugin/src/compile.rs) beside `Plugin`:

```rust
impl Compile for Fountain {
    fn export_format(&mut self, format: &str, info: CompileInfo, manuscript: Document,
                     notes: Vec<Note>, layout: Layout) -> prose_plugin::Result<Vec<u8>> {
        // Read manuscript.blocks() and return the file's bytes.
    }
}

prose_plugin::export!(Fountain, compile);
```

- **`transform`** gets the whole manuscript as Compile assembled it, after the preset's replacements, as a read-only `Document`, and returns ops to apply to it (built as for an edit). It runs on every compile.
- **`export-format`** gets the manuscript, its footnotes and the preset's layout (page, body text, paragraphs laid out apart) and returns the bytes of the file. A preset writes it with `format = "<plugin id>.<format name>"`.
- **`import-format`** gets the bytes and name of a file the writer imports and returns the documents to add, each with a title, synopsis and blocks.

None of them reach the disk: Prose reads and writes the files. Prose calls only what `plugin.toml` names (below), the plugin needs the `compile` permission, and Compile's progress and Cancel cover the calls.

### Answering apps on this computer (MCP)

A plugin can be a server for apps on the writer's computer by exporting the `serve` interface (the `server` world) and implementing [`Serve`](crates/prose-plugin/src/serve.rs):

```rust
impl Serve for Mcp {
    fn handle(&mut self, message: &str) -> Option<String> {
        // A JSON-RPC message in; the reply out, or None for a notification.
    }
}

prose_plugin::export!(Mcp, serve);
```

```toml
permissions = ["serve", "read-project"]
port = 7767          # Prose listens on 127.0.0.1:7767; the writer can change it
```

The plugin never touches a socket. Prose listens on 127.0.0.1 only, while the plugin is on, speaking MCP's Streamable HTTP (`POST /mcp`, answered with JSON). It keeps a key for the plugin in `plugins/<id>/serve.toml`, beside the plugin's folder and out of its reach, and turns away any request without `Authorization: Bearer <key>`, any from a web page (one with an `Origin` header) and any addressed to another host name. Each message is queued like a command and runs under the command deadline, with the API the plugin's other permissions give; what it changes is one undo step.

**Connecting an assistant to the MCP example.** Install it in Prose (build it, then `:PluginInstall` its folder, or a `.prose-plugin` zip of `plugin.toml` and `plugin.wasm`) and allow what it asks. Then point the assistant at Prose's own executable with `--mcp`, which passes messages between the assistant and the running Prose:

- Claude Desktop, in `claude_desktop_config.json`:
  ```json
  {"mcpServers": {"prose": {"command": "C:\\Users\\you\\AppData\\Local\\Prose\\current\\prose.exe", "args": ["--mcp"]}}}
  ```
  (Prose's installer puts it in `%LOCALAPPDATA%\Prose\current`; on a Mac, use `"/Applications/Prose.app/Contents/MacOS/prose"`).
- Claude Code: `claude mcp add prose -- "<path to prose>" --mcp`.
- A client that speaks HTTP: the `url` in `plugins/mcp/serve.toml`, with its `key` as a bearer token.

Prose must be running with a project open. In settings.toml, `[plugins.mcp] read_only = true` leaves the assistant only the tools that read, `open_in` says where a document it edits is opened (it edits through a pane, so you see each change and `u` undoes it), and `port` moves the server.

## plugin.toml

```toml
id = "word-freq"            # lowercase letter, then lowercase letters, digits, - and _ (64 at most)
name = "WordFreq"
version = "0.1.0"
api = "0.1.0"               # the WIT version it was built against
description = "Lists the most used words in a document."
authors = ["You"]
license = "MIT OR Apache-2.0"
homepage = "https://example.com"   # optional
permissions = []            # e.g. ["edit-documents"]
events = []                 # e.g. ["document-saved", "idle", "timer"]
# timer_seconds = 60        # with the "timer" event, at least 10
# port = 7767               # with the "serve" permission: where Prose listens, 1024 or more

[settings.min_length]       # one table per setting, shown on Prose's settings screen
title = "Shortest word"     # optional
kind = "integer"            # boolean, integer, float, text, text-list or choice
default = 3
min = 1                     # optional, for integer and float
max = 20
help = "Words shorter than this are left out."
```

A plugin with compile hooks names them:

```toml
permissions = ["compile"]
compile-transform = true    # Prose calls `transform` on every compile

[[export-format]]           # a preset writes it with format = "fountain.fountain"
name = "fountain"           # lowercase letters, digits, - and _
title = "Fountain screenplay"
extension = "fountain"      # of the file compile writes

[[import-format]]           # files with these extensions import through it
name = "fountain"
title = "Fountain screenplay"
extensions = ["fountain"]
```

A `choice` setting lists its `options = [...]`. `grants` and `lint` are Prose's own keys in the plugin's table, so no setting may be called that. A plugin installed in Prose is a folder holding `plugin.toml` and the built component renamed `plugin.wasm`.

## Writers' settings and keys

```toml
# settings.toml
[plugins]
enabled = ["word-freq"]

[plugins.word-freq]
grants = ["edit-documents"]
min_length = 4

[plugins.filter-words]
lint = false                # its squiggles off; true or no key is on

# keymap.toml
[app]
"word-freq.top_words" = "secondary-k secondary-f"

[normal]
"<leader>wf" = ":WordFreq"
```

A plugin can suggest keys for its commands, but the writer's `keymap.toml` always wins.

## Other languages

Generate bindings from `wit/v0.1.0`, world `prose:plugin/plugin@0.1.0`, or `prose:plugin/linter@0.1.0` for a plugin that checks text, or `prose:plugin/compile-plugin@0.1.0` for one with compile hooks:

| Language | Tool |
|---|---|
| C, C++ | `wit-bindgen c` with wasi-sdk |
| Go | `wit-bindgen-go` (TinyGo or Go's `wasip2` port) |
| JavaScript, TypeScript | `componentize-js` / `jco` |
| Python | `componentize-py` |
| C# | `componentize-dotnet` |

JavaScript and Python components carry their interpreter, so they are a few MB and start slower than Rust, C or Go ones, but they work the same.

## Versions

The API is versioned as a WIT package, `prose:plugin@<version>`, with one folder per version under `wit/`. A released version never changes. Prose loads plugins built for any version it supports, so a plugin built for `0.1.0` keeps working when `0.2.0` ships.

## This repository

| Path | What |
|---|---|
| `wit/v0.1.0/` | The API |
| `crates/prose-plugin/` | The Rust SDK |
| `template/` | The `cargo generate` template |
| `examples/` | Example plugins |
| `tests/host/` | A fake Prose that loads the examples in Wasmtime and checks what they do |

```sh
cargo test                  # the SDK and examples' unit tests
cargo test -p host-test     # build the examples to wasm and run them in Wasmtime (builds Wasmtime once, a few minutes)
wasm-tools component wit wit/v0.1.0   # check the WIT parses
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option. Plugins you write with it may use any license you like.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this repository by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
