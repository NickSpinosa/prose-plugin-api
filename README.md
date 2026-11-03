# Prose plugin API

Everything you need to write a plugin for Prose, the writing app: the API, written in [WIT](https://component-model.bytecodealliance.org/design/wit.html), a Rust SDK, a project template and three example plugins.

A Prose plugin is a WebAssembly component plus a `plugin.toml`. Because the API is WIT, a plugin can be written in any language that builds components (Rust, C, Go, JavaScript, Python, C#). This repository's SDK is for Rust; for other languages, generate bindings from [`wit/v0.1.0`](wit/v0.1.0) with your language's tool.

Prose's own source is not here. The WIT is the whole contract between Prose and a plugin.

> **Status:** API `0.1.0` is a draft. Prose doesn't load plugins yet; it will change until the first Prose release with plugin support, when it is tagged `v0.1.0` and frozen.

## What a plugin can do (0.1)

| Interface | What it gives | Permission |
|---|---|---|
| `commands` | Add commands. They show in the `:` line, the command palette and the Shortcuts settings, and writers bind them in `keymap.toml` as `<plugin id>.<command>`. | none |
| `documents` | Read the document a command was run on: its blocks (text, formatting, kind, annotations), a page at a time, its selection and word count, and UTF-16 offsets for languages that count strings that way. | none |
| `edits` | Change that document. Everything one command or event applies is one undo step, so `u` undoes a whole plugin action. | `edit-documents` |
| `ui` | Status messages, notifications with buttons, text and choice prompts, a log. | none |
| `settings` | The plugin's own `[plugins.<id>]` table from the writer's `settings.toml`. | none |
| `workspace` | The open project's name, the documents open in panes and the active one, and opening a document in a pane or a split. | none |
| `project` | The whole project: the binder and every item's metadata (synopsis, label, status, keywords, target, custom fields) and notes, any document's text, project search, word counts, the session's words and the words written each day. | `read-project` |
| `history` | The project's drafts and revisions, a document's versions and snapshots, and a document as any of them kept it. Read only. | `read-project` |
| `binder` | Making, retitling, moving, duplicating and trashing binder items, setting their metadata and custom fields, and replacing text across the project. Each call is one change the writer can undo from the binder; nothing empties the trash. | `edit-binder` |
| `lint` (an export) | Check paragraphs: what the plugin finds is underlined and offered in the same menu as Prose's own spelling and grammar problems. | none |

Prose calls three functions the plugin exports: `activate` (register commands), `run-command`, and `on-event` (for the events the plugin's `plugin.toml` lists: document opened, saved or changed, idle, timers and so on). A plugin may export more, for what only some plugins do: the `linter` world adds the `lint` interface (below), and Prose calls it only on a plugin that exports it.

Plugins never draw their own UI. They describe it, and Prose draws it in the writer's theme, the same on desktop and phone. Later API versions add pickers, decorations over text, compile and import formats, and panels.

### Permissions

A plugin asks for permissions in `plugin.toml`, and the writer grants them when installing it. Prose links only the interfaces a plugin was granted, so a plugin that imports `edits` without `edit-documents` (or `project` without `read-project`, or `binder` without `edit-binder`) doesn't load at all. No permission reaches git remotes, credentials, Prose's license, other plugins' data, `settings.toml` beyond the plugin's own table, or the shell: the API takes binder ids and revision ids, never paths. A component only imports the interfaces it actually calls, so a plugin that never edits doesn't need the permission.

Plugins run in a sandbox: no files, no network, no clock beyond WASI's monotonic clock, and no other process. Prose stops a call that runs too long and turns off a plugin that keeps failing.

## Write a plugin in Rust

You need Rust 1.90 or newer and the WebAssembly target:

```sh
rustup target add wasm32-wasip2
```

Start from the template with [cargo-generate](https://github.com/cargo-generate/cargo-generate):

```sh
cargo install cargo-generate
cargo generate --git https://github.com/NickSpinosa/prose-plugin-api template --name my-plugin
cd my-plugin
cargo build --release
```

The template's `.cargo/config.toml` builds for `wasm32-wasip2`, so the plugin is `target/wasm32-wasip2/release/my_plugin.wasm`. To build without the template, add the SDK and make the crate a `cdylib`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
prose-plugin = { git = "https://github.com/NickSpinosa/prose-plugin-api", branch = "main" }
```

and build with `cargo build --release --target wasm32-wasip2`.

A plugin is a type implementing `Plugin`:

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

Edits are built against the version of the document you read. If the writer typed in between, Prose refuses the edit rather than guessing, and `Document::edit_with_retry` reads again and rebuilds it:

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

Ops apply in order, each to the document as the earlier ones left it, so when you make several changes in one block, go from its end to its start.

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

### Examples

- [`examples/word-freq`](examples/word-freq): `:WordFreq [n]` lists the most used words in the document. Reads blocks and settings; needs no permissions.
- [`examples/smart-quotes`](examples/smart-quotes): `:SmartQuotes` curls straight quotes in the selection or the whole document, as one undo step. Needs `edit-documents`.
- [`examples/filter-words`](examples/filter-words): underlines filter words ("just", "really", "suddenly") and offers to cut them, leaving dialogue alone unless asked. Exports `lint`; the words are a setting.

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

[settings.min_length]       # one table per setting, shown on Prose's settings screen
title = "Shortest word"     # optional
kind = "integer"            # boolean, integer, float, text, text-list or choice
default = 3
min = 1                     # optional, for integer and float
max = 20
help = "Words shorter than this are left out."
```

A `choice` setting lists its `options = [...]`. `grants` and `lint` are Prose's own keys in the plugin's table, so no setting may be called that. A plugin installed in Prose is a folder holding `plugin.toml` and the built component renamed `plugin.wasm`.

## Writers' settings and keys

```toml
# settings.toml
[plugins]
enabled = ["word-freq"]

[plugins.word-freq]
grants = []
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

Generate bindings from `wit/v0.1.0`, world `prose:plugin/plugin@0.1.0`, or `prose:plugin/linter@0.1.0` for a plugin that checks text:

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
