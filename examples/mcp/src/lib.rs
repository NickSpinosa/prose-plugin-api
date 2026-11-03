//! MCP: a Model Context Protocol server for Prose, so an AI assistant on the
//! writer's computer (Claude Desktop, Claude Code, or any MCP client) can
//! read the project and, unless the writer turns on `read_only`, edit it.
//!
//! Shows the `serve` export: Prose listens on 127.0.0.1 and hands each
//! message to [`Serve::handle`], so the plugin is only the protocol. It
//! answers JSON-RPC 2.0: `initialize`, `ping`, `tools/list` and
//! `tools/call` (see [`tools`] for what each tool does), and ignores
//! notifications.
//!
//! Connect a client with `prose --mcp mcp` as its command (stdio), or with
//! the URL and key in `plugins/mcp/serve.toml` (Streamable HTTP).

use prose_plugin::{Context, Plugin, Serve};
use serde_json::{Map, Value, json};

mod edit;
mod text;
mod tools;

/// The protocol versions this server speaks, newest first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// What the assistant is told about the server when it connects.
const INSTRUCTIONS: &str = "\
This server is Prose, the writing app, with the writer's project open. Items \
(documents and folders) are named by binder id; get them from list_binder, \
search or get_project. Documents are paragraphs, numbered from 0 as \
read_document shows them; edit_document takes those numbers and the version \
read_document gave, and refuses an edit if the writer has typed since, so read \
again then. Each edit is one undo step the writer can take back in Prose, and \
binder changes are undoable from the binder. Nothing is ever deleted: trashed \
items go to the Trash. Treat the text as the writer's own work: change only \
what they ask you to.";

#[derive(Default)]
struct Mcp;

impl Plugin for Mcp {
    fn run_command(
        &mut self,
        name: &str,
        _args: Vec<String>,
        _cx: Context,
    ) -> prose_plugin::Result {
        Err(format!("MCP has no command `{name}`"))
    }
}

impl Serve for Mcp {
    fn handle(&mut self, message: &str) -> Option<String> {
        let reply = match serde_json::from_str::<Value>(message) {
            Ok(Value::Array(batch)) if batch.is_empty() => {
                Some(error(Value::Null, -32600, "An empty batch"))
            }
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.into_iter().filter_map(one).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(message) => one(message),
            Err(e) => Some(error(Value::Null, -32700, &format!("Not JSON: {e}"))),
        };
        reply.map(|reply| reply.to_string())
    }
}

/// Answer one JSON-RPC message; `None` for a notification or a response.
fn one(message: Value) -> Option<Value> {
    let Value::Object(message) = message else {
        return Some(error(Value::Null, -32600, "A message must be an object"));
    };
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to something we never asked, or nonsense.
        return id.map(|id| error(id, -32600, "A request needs a method"));
    };
    let Some(id) = id else {
        // Notifications (initialized, cancelled, ...) need nothing.
        return None;
    };
    let params = match message.get("params") {
        Some(Value::Object(params)) => params.clone(),
        _ => Map::new(),
    };
    Some(match request(method, &params) {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, why)) => error(id, code, &why),
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn request(method: &str, params: &Map<String, Value>) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked
                .filter(|v| VERSIONS.contains(v))
                .unwrap_or(VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {
                    "name": "prose",
                    "title": "Prose",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools::list()})),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or((-32602, "tools/call needs a name".to_owned()))?;
            let arguments = match params.get("arguments") {
                Some(Value::Object(arguments)) => arguments.clone(),
                None | Some(Value::Null) => Map::new(),
                Some(_) => return Err((-32602, "arguments must be an object".to_owned())),
            };
            if !tools::exists(name) {
                return Err((-32602, format!("There is no tool {name}")));
            }
            Ok(match tools::call(name, &arguments) {
                Ok(text) => json!({"content": [{"type": "text", "text": text}]}),
                Err(why) => json!({"content": [{"type": "text", "text": why}], "isError": true}),
            })
        }
        _ => Err((-32601, format!("Prose doesn't do {method}"))),
    }
}

prose_plugin::export!(Mcp, serve);

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(message: Value) -> Value {
        let reply = Mcp.handle(&message.to_string()).expect("a reply");
        serde_json::from_str(&reply).unwrap()
    }

    #[test]
    fn initialize_agrees_on_a_version() {
        let reply = ask(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}}}));
        assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(reply["result"]["serverInfo"]["name"], "prose");
        let reply = ask(json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "1999-01-01"}}));
        assert_eq!(reply["result"]["protocolVersion"], VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_reply_and_unknown_methods_an_error() {
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert_eq!(Mcp.handle(&note.to_string()), None);
        let reply = ask(json!({"jsonrpc": "2.0", "id": "a", "method": "frobnicate"}));
        assert_eq!(reply["error"]["code"], -32601);
        assert_eq!(reply["id"], "a");
        let reply: Value = serde_json::from_str(&Mcp.handle("{nope").unwrap()).unwrap();
        assert_eq!(reply["error"]["code"], -32700);
    }

    #[test]
    fn batches_answer_each_request() {
        let reply = ask(json!([
            {"jsonrpc": "2.0", "id": 1, "method": "ping"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "ping"},
        ]));
        assert_eq!(reply.as_array().unwrap().len(), 2);
    }
}
