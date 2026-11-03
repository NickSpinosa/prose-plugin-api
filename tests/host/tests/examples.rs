//! The example plugins, built for wasm32-wasip2 and run in the fake Prose.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use host_test::compile::Note;
use host_test::prose::plugin::settings::Value;
use host_test::prose::plugin::types::{
    Annotation, AnnotationKind, Marks, Position, Run, Selection,
};
use host_test::{BlockKind, DiagnosticKind, Doc, Grants, Level, Loaded, Prose, load};

/// Builds the examples once and returns the folder holding the `.wasm`s.
fn wasm_dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = root.join("target/host-test");
        let status = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args(["build", "--release", "--target", "wasm32-wasip2"])
            .args([
                "-p",
                "word-freq",
                "-p",
                "smart-quotes",
                "-p",
                "filter-words",
                "-p",
                "fountain",
            ])
            .env("CARGO_TARGET_DIR", &target)
            .status()
            .expect("run cargo");
        assert!(
            status.success(),
            "building the examples failed (rustup target add wasm32-wasip2?)"
        );
        target.join("wasm32-wasip2/release")
    })
}

fn wasm(name: &str) -> Vec<u8> {
    std::fs::read(wasm_dir().join(format!("{name}.wasm"))).unwrap()
}

fn messages(p: &Loaded) -> Vec<String> {
    p.prose().messages.iter().map(|(m, _)| m.clone()).collect()
}

#[test]
fn word_freq_lists_top_words() {
    let prose = Prose {
        doc: Doc::paragraphs(&[
            "The lighthouse keeper climbed the lighthouse stairs.",
            "Stairs, always stairs. The keeper counted them.",
        ]),
        settings: [
            ("ignore".to_string(), Value::TextList(vec!["the".into()])),
            ("min_length".to_string(), Value::Integer(4)),
        ]
        .into(),
        ..Prose::default()
    };
    let mut p = load(&wasm("word_freq"), Grants::default(), prose).unwrap();

    let cmd = &p.prose().commands[0];
    assert_eq!(cmd.name, "top_words");
    assert_eq!(cmd.ex_names, ["WordFreq"]);

    p.run("top_words", &["3"]).unwrap().unwrap();
    assert_eq!(
        messages(&p),
        ["Chapter One: stairs 3, keeper 2, lighthouse 2"]
    );
    assert_eq!(p.prose().messages[0].1, Level::Info);
}

#[test]
fn word_freq_reports_a_bad_argument() {
    let mut p = load(
        &wasm("word_freq"),
        Grants::default(),
        Prose {
            doc: Doc::paragraphs(&["x"]),
            ..Prose::default()
        },
    )
    .unwrap();
    assert_eq!(
        p.run("top_words", &["lots"]).unwrap(),
        Err("`lots` is not a number".into())
    );
}

#[test]
fn word_freq_pages_through_a_long_document() {
    let texts: Vec<String> = (0..1000).map(|i| format!("word{} common", i % 7)).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut p = load(
        &wasm("word_freq"),
        Grants::default(),
        Prose {
            doc: Doc::paragraphs(&refs),
            ..Prose::default()
        },
    )
    .unwrap();
    p.run("top_words", &["1"]).unwrap().unwrap();
    assert_eq!(messages(&p), ["Chapter One: common 1000"]);
}

#[test]
fn word_freq_does_not_need_edits() {
    // It never calls `edits`, so it loads with nothing granted.
    assert!(load(&wasm("word_freq"), Grants::default(), Prose::default()).is_ok());
}

#[test]
fn smart_quotes_needs_edit_documents() {
    // Without the grant the `edits` interface is not linked, so the
    // component cannot be instantiated at all.
    let err = load(&wasm("smart_quotes"), Grants::default(), Prose::default())
        .err()
        .expect("must not load without edit-documents");
    assert!(
        format!("{err:?}").contains("prose:plugin/edits@0.1.0"),
        "{err:?}"
    );
}

#[test]
fn smart_quotes_curls_the_document_in_one_undo_step() {
    let prose = Prose {
        doc: Doc::paragraphs(&[r#""Don't," she said."#, "'Fine.'"]),
        ..Prose::default()
    };
    let mut p = load(
        &wasm("smart_quotes"),
        Grants {
            edit_documents: true,
        },
        prose,
    )
    .unwrap();
    p.run("curl", &[]).unwrap().unwrap();
    let texts: Vec<_> = p
        .prose()
        .doc
        .blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "\u{201C}Don\u{2019}t,\u{201D} she said.",
            "\u{2018}Fine.\u{2019}"
        ]
    );
    assert_eq!(p.prose().undo_steps, 1);
    assert_eq!(messages(&p), ["Curled 5 quotes"]);
}

#[test]
fn smart_quotes_only_touches_the_selection() {
    let mut doc = Doc::paragraphs(&[r#"'a' 'b'"#, r#""c""#]);
    doc.selection = Some(Selection {
        anchor: Position {
            block: "b0".into(),
            offset: 4,
        },
        head: Position {
            block: "b0".into(),
            offset: 7,
        },
    });
    let mut p = load(
        &wasm("smart_quotes"),
        Grants {
            edit_documents: true,
        },
        Prose {
            doc,
            ..Prose::default()
        },
    )
    .unwrap();
    p.run("curl", &[]).unwrap().unwrap();
    let texts: Vec<_> = p
        .prose()
        .doc
        .blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(texts, ["'a' \u{2018}b\u{2019}", r#""c""#]);
}

#[test]
fn smart_quotes_retries_when_the_writer_typed() {
    let mut p = load(
        &wasm("smart_quotes"),
        Grants {
            edit_documents: true,
        },
        Prose {
            doc: Doc::paragraphs(&["'x'"]),
            writer_types_once: true,
            ..Prose::default()
        },
    )
    .unwrap();
    p.run("curl", &[]).unwrap().unwrap();
    assert_eq!(p.prose().doc.blocks[0].text, "\u{2018}x\u{2019}");
    assert_eq!(p.prose().undo_steps, 1);
}

#[test]
fn a_command_without_a_document_is_an_error() {
    let mut p = load(&wasm("word_freq"), Grants::default(), Prose::default()).unwrap();
    // Outside a command run on a document, `current()` gives none.
    let r = p.plugin.call_run_command(
        &mut p.store,
        "top_words",
        &[],
        &host_test::Context {
            document: None,
            selection: None,
            span: None,
            count: None,
        },
    );
    assert_eq!(r.unwrap(), Err("Open a document first".into()));
}

#[test]
fn filter_words_lints_paragraphs() {
    let prose = Prose {
        settings: [(
            "words".to_string(),
            Value::TextList(vec!["just".into(), "suddenly".into()]),
        )]
        .into(),
        ..Prose::default()
    };
    let mut p = load(&wasm("filter_words"), Grants::default(), prose).unwrap();
    let text = "Suddenly she just left. \u{201c}I just can\u{2019}t,\u{201d} he said.";
    let found = p.lint(text).unwrap();
    let flagged: Vec<&str> = found
        .iter()
        .map(|d| &text[d.start as usize..d.end as usize])
        .collect();
    assert_eq!(flagged, ["Suddenly ", "just "], "dialogue is left alone");
    assert!(found.iter().all(|d| d.kind == DiagnosticKind::Style));
    assert_eq!(found[1].rule, "filter_word");
    assert_eq!(found[1].suggestions, [""]);
    assert!(found[1].message.contains("just"));
}

#[test]
fn a_plugin_without_the_export_has_no_lint() {
    let p = load(&wasm("word_freq"), Grants::default(), Prose::default()).unwrap();
    assert!(p.lint.is_none());
}

fn grants() -> Grants {
    Grants {
        edit_documents: true,
    }
}

#[test]
fn smart_quotes_curls_quotes_in_a_compile() {
    let prose = Prose {
        doc: Doc::paragraphs(&[r#""Don't," she said."#, "No quotes here."]),
        ..Prose::default()
    };
    let mut p = load(&wasm("smart_quotes"), grants(), prose).unwrap();
    p.transform().unwrap().unwrap();
    assert_eq!(
        p.prose().doc.blocks[0].text,
        "\u{201C}Don\u{2019}t,\u{201D} she said."
    );
    assert_eq!(p.prose().doc.blocks[1].text, "No quotes here.");
    // A transform hands its ops back: nothing is applied as an edit.
    assert_eq!(p.prose().undo_steps, 0);
}

#[test]
fn word_freq_has_no_compile_hooks() {
    let p = load(&wasm("word_freq"), Grants::default(), Prose::default()).unwrap();
    assert!(p.compile.is_none());
}

fn screenplay() -> Doc {
    let mut doc = Doc::paragraphs(&[
        "Act One",
        "INT. LIGHTHOUSE - NIGHT",
        "Rain hammers the glass. The lamp turns.",
        "MARA: Who keeps the light now?",
        "",
        "THE END",
    ]);
    doc.title = "The Lighthouse".into();
    doc.blocks[0].kind = BlockKind::Heading(1);
    doc.blocks[4].kind = BlockKind::SceneBreak;
    // "hammers" in italics.
    doc.blocks[2].runs = vec![
        run(5, Marks::empty()),
        run(7, Marks::ITALIC),
        run(27, Marks::empty()),
    ];
    doc
}

fn run(len: u32, marks: Marks) -> Run {
    Run {
        len,
        marks,
        style: None,
    }
}

#[test]
fn fountain_exports_a_screenplay() {
    let prose = Prose {
        doc: screenplay(),
        ..Prose::default()
    };
    let mut p = load(&wasm("fountain"), Grants::default(), prose).unwrap();
    let bytes = p.export("fountain", &[]).unwrap().unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Title: The Lighthouse\n\
         Credit: written by\n\
         Author: Ada Lovelace\n\
         \n\
         # Act One\n\
         \n\
         INT. LIGHTHOUSE - NIGHT\n\
         \n\
         Rain *hammers* the glass. The lamp turns.\n\
         \n\
         MARA\n\
         Who keeps the light now?\n\
         \n\
         ===\n\
         \n\
         !THE END\n\
         \n"
    );
    assert!(p.export("pdf", &[]).unwrap().is_err());
}

#[test]
fn fountain_writes_footnotes_as_notes() {
    let mut doc = Doc::paragraphs(&["The lamp turns."]);
    doc.blocks[0].annotations = vec![Annotation {
        id: "n1".into(),
        start: 8,
        end: 8,
        kind: AnnotationKind::Footnote,
    }];
    let prose = Prose {
        doc,
        ..Prose::default()
    };
    let mut p = load(&wasm("fountain"), Grants::default(), prose).unwrap();
    let notes = [Note {
        id: "n1".into(),
        blocks: Doc::paragraphs(&["A Fresnel lens."]).blocks,
    }];
    let bytes = p.export("fountain", &notes).unwrap().unwrap();
    let script = String::from_utf8(bytes).unwrap();
    assert!(
        script.ends_with("The lamp[[A Fresnel lens.]] turns.\n\n"),
        "{script}"
    );
}

#[test]
fn fountain_imports_a_document_per_section() {
    let mut p = load(&wasm("fountain"), Grants::default(), Prose::default()).unwrap();
    let script = "Title: The Lighthouse\n\
                  Author: Ada\n\
                  \n\
                  # Act One\n\
                  = Mara arrives.\n\
                  \n\
                  INT. LIGHTHOUSE - NIGHT\n\
                  \n\
                  Rain **hammers** the glass.\n\
                  \n\
                  MARA\n\
                  Who keeps\n\
                  the light now?\n\
                  \n\
                  ===\n\
                  \n\
                  # Act Two\n\
                  \n\
                  ## Morning\n\
                  \n\
                  !THE END\n";
    let docs = p
        .import("fountain", "lighthouse.fountain", script.as_bytes())
        .unwrap()
        .unwrap();
    let titles: Vec<&str> = docs.iter().map(|d| d.title.as_str()).collect();
    assert_eq!(titles, ["Act One", "Act Two"]);
    assert_eq!(docs[0].synopsis, "Mara arrives.");
    let texts: Vec<&str> = docs[0].blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "INT. LIGHTHOUSE - NIGHT",
            "Rain hammers the glass.",
            "MARA: Who keeps the light now?",
            "",
        ]
    );
    let bold = &docs[0].blocks[1].runs[1];
    assert_eq!((bold.len, bold.marks), (7, Marks::BOLD));
    assert!(matches!(docs[0].blocks[3].kind, BlockKind::SceneBreak));
    assert!(matches!(docs[1].blocks[0].kind, BlockKind::Heading(2)));
    assert_eq!(docs[1].blocks[1].text, "THE END");

    let untitled = p
        .import("fountain", "draft.fountain", b"FADE IN:\n\nA beach.\n")
        .unwrap()
        .unwrap();
    assert_eq!(untitled[0].title, "draft");
    assert!(p.import("fdx", "x.fdx", b"").unwrap().is_err());
}
