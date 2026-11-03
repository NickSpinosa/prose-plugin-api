//! Write plugins for [Prose](https://github.com/NickSpinosa/prose-releases),
//! the writing app, in Rust.
//!
//! A plugin is a WebAssembly component built for `wasm32-wasip2` against the
//! WIT package `prose:plugin@0.1.0` (in this repository's `wit/v0.1.0`).
//! This crate generates the bindings and wraps them in friendlier types:
//!
//! ```ignore
//! use prose_plugin::{Command, Context, Plugin, ui};
//!
//! #[derive(Default)]
//! struct Hello;
//!
//! impl Plugin for Hello {
//!     fn activate(&mut self) -> prose_plugin::Result {
//!         Command::new("hello", "Say hello").ex_name("Hello").register()?;
//!         Ok(())
//!     }
//!
//!     fn run_command(&mut self, name: &str, _args: Vec<String>, _cx: Context) -> prose_plugin::Result {
//!         match name {
//!             "hello" => ui::info("Hello from a plugin"),
//!             _ => return Err(format!("unknown command {name}")),
//!         }
//!         Ok(())
//!     }
//! }
//!
//! prose_plugin::export!(Hello);
//! ```
//!
//! The raw generated bindings are in [`bindings`] if you need something this
//! crate does not wrap.

#![warn(missing_docs)]

extern crate self as prose_plugin;

/// The bindings `wit-bindgen` generates from `wit/v0.1.0`, unwrapped.
#[allow(missing_docs, clippy::too_many_arguments)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "../../wit/v0.1.0",
        world: "prose:plugin/plugin@0.1.0",
        pub_export_macro: true,
        export_macro_name: "__export_world",
        default_bindings_module: "::prose_plugin::bindings",
        runtime_path: "::prose_plugin::__private::wit_bindgen::rt",
        bitflags_path: "::prose_plugin::__private::wit_bindgen::rt::bitflags",
        additional_derives: [PartialEq],
    });

    /// The `lint` export, generated on its own so that only a plugin that
    /// exports it with [`export!`](crate::export)`(Type, lint)` has it.
    /// The component then targets the `linter` world.
    pub mod lint {
        wit_bindgen::generate!({
            path: "../../wit/v0.1.0",
            inline: "package prose:sdk;\nworld lint-export { export prose:plugin/lint@0.1.0; }",
            world: "prose:sdk/lint-export",
            pub_export_macro: true,
            export_macro_name: "__export_lint",
            default_bindings_module: "::prose_plugin::bindings::lint",
            runtime_path: "::prose_plugin::__private::wit_bindgen::rt",
            bitflags_path: "::prose_plugin::__private::wit_bindgen::rt::bitflags",
            additional_derives: [PartialEq],
        });
    }

    /// The `compile` export (compile and import hooks), generated on its
    /// own like `lint`: only a plugin exported with
    /// [`export!`](crate::export)`(Type, compile)` has it, and the component
    /// then targets the `compile-plugin` world.
    pub mod compile {
        wit_bindgen::generate!({
            path: "../../wit/v0.1.0",
            inline: "package prose:sdk;\nworld compile-export { export prose:plugin/compile@0.1.0; }",
            world: "prose:sdk/compile-export",
            with: {
                "prose:plugin/types@0.1.0": crate::bindings::prose::plugin::types,
                "prose:plugin/documents@0.1.0": crate::bindings::prose::plugin::documents,
            },
            pub_export_macro: true,
            export_macro_name: "__export_compile",
            default_bindings_module: "::prose_plugin::bindings::compile",
            runtime_path: "::prose_plugin::__private::wit_bindgen::rt",
            additional_derives: [PartialEq],
        });
    }
}

pub mod binder;
mod command;
pub mod compile;
pub mod decorations;
mod document;
pub mod history;
mod lint;
pub mod project;
pub mod settings;
pub mod ui;
pub mod workspace;

pub use bindings::prose::plugin::types::{
    self, Annotation, AnnotationKind, Block, BlockKind, HighlightColor, ListItem, ListMarker,
    Marks, NewBlock, Op, Position, Run, Selection, Span, Transaction,
};
pub use bindings::{CompileFinished, Context, DocumentChanged, Event, SelectionChanged};
pub use command::{Argument, Command, RegisterError};
pub use compile::Compile;
pub use document::{Document, Edit, EditError, InvalidOp, new_id};
pub use lint::{Diagnostic, DiagnosticKind, Lint};

/// The version of the WIT package this crate is built against. A plugin's
/// `plugin.toml` names it as `api`.
pub const API_VERSION: &str = "0.1.0";

/// What a plugin's hooks return. An `Err` is shown to the writer.
pub type Result<T = (), E = String> = std::result::Result<T, E>;

/// A Prose plugin. Implement it on a type with [`Default`] and pass the type
/// to [`export!`]. Prose makes one instance when it loads the plugin and
/// calls every hook on it, one call at a time, on the plugin's own thread.
pub trait Plugin: Default + 'static {
    /// Called once when the plugin loads. Register commands here. An error
    /// turns the plugin off and shows the message.
    fn activate(&mut self) -> Result {
        Ok(())
    }

    /// The writer ran one of the plugin's commands: `name` is the name it
    /// was registered with, `args` the words after it on the `:` line.
    fn run_command(&mut self, name: &str, args: Vec<String>, cx: Context) -> Result;

    /// Something the plugin subscribed to in `plugin.toml` happened.
    fn on_event(&mut self, event: Event) {
        let _ = event;
    }
}

impl Context {
    /// The document the command was run on, if any.
    pub fn document(&self) -> Option<Document> {
        document::current()
    }
}

/// Exports a [`Plugin`] type as the component's implementation of the
/// `plugin` world. Use it once, at the crate root.
///
/// A plugin that also checks paragraphs implements [`Lint`] and passes
/// `lint` too: `prose_plugin::export!(FilterWords, lint);`. A plugin with
/// compile and import hooks implements [`Compile`] and passes `compile`:
/// `prose_plugin::export!(Fountain, compile);`. One with both passes
/// `lint, compile`.
#[macro_export]
macro_rules! export {
    ($plugin:ty) => {
        $crate::__export_plugin!($plugin, {});
    };
    ($plugin:ty, lint) => {
        $crate::__export_plugin!($plugin, {
            $crate::__prose_lint_export!();
        });
    };
    ($plugin:ty, compile) => {
        $crate::__export_plugin!($plugin, {
            $crate::__prose_compile_export!();
        });
    };
    ($plugin:ty, lint, compile) => {
        $crate::__export_plugin!($plugin, {
            $crate::__prose_lint_export!();
            $crate::__prose_compile_export!();
        });
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __prose_lint_export {
    () => {
        struct __ProseLint;

        impl $crate::bindings::lint::exports::prose::plugin::lint::Guest for __ProseLint {
            fn lint(paragraph: ::std::string::String) -> ::std::vec::Vec<$crate::Diagnostic> {
                __PROSE_PLUGIN.with_borrow_mut(|p| $crate::Lint::lint(p, &paragraph))
            }
        }

        $crate::bindings::lint::__export_lint!(__ProseLint);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __prose_compile_export {
    () => {
        struct __ProseCompile;

        impl $crate::bindings::compile::exports::prose::plugin::compile::Guest for __ProseCompile {
            fn transform(
                info: $crate::compile::CompileInfo,
                manuscript: $crate::bindings::prose::plugin::documents::Document,
                notes: ::std::vec::Vec<$crate::compile::Note>,
            ) -> ::std::result::Result<::std::vec::Vec<$crate::Op>, ::std::string::String> {
                __PROSE_PLUGIN.with_borrow_mut(|p| {
                    $crate::Compile::transform(p, info, manuscript.into(), notes)
                })
            }

            fn export_format(
                format: ::std::string::String,
                info: $crate::compile::CompileInfo,
                manuscript: $crate::bindings::prose::plugin::documents::Document,
                notes: ::std::vec::Vec<$crate::compile::Note>,
                layout: $crate::compile::Layout,
            ) -> ::std::result::Result<::std::vec::Vec<u8>, ::std::string::String> {
                __PROSE_PLUGIN.with_borrow_mut(|p| {
                    $crate::Compile::export_format(
                        p,
                        &format,
                        info,
                        manuscript.into(),
                        notes,
                        layout,
                    )
                })
            }

            fn import_format(
                format: ::std::string::String,
                file_name: ::std::string::String,
                bytes: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<
                ::std::vec::Vec<$crate::compile::Imported>,
                ::std::string::String,
            > {
                __PROSE_PLUGIN.with_borrow_mut(|p| {
                    $crate::Compile::import_format(p, &format, &file_name, bytes)
                })
            }
        }

        $crate::bindings::compile::__export_compile!(__ProseCompile);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __export_plugin {
    ($plugin:ty, { $($more:tt)* }) => {
        const _: () = {
            ::std::thread_local! {
                static __PROSE_PLUGIN: ::std::cell::RefCell<$plugin> =
                    ::std::cell::RefCell::new(<$plugin as ::std::default::Default>::default());
            }

            struct __ProseGuest;

            impl $crate::bindings::Guest for __ProseGuest {
                fn activate() -> ::std::result::Result<(), ::std::string::String> {
                    __PROSE_PLUGIN.with_borrow_mut(|p| $crate::Plugin::activate(p))
                }

                fn run_command(
                    name: ::std::string::String,
                    args: ::std::vec::Vec<::std::string::String>,
                    context: $crate::Context,
                ) -> ::std::result::Result<(), ::std::string::String> {
                    __PROSE_PLUGIN
                        .with_borrow_mut(|p| $crate::Plugin::run_command(p, &name, args, context))
                }

                fn on_event(event: $crate::Event) {
                    __PROSE_PLUGIN.with_borrow_mut(|p| $crate::Plugin::on_event(p, event))
                }
            }

            $crate::bindings::__export_world!(__ProseGuest);

            $($more)*
        };
    };
}

#[doc(hidden)]
pub mod __private {
    pub use wit_bindgen;
}

impl Position {
    /// Byte `offset` into block `block`'s text.
    pub fn new(block: impl Into<String>, offset: u32) -> Self {
        Position {
            block: block.into(),
            offset,
        }
    }
}

impl Span {
    /// From `start` to `end`, in document order.
    pub fn new(start: Position, end: Position) -> Self {
        Span { start, end }
    }

    /// Byte range `start..end` of one block.
    pub fn in_block(block: &str, start: u32, end: u32) -> Self {
        Span {
            start: Position::new(block, start),
            end: Position::new(block, end),
        }
    }

    /// The whole of `block`'s text.
    pub fn whole_block(block: &Block) -> Self {
        Span::in_block(&block.id, 0, block.text.len() as u32)
    }
}

impl Selection {
    /// Whether it is a bare caret, selecting nothing.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected span, start first. Needs the document to order positions
    /// in different blocks; see [`Document::selection_span`].
    pub fn span_in(&self, doc: &Document) -> Span {
        doc.order(self.anchor.clone(), self.head.clone())
    }
}

impl Block {
    /// The text of byte range `start..end`, or `None` if it is out of bounds
    /// or splits a character.
    pub fn slice(&self, start: u32, end: u32) -> Option<&str> {
        self.text.get(start as usize..end as usize)
    }
}

impl NewBlock {
    /// A plain paragraph with a fresh id.
    pub fn paragraph(text: impl Into<String>) -> Self {
        NewBlock {
            id: new_id(),
            kind: BlockKind::Paragraph,
            text: text.into(),
            runs: Vec::new(),
        }
    }
}
