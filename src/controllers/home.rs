use std::{
    collections::{HashMap, HashSet},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::http::StatusCode;
use loco_rs::prelude::*;

use crate::views::home::{AppInfo, ScanError, ServerInfo, ServerSnapshot};
use crate::{
    models::server_preferences::{self, PinnedServer},
    views::home::{CommandResponse, OpenServerParams, PinServerParams},
};

const DASHBOARD: &str = include_str!("../../assets/index.html");

#[derive(Default)]
struct ProcessDetails {
    parent_pid: Option<u32>,
    cpu_percent: Option<f32>,
    memory_percent: Option<f32>,
    started_at: Option<String>,
    command: String,
    executable: String,
}

#[derive(Default)]
struct ListeningProcess {
    pid: u32,
    name: String,
    user: String,
    descriptor: Option<String>,
}

#[debug_handler]
async fn dashboard() -> Result<Response> {
    format::html(DASHBOARD)
}

#[debug_handler]
async fn current() -> Result<Response> {
    format::json(AppInfo {
        name: "Port Watch",
        version: env!("CARGO_PKG_VERSION"),
        description: "Live local TCP listener discovery, powered by Loco and Rust.",
        endpoints: [
            "GET /",
            "GET /api",
            "GET /api/servers",
            "POST /api/pins",
            "POST /api/open",
        ],
    })
}

#[debug_handler]
async fn pin(Json(params): Json<PinServerParams>) -> Result<Response> {
    let servers = server_preferences::set(
        PinnedServer {
            process: params.process,
            address: params.address,
            port: params.port,
            url: params.url,
            custom_command: params.custom_command,
        },
        params.pinned,
    )?;
    format::json(servers)
}

#[debug_handler]
async fn open(Json(params): Json<OpenServerParams>) -> Result<Response> {
    let Some(server) = server_preferences::list()?
        .into_iter()
        .find(|item| item.matches(&params.process, &params.address, params.port))
    else {
        return bad_request("pin this server before running a custom command");
    };
    let Some(command) = server.rendered_command() else {
        return bad_request("this server does not have a custom command");
    };

    let mut child = Command::new("sh").args(["-c", &command]).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    format::json(CommandResponse { status: "launched" })
}

#[debug_handler]
async fn servers() -> Result<Response> {
    match scan_servers() {
        Ok(snapshot) => format::json(snapshot),
        Err(message) => format::render()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .json(ScanError {
                error: "scan_unavailable",
                message,
            }),
    }
}

pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(dashboard))
        .add("/api", get(current))
        .add("/api/servers", get(servers))
        .add("/api/pins", post(pin))
        .add("/api/open", post(open))
}

fn scan_servers() -> std::result::Result<ServerSnapshot, String> {
    let output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-FpcLfnT"])
        .output()
        .map_err(|error| format!("Could not run lsof: {error}. Install lsof and try again."))?;

    if !output.status.success() && !output.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let pids = raw
        .lines()
        .filter_map(|line| line.strip_prefix('p')?.parse::<u32>().ok())
        .collect::<HashSet<_>>();
    let details = process_details(&pids);
    let mut servers = parse_lsof(&raw, &details);
    let pinned = server_preferences::list().map_err(|error| error.to_string())?;
    for server in &mut servers {
        if let Some(preference) = pinned
            .iter()
            .find(|item| item.matches(&server.process, &server.address, server.port))
        {
            server.pinned = true;
            server.custom_command.clone_from(&preference.custom_command);
        }
    }
    servers.sort_by_key(|server| (!server.pinned, server.port, server.pid));
    let hostname = Command::new("hostname")
        .output()
        .ok()
        .filter(|result| result.status.success())
        .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "this machine".to_string());

    Ok(ServerSnapshot {
        scanned_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis()),
        hostname,
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        listener_count: servers.len(),
        process_count: servers
            .iter()
            .map(|server| server.pid)
            .collect::<HashSet<_>>()
            .len(),
        exposed_count: servers
            .iter()
            .filter(|server| server.scope != "loopback")
            .count(),
        servers,
    })
}

fn process_details(pids: &HashSet<u32>) -> HashMap<u32, ProcessDetails> {
    if pids.is_empty() {
        return HashMap::new();
    }

    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let Ok(output) = Command::new("ps")
        .args([
            "-ww", "-p", &list, "-o", "pid=", "-o", "ppid=", "-o", "%cpu=", "-o", "%mem=", "-o",
            "lstart=", "-o", "command=",
        ])
        .output()
    else {
        return HashMap::new();
    };

    let mut details = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 10 {
                return None;
            }
            let pid = parts[0].parse().ok()?;
            Some((
                pid,
                ProcessDetails {
                    parent_pid: parts[1].parse().ok(),
                    cpu_percent: parts[2].parse().ok(),
                    memory_percent: parts[3].parse().ok(),
                    started_at: Some(parts[4..9].join(" ")),
                    command: parts[9..].join(" "),
                    executable: String::new(),
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    if let Ok(output) = Command::new("ps")
        .args(["-ww", "-p", &list, "-o", "pid=", "-o", "comm="])
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let line = line.trim_start();
            let Some(separator) = line.find(char::is_whitespace) else {
                continue;
            };
            let Ok(pid) = line[..separator].parse::<u32>() else {
                continue;
            };
            if let Some(detail) = details.get_mut(&pid) {
                detail.executable = line[separator..].trim().to_string();
            }
        }
    }

    details
}

fn parse_lsof(raw: &str, details: &HashMap<u32, ProcessDetails>) -> Vec<ServerInfo> {
    let mut current = ListeningProcess::default();
    let mut seen = HashSet::new();
    let mut servers = Vec::new();

    for line in raw.lines() {
        let Some((field, value)) = line.split_at_checked(1) else {
            continue;
        };
        match field {
            "p" => {
                current = ListeningProcess {
                    pid: value.parse().unwrap_or_default(),
                    ..ListeningProcess::default()
                };
            }
            "c" => current.name = value.to_string(),
            "L" => current.user = value.to_string(),
            "f" => current.descriptor = Some(value.to_string()),
            "n" => {
                let Some((address, port)) = split_endpoint(value) else {
                    continue;
                };
                let identity = (current.pid, address.clone(), port);
                if !seen.insert(identity) {
                    continue;
                }

                let detail = details.get(&current.pid);
                let command = detail
                    .map(|item| item.command.clone())
                    .filter(|item| !item.is_empty())
                    .unwrap_or_else(|| current.name.clone());
                let executable = detail
                    .map(|item| item.executable.clone())
                    .filter(|item| !item.is_empty())
                    .unwrap_or_else(|| current.name.clone());
                let scope = address_scope(&address);
                let ip_version = if address.contains(':') {
                    "IPv6"
                } else {
                    "IPv4"
                };
                let url = probable_url(&current.name, &address, port);

                servers.push(ServerInfo {
                    id: format!("{}:{address}:{port}", current.pid),
                    process: current.name.clone(),
                    pid: current.pid,
                    parent_pid: detail.and_then(|item| item.parent_pid),
                    user: current.user.clone(),
                    address,
                    port,
                    protocol: "TCP",
                    ip_version,
                    scope,
                    status: "listening",
                    file_descriptor: current.descriptor.clone(),
                    command,
                    executable,
                    started_at: detail.and_then(|item| item.started_at.clone()),
                    cpu_percent: detail.and_then(|item| item.cpu_percent),
                    memory_percent: detail.and_then(|item| item.memory_percent),
                    url,
                    pinned: false,
                    custom_command: None,
                });
            }
            _ => {}
        }
    }

    servers.sort_by_key(|server| (server.port, server.pid));
    servers
}

fn split_endpoint(value: &str) -> Option<(String, u16)> {
    let (address, port) = value.rsplit_once(':')?;
    Some((
        address.trim_matches(['[', ']']).to_string(),
        port.parse().ok()?,
    ))
}

fn address_scope(address: &str) -> &'static str {
    if address == "localhost" || address == "::1" || address.starts_with("127.") {
        "loopback"
    } else if matches!(address, "*" | "0.0.0.0" | "::") {
        "all-interfaces"
    } else {
        "network"
    }
}

fn probable_url(process: &str, address: &str, port: u16) -> Option<String> {
    const WEB_PORTS: [u16; 16] = [
        80, 443, 3000, 4000, 4200, 5000, 5150, 5173, 7000, 8000, 8080, 8081, 8443, 8888, 9000,
        11434,
    ];
    let web_process = [
        "http", "node", "python", "ruby", "rails", "cargo", "rust", "loco", "ollama",
    ]
    .iter()
    .any(|name| process.to_ascii_lowercase().contains(name));
    if !WEB_PORTS.contains(&port) && !web_process {
        return None;
    }

    let host = if matches!(address, "*" | "0.0.0.0" | "::") {
        "localhost".to_string()
    } else if address.contains(':') {
        format!("[{address}]")
    } else {
        address.to_string()
    };
    let scheme = if matches!(port, 443 | 8443) {
        "https"
    } else {
        "http"
    };
    Some(format!("{scheme}://{host}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_deduplicates_listeners() {
        let raw = "p42\ncmy-server\nLme\nf8\nn*:5150\nTST=LISTEN\nf9\nn*:5150\nTST=LISTEN\nf10\nn127.0.0.1:9000\nTST=LISTEN\n";
        let details = HashMap::from([(
            42,
            ProcessDetails {
                parent_pid: Some(1),
                command: "/tmp/my-server start".to_string(),
                ..ProcessDetails::default()
            },
        )]);

        let servers = parse_lsof(raw, &details);

        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].port, 5150);
        assert_eq!(servers[0].scope, "all-interfaces");
        assert_eq!(servers[1].scope, "loopback");
        assert_eq!(servers[1].parent_pid, Some(1));
    }
}
