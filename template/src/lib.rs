//! {{title}}: {{description}}

use prose_plugin::{Command, Context, Plugin, ui};

#[derive(Default)]
struct {{ex_name}};

impl Plugin for {{ex_name}} {
    fn activate(&mut self) -> prose_plugin::Result {
        // Writers run it as :{{ex_name}}, from the palette, or with a key
        // they bind to "{{project-name}}.run" in keymap.toml.
        Command::new("run", "{{title}}")
            .description("{{description}}")
            .ex_name("{{ex_name}}")
            .register()?;
        Ok(())
    }

    fn run_command(&mut self, name: &str, _args: Vec<String>, cx: Context) -> prose_plugin::Result {
        match name {
            "run" => {
                let doc = cx.document().ok_or("Open a document first")?;
                ui::info(&format!("{} has {} words", doc.title(), doc.word_count()));
                Ok(())
            }
            _ => Err(format!("no command `{name}`")),
        }
    }
}

prose_plugin::export!({{ex_name}});
