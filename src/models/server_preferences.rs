use std::{fs, path::Path, sync::Mutex};

use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

const PREFERENCES_PATH: &str = "servers.json";
// ponytail: this lock coordinates one app process; move to a database if multi-process writes matter.
static PREFERENCES_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PinnedServer {
    pub process: String,
    pub address: String,
    pub port: u16,
    pub url: Option<String>,
    pub custom_command: Option<String>,
}

impl PinnedServer {
    pub fn matches(&self, process: &str, address: &str, port: u16) -> bool {
        self.process == process && self.address == address && self.port == port
    }

    fn normalize(mut self) -> Result<Self> {
        self.process = self.process.trim().to_string();
        self.address = self.address.trim().to_string();
        self.url = self.url.and_then(trimmed);
        self.custom_command = self.custom_command.and_then(trimmed);

        if self.process.is_empty() || self.address.is_empty() {
            return Err(Error::BadRequest(
                "process and address are required".to_string(),
            ));
        }
        if self
            .custom_command
            .as_ref()
            .is_some_and(|value| value.len() > 4096)
        {
            return Err(Error::BadRequest(
                "custom command must be 4096 characters or fewer".to_string(),
            ));
        }

        Ok(self)
    }

    pub fn rendered_command(&self) -> Option<String> {
        let command = self.custom_command.as_ref()?;
        let host = if matches!(self.address.as_str(), "*" | "0.0.0.0" | "::") {
            "localhost"
        } else {
            &self.address
        };
        Some(
            command
                .replace("{url}", &shell_quote(self.url.as_deref().unwrap_or("")))
                .replace("{host}", &shell_quote(host))
                .replace("{port}", &self.port.to_string()),
        )
    }
}

pub fn list() -> Result<Vec<PinnedServer>> {
    let _guard = PREFERENCES_LOCK
        .lock()
        .map_err(|_| Error::InternalServerError)?;
    read_from(Path::new(PREFERENCES_PATH))
}

pub fn set(server: PinnedServer, pinned: bool) -> Result<Vec<PinnedServer>> {
    let _guard = PREFERENCES_LOCK
        .lock()
        .map_err(|_| Error::InternalServerError)?;
    let server = server.normalize()?;
    set_at(Path::new(PREFERENCES_PATH), server, pinned)
}

fn set_at(path: &Path, server: PinnedServer, pinned: bool) -> Result<Vec<PinnedServer>> {
    let mut servers = read_from(path)?;
    servers.retain(|item| !item.matches(&server.process, &server.address, server.port));
    if pinned {
        servers.push(server);
        servers.sort_by(|left, right| {
            (&left.process, &left.address, left.port).cmp(&(
                &right.process,
                &right.address,
                right.port,
            ))
        });
    }
    write_to(path, &servers)?;
    Ok(servers)
}

fn trimmed(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn read_from(path: &Path) -> Result<Vec<PinnedServer>> {
    match fs::read(path) {
        Ok(contents) => Ok(serde_json::from_slice(&contents)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn write_to(path: &Path, servers: &[PinnedServer]) -> Result<()> {
    let temporary = path.with_extension("json.tmp");
    let mut contents = serde_json::to_vec_pretty(servers)?;
    contents.push(b'\n');
    fs::write(&temporary, contents)?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_a_json_array_and_renders_placeholders() {
        let path = std::env::temp_dir().join(format!(
            "port-watch-servers-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let server = PinnedServer {
            process: " cargo ".to_string(),
            address: " 0.0.0.0 ".to_string(),
            port: 5150,
            url: Some("http://localhost:5150".to_string()),
            custom_command: Some("open {url} --host {host} --port {port}".to_string()),
        }
        .normalize()
        .unwrap();

        let saved = set_at(&path, server.clone(), true).unwrap();

        assert_eq!(saved, vec![server.clone()]);
        assert_eq!(
            server.rendered_command().as_deref(),
            Some("open 'http://localhost:5150' --host 'localhost' --port 5150")
        );
        assert!(
            serde_json::from_slice::<Vec<serde_json::Value>>(&fs::read(&path).unwrap()).is_ok()
        );
        assert!(set_at(&path, server, false).unwrap().is_empty());
        assert!(read_from(&path).unwrap().is_empty());
        fs::remove_file(path).unwrap();
    }
}
