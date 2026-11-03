//! The plugin's own settings: its `[plugins.<id>]` table in the writer's
//! `settings.toml`, with defaults from `plugin.toml`. Read a setting when you
//! need it; Prose picks up changes to the file.

pub use crate::bindings::prose::plugin::settings::{Value, get, keys};

/// A boolean setting.
pub fn bool(key: &str) -> Option<bool> {
    match get(key)? {
        Value::Boolean(b) => Some(b),
        _ => None,
    }
}

/// An integer setting.
pub fn integer(key: &str) -> Option<i64> {
    match get(key)? {
        Value::Integer(n) => Some(n),
        _ => None,
    }
}

/// A number setting. Integers count.
pub fn float(key: &str) -> Option<f64> {
    match get(key)? {
        Value::Float(x) => Some(x),
        Value::Integer(n) => Some(n as f64),
        _ => None,
    }
}

/// A text setting.
pub fn text(key: &str) -> Option<String> {
    match get(key)? {
        Value::Text(s) => Some(s),
        _ => None,
    }
}

/// A list-of-text setting.
pub fn text_list(key: &str) -> Option<Vec<String>> {
    match get(key)? {
        Value::TextList(list) => Some(list),
        _ => None,
    }
}
