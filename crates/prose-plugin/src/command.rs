use crate::bindings::prose::plugin::commands;

pub use commands::{Argument, RegisterError};

/// A command to add to Prose. Build it and call [`register`](Self::register)
/// from [`Plugin::activate`](crate::Plugin::activate).
///
/// ```ignore
/// Command::new("top_words", "Top words")
///     .description("List the document's most used words")
///     .ex_name("WordFreq")
///     .argument(Argument::Text)
///     .suggested_keys("secondary-k secondary-f")
///     .register()?;
/// ```
#[derive(Clone, Debug)]
pub struct Command {
    raw: commands::Command,
}

impl Command {
    /// A command called `name` (lowercase words joined with `_`), shown as
    /// `title`. The writer binds it in `keymap.toml` as
    /// `<plugin id>.<name>`.
    pub fn new(name: impl Into<String>, title: impl Into<String>) -> Self {
        Command {
            raw: commands::Command {
                name: name.into(),
                title: title.into(),
                description: String::new(),
                ex_names: Vec::new(),
                argument: Argument::None,
                suggested_keys: Vec::new(),
            },
        }
    }

    /// One line on what it does.
    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.raw.description = text.into();
        self
    }

    /// A name for the `:` line, starting with a capital letter.
    pub fn ex_name(mut self, name: impl Into<String>) -> Self {
        self.raw.ex_names.push(name.into());
        self
    }

    /// What it takes after its name on the `:` line.
    pub fn argument(mut self, argument: Argument) -> Self {
        self.raw.argument = argument;
        self
    }

    /// Keys to suggest, in `keymap.toml` syntax. The writer's own bindings
    /// always win.
    pub fn suggested_keys(mut self, keys: impl Into<String>) -> Self {
        self.raw.suggested_keys.push(keys.into());
        self
    }

    /// Adds it to Prose.
    pub fn register(self) -> Result<(), RegisterError> {
        commands::register(&self.raw)
    }
}

impl From<RegisterError> for String {
    fn from(e: RegisterError) -> String {
        e.to_string()
    }
}
