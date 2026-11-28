//! `rivetctl mcp`: the Model Context Protocol over standard input and output, so an AI assistant
//! can see which Rivet apps are here and install, update or remove them. Written from the
//! specification (modelcontextprotocol.io): JSON-RPC 2.0 messages, one per line, with
//! `initialize`, `ping`, `tools/list` and `tools/call`. Nothing but those messages goes to
//! standard output; anything else goes to standard error.
//!
//! Installing, updating and removing change the computer, so each takes `confirm`. Without
//! `confirm: true` the tool says what it would do and does nothing, so an assistant can show
//! that to the person first.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use rivet_install::catalog;
use rivet_install::channel::Channel;
use serde_json::{Value, json};

use crate::apps::{self, Done, World};

/// The protocol versions this speaks, newest first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Tools for the Rivet apps on this computer (a file manager, editor, terminal, notes and more, from github.com/rivetsoft/apps). Use list_apps or app_status to see what is installed and what each channel offers. install_app, update_apps and remove_app change the computer: call them first without confirm to see what they would do, tell the person, and call again with confirm set to true only once they agree. Apps are installed for this user alone, in ~/.local/bin.";

fn tools() -> Value {
    let app = json!({ "type": "string", "description": "The app: files, notes, edit, terminal, ftp, fetch… (list_apps has every id)" });
    let held = json!({ "type": "boolean", "description": "Include the apps held back from release", "default": false });
    let confirm = json!({ "type": "boolean", "description": "true to go ahead. Without it, nothing changes and the answer says what would happen" });
    json!([
        {
            "name": "list_apps",
            "title": "List the Rivet apps",
            "description": "Every Rivet app, with what is installed and the version each channel (stable, alpha, edge) offers, and which have updates.",
            "inputSchema": { "type": "object", "properties": { "held": held }, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": true },
        },
        {
            "name": "app_status",
            "title": "One app's status",
            "description": "One Rivet app: what is installed and where, its channel, what each channel offers and whether there is an update.",
            "inputSchema": { "type": "object", "properties": { "app": app, "held": held }, "required": ["app"], "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": true },
        },
        {
            "name": "install_app",
            "title": "Install an app",
            "description": "Install a Rivet app from its release, or update it if it is installed. With channel, the app follows that channel from now on. Needs confirm: true to change anything.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": app,
                    "channel": { "type": "string", "enum": ["stable", "alpha", "edge"], "description": "The channel to install from; the app keeps it" },
                    "held": held,
                    "confirm": confirm,
                },
                "required": ["app"],
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true },
        },
        {
            "name": "update_apps",
            "title": "Install updates",
            "description": "Install the updates of one Rivet app, or of every app when app is left out. Needs confirm: true to change anything.",
            "inputSchema": { "type": "object", "properties": { "app": app, "held": held, "confirm": confirm }, "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true },
        },
        {
            "name": "remove_app",
            "title": "Remove an app",
            "description": "Remove a Rivet app this computer's user installed: its program, launcher entry and icons. Its settings and documents stay. Needs confirm: true to change anything.",
            "inputSchema": { "type": "object", "properties": { "app": app, "held": held, "confirm": confirm }, "required": ["app"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "doctor",
            "title": "Check this computer",
            "description": "What is missing for the Rivet apps to install and run here (programs, libraries such as libmpv for Rivet Cinema, PATH, launcher entries), each with what to do.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
    ])
}

/// A tool's answer: text for the person and the assistant, the same as data, and whether it
/// failed.
fn answer(text: String, data: Value, error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": data, "isError": error })
}

fn refused(text: String) -> Value {
    answer(text.clone(), json!({ "error": text }), true)
}

fn flag(args: &Value, k: &str) -> bool {
    args.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn call(name: &str, args: &Value) -> Result<Value, (i64, String)> {
    let held = flag(args, "held");
    let confirm = flag(args, "confirm");
    let app = args.get("app").and_then(Value::as_str);
    Ok(match name {
        "list_apps" => {
            let w = World::load(held);
            let list: Vec<Value> = apps::ordered(&w.rows)
                .iter()
                .map(|r| apps::row_json(&w, r))
                .collect();
            let lines: Vec<String> = apps::ordered(&w.rows)
                .iter()
                .map(|r| {
                    let have = r
                        .installed
                        .as_ref()
                        .map(|i| format!("{} installed", apps::short(i.version.as_ref())))
                        .unwrap_or_else(|| "not installed".into());
                    let up = r
                        .update()
                        .map(|(_, v)| format!(", update to {}", v.short()))
                        .unwrap_or_default();
                    format!(
                        "{} ({}): {have}{up}, follows {}",
                        r.entry.title,
                        r.entry.id,
                        r.channel.name()
                    )
                })
                .collect();
            let mut text = lines.join("\n");
            if let Some(e) = &w.offline {
                text = format!(
                    "GitHub couldn't be reached ({e}); these are the releases from the last check.\n{text}"
                );
            }
            answer(text, json!({ "apps": list, "offline": w.offline }), false)
        }
        "app_status" => {
            let Some(app) = app else {
                return Err((-32602, "app_status needs app".into()));
            };
            let w = World::load(held);
            match w.find(app, held) {
                Ok(r) => {
                    let data = apps::row_json(&w, r);
                    answer(
                        serde_json::to_string_pretty(&data).unwrap_or_default(),
                        data,
                        false,
                    )
                }
                Err(e) => refused(e),
            }
        }
        "install_app" | "update_apps" => {
            let install = name == "install_app";
            if install && app.is_none() {
                return Err((-32602, "install_app needs app".into()));
            }
            let channel = match args.get("channel").and_then(Value::as_str) {
                Some(c) => match Channel::parse(c) {
                    Some(c) => Some(c),
                    None => {
                        return Err((
                            -32602,
                            format!("{c} isn't a channel: use stable, alpha or edge"),
                        ));
                    }
                },
                None => None,
            };
            let mut w = World::load(held);
            let picked: Vec<String> = match app {
                Some(a) => match w.find(a, held) {
                    Ok(r) if !install && r.installed.is_none() => {
                        return Ok(refused(format!(
                            "{} isn't installed, so there is nothing to update; install_app installs it",
                            r.entry.title
                        )));
                    }
                    Ok(r) => vec![r.entry.id.clone()],
                    Err(e) => return Ok(refused(e)),
                },
                None => rivet_install::state::updates(&w.rows)
                    .iter()
                    .map(|r| r.entry.id.clone())
                    .collect(),
            };
            if picked.is_empty() {
                return Ok(answer(
                    "Every app is up to date.".into(),
                    json!({ "done": [] }),
                    false,
                ));
            }
            if !confirm {
                let plan: Vec<Value> = picked
                    .iter()
                    .filter_map(|id| w.rows.iter().find(|r| &r.entry.id == id))
                    .map(|r| {
                        let c = channel.unwrap_or(r.channel);
                        let offered = catalog::latest(&r.entry, &w.releases, c).map(|(_, v)| v);
                        let have = r.installed.as_ref().and_then(|i| i.version.as_ref()).map(|v| v.to_string());
                        json!({ "app": r.entry.id, "title": r.entry.title, "channel": c.name(), "installed": have, "would_install": offered.map(|v| v.to_string()) })
                    })
                    .collect();
                let text = plan
                    .iter()
                    .map(|p| {
                        let to = p["would_install"]
                            .as_str()
                            .unwrap_or("nothing (no release on that channel)");
                        let from = p["installed"]
                            .as_str()
                            .map(|v| format!(" over {v}"))
                            .unwrap_or_default();
                        format!(
                            "{}: would install {to}{from} from the {} channel",
                            p["title"].as_str().unwrap_or(""),
                            p["channel"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(answer(
                    format!(
                        "Nothing changed yet.\n{text}\nCall {name} again with confirm set to true to go ahead."
                    ),
                    json!({ "confirmed": false, "plan": plan }),
                    false,
                ));
            }
            if let Some(c) = channel {
                for id in &picked {
                    if let Err(e) = rivet_install::channel::set_own(id, Some(c)) {
                        return Ok(refused(e));
                    }
                }
                w = World::from(w.releases, w.offline, held);
            }
            let mut done = Vec::new();
            let mut lines = Vec::new();
            let mut failed = false;
            for id in &picked {
                let Some(r) = w.rows.iter().find(|r| &r.entry.id == id) else {
                    continue;
                };
                match apps::install(r, &|_| {}) {
                    Ok(Done::Installed(v)) => {
                        lines.push(format!("Installed {} {}.", r.entry.title, v.label()));
                        done.push(json!({ "app": id, "ok": true, "installed": v.to_string() }));
                    }
                    Ok(Done::Already(v)) => {
                        lines.push(format!(
                            "{} {} is already installed.",
                            r.entry.title,
                            v.label()
                        ));
                        done.push(json!({ "app": id, "ok": true, "already": v.to_string() }));
                    }
                    Err(e) => {
                        failed = true;
                        lines.push(format!("{}: {e}", r.entry.title));
                        done.push(json!({ "app": id, "ok": false, "error": e }));
                    }
                }
            }
            answer(
                lines.join("\n"),
                json!({ "confirmed": true, "done": done }),
                failed,
            )
        }
        "remove_app" => {
            let Some(app) = app else {
                return Err((-32602, "remove_app needs app".into()));
            };
            let w = World::cached(held);
            let r = match w.find(app, held) {
                Ok(r) => r,
                Err(e) => return Ok(refused(e)),
            };
            if !confirm {
                let what = match &r.installed {
                    Some(i) if i.ours => format!(
                        "Nothing changed yet. remove_app would remove {} {} ({}), its launcher entry and icons; its settings and documents stay. Call remove_app again with confirm set to true to go ahead.",
                        r.entry.title,
                        apps::short(i.version.as_ref()),
                        i.path.display()
                    ),
                    Some(i) => {
                        return Ok(refused(format!(
                            "{} can't be removed here: {}",
                            r.entry.title,
                            i.left_alone()
                        )));
                    }
                    None => return Ok(refused(format!("{} isn't installed", r.entry.title))),
                };
                return Ok(answer(
                    what,
                    json!({ "confirmed": false, "app": r.entry.id }),
                    false,
                ));
            }
            match apps::remove(r) {
                Ok(()) => answer(
                    format!(
                        "Removed {}. Its settings and documents are still there.",
                        r.entry.title
                    ),
                    json!({ "confirmed": true, "app": r.entry.id, "removed": true }),
                    false,
                ),
                Err(e) => refused(e),
            }
        }
        "doctor" => {
            let w = World::cached(true);
            let checks = crate::doctor::run(&w.rows);
            answer(
                crate::doctor::text(&checks),
                json!({ "checks": crate::doctor::json(&checks) }),
                false,
            )
        }
        other => return Err((-32602, format!("no tool called {other}"))),
    })
}

/// The answer to one message, or None for a notification.
pub fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str);
    let reply = |result: Result<Value, (i64, String)>| {
        let id = id.clone().unwrap_or(Value::Null);
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        })
    };
    let Some(method) = method else {
        // An answer to something this never asks, or not a request at all.
        return if msg.get("result").is_some() || msg.get("error").is_some() {
            None
        } else {
            reply(Err((-32600, "not a JSON-RPC request".into())))
        };
    };
    // Notifications (initialized, cancelled…) get no answer.
    id.as_ref()?;
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    reply(match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "rivetctl", "title": "Rivet apps", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            call(name, &args)
        }
        other => Err((-32601, format!("no method {other}"))),
    })
}

pub fn serve() -> ExitCode {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Object(m)) => handle(&Value::Object(m)),
            Ok(_) => Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32600, "message": "send one JSON-RPC message per line" } }),
            ),
            Err(e) => Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("not JSON: {e}") } }),
            ),
        };
        if let Some(r) = reply
            && writeln!(out, "{r}").and_then(|()| out.flush()).is_err()
        {
            break;
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_and_tools() {
        let init = handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } })).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "rivetctl");
        // A version it doesn't know gets its newest.
        let other = handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } })).unwrap();
        assert_eq!(other["result"]["protocolVersion"], VERSIONS[0]);
        assert!(
            handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none()
        );
        let list = handle(&json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" })).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "list_apps",
                "app_status",
                "install_app",
                "update_apps",
                "remove_app",
                "doctor"
            ]
        );
        assert_eq!(list["id"], "a");
        let missing =
            handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" })).unwrap();
        assert_eq!(missing["error"]["code"], -32601);
        let bad = handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "format_disk" } })).unwrap();
        assert_eq!(bad["error"]["code"], -32602);
    }
}
