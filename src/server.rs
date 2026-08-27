use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
};

use parking_lot::RwLock;
use serde_json::json;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::model::{AppCommand, UsageSnapshot};

fn json_response(value: serde_json::Value, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let mut response = Response::from_data(body).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("Content-Type", "application/json; charset=utf-8") {
        response.add_header(header);
    }
    response
}

pub fn start(
    port: u16,
    snapshot: Arc<RwLock<UsageSnapshot>>,
    visible: Arc<AtomicBool>,
    commands: Sender<AppCommand>,
) -> anyhow::Result<thread::JoinHandle<()>> {
    let server =
        Server::http(("127.0.0.1", port)).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(thread::Builder::new()
        .name("action-listener".into())
        .spawn(move || {
            for mut request in server.incoming_requests() {
                let route = request
                    .url()
                    .split('?')
                    .next()
                    .unwrap_or(request.url())
                    .to_owned();
                let method = request.method().clone();
                let response = match (method, route.as_str()) {
                    (Method::Get, "/health") => {
                        json_response(json!({"ok": true, "app": "codex-calendar"}), 200)
                    }
                    (Method::Get, "/quota") => {
                        let value = serde_json::to_value(snapshot.read().clone())
                            .unwrap_or_else(|_| json!({"ok": false}));
                        json_response(value, 200)
                    }
                    (Method::Post, "/panel/show") => {
                        let _ = commands.send(AppCommand::Show);
                        json_response(json!({"ok": true, "action": "show", "visible": true}), 200)
                    }
                    (Method::Post, "/panel/settings/language") => {
                        let _ = commands.send(AppCommand::ShowLanguageMenu);
                        json_response(
                            json!({"ok": true, "action": "show-language-menu", "visible": true}),
                            200,
                        )
                    }
                    (Method::Post, "/panel/statistics") => {
                        let _ = commands.send(AppCommand::ShowStatistics);
                        json_response(
                            json!({"ok": true, "action": "show-statistics", "visible": true}),
                            200,
                        )
                    }
                    (Method::Post, "/panel/quota") => {
                        let _ = commands.send(AppCommand::ShowQuota);
                        json_response(
                            json!({"ok": true, "action": "show-quota", "visible": true}),
                            200,
                        )
                    }
                    (Method::Post, "/panel/hide") => {
                        let _ = commands.send(AppCommand::Hide);
                        json_response(json!({"ok": true, "action": "hide", "visible": false}), 200)
                    }
                    (Method::Post, "/panel/toggle") => {
                        let next = !visible.load(Ordering::Relaxed);
                        let _ = commands.send(AppCommand::Toggle);
                        json_response(
                            json!({"ok": true, "action": "toggle", "visible": next}),
                            200,
                        )
                    }
                    (Method::Post, "/panel/quit") => {
                        let _ = commands.send(AppCommand::Quit);
                        json_response(json!({"ok": true, "action": "quit"}), 200)
                    }
                    _ => {
                        let mut ignored = Vec::new();
                        let _ = request.as_reader().take(8 * 1024).read_to_end(&mut ignored);
                        json_response(json!({"ok": false, "error": "not found"}), 404)
                    }
                };
                let _ = request.respond(response);
            }
        })?)
}

pub fn notify_existing(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/panel/toggle");
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .and_then(|client| client.post(url).send())
        .is_ok_and(|response| response.status().is_success())
}
