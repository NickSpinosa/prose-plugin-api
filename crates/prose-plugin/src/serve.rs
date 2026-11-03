//! Answering apps on this computer: the `serve` export (WIT
//! `prose:plugin/serve`), for a plugin that is an MCP server or speaks any
//! other message protocol to a local app. See [`Serve`].

/// A plugin that answers apps on this computer. Implement it beside
/// [`Plugin`] and export with `prose_plugin::export!(MyPlugin, serve)`. Ask
/// for the permission and name the port in `plugin.toml`:
///
/// ```toml
/// permissions = ["serve", "read-project"]
/// port = 7767        # Prose listens on 127.0.0.1:7767; the writer can change it
/// ```
///
/// Prose owns the socket: it listens on 127.0.0.1 only while the plugin is
/// on, turns away requests without the key it keeps for the plugin, and
/// hands each message to [`handle`](Serve::handle). Apps that speak over
/// standard input and output reach the plugin through
/// `prose --mcp <plugin id>`.
///
/// A message runs like a command (one at a time, under the command
/// deadline, with the whole API but no current document), and its edits are
/// one undo step.
///
/// ```ignore
/// impl Serve for Echo {
///     fn handle(&mut self, message: &str) -> Option<String> {
///         Some(message.to_owned())
///     }
/// }
/// ```
///
/// [`Plugin`]: crate::Plugin
pub trait Serve: crate::Plugin {
    /// Answer `message`, or return `None` for one that takes no reply (a
    /// JSON-RPC notification).
    fn handle(&mut self, message: &str) -> Option<String>;
}
