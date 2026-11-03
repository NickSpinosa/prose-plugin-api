//! The example plugins, built for wasm32-wasip2 and run in the fake Prose.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use host_test::prose::plugin::settings::Value;
use host_test::prose::plugin::types::{Position, Selection};
use host_test::{Doc, Grants, Level, Loaded, Prose, load};

/// Builds both examples once and returns the folder holding the `.wasm`s.
fn wasm_dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = root.join("target/host-test");
        let status = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args(["build", "--release", "--target", "wasm32-wasip2"])
            .args(["-p", "word-freq", "-p", "smart-quotes"])
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
