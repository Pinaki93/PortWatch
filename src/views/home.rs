use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub endpoints: [&'static str; 5],
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ServerInfo {
    pub id: String,
    pub process: String,
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub user: String,
    pub address: String,
    pub port: u16,
    pub protocol: &'static str,
    pub ip_version: &'static str,
    pub scope: &'static str,
    pub status: &'static str,
    pub file_descriptor: Option<String>,
    pub command: String,
    pub executable: String,
    pub started_at: Option<String>,
    pub cpu_percent: Option<f32>,
    pub memory_percent: Option<f32>,
    pub url: Option<String>,
    pub pinned: bool,
    pub custom_command: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerSnapshot {
    pub scanned_at: u128,
    pub hostname: String,
    pub os: &'static str,
    pub architecture: &'static str,
    pub listener_count: usize,
    pub process_count: usize,
    pub exposed_count: usize,
    pub servers: Vec<ServerInfo>,
}

#[derive(Debug, Serialize)]
pub struct ScanError {
    pub error: &'static str,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct PinServerParams {
    pub process: String,
    pub address: String,
    pub port: u16,
    pub url: Option<String>,
    pub custom_command: Option<String>,
    pub pinned: bool,
}

#[derive(Debug, Deserialize)]
pub struct OpenServerParams {
    pub process: String,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct CommandResponse {
    pub status: &'static str,
}
