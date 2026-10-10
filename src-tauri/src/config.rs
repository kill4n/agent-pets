use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub port: u16,
    pub token: String,
    pub pack: String,
    pub bind_all: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { port: 7433, token: generate_token(), pack: "default".into(), bind_all: false }
    }
}

impl Config {
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path).ok()
            .and_then(|t| serde_json::from_str::<Config>(&t).ok())
            .filter(|c| c.token.len() == 32)
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
    pub fn hook_url(&self) -> String { format!("http://127.0.0.1:{}/hook", self.port) }
    pub fn bind_addr(&self) -> SocketAddr {
        let ip = if self.bind_all { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
        SocketAddr::new(IpAddr::V4(ip), self.port)
    }
}

pub fn generate_token() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "kill4n", "codepets")
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".buddy"))
}
pub fn config_path() -> PathBuf { config_dir().join("config.json") }
pub fn positions_path() -> PathBuf { config_dir().join("positions.json") }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn token_is_32_hex() {
        let t = generate_token();
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(t, generate_token());
    }
    #[test] fn load_missing_creates_defaults_with_token() {
        let d = tempfile::tempdir().unwrap();
        let c = Config::load(&d.path().join("config.json"));
        assert_eq!(c.port, 7433); assert_eq!(c.pack, "default"); assert!(!c.bind_all);
        assert_eq!(c.token.len(), 32);
    }
    #[test] fn save_then_load_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.json");
        let mut c = Config::load(&p); c.port = 8000; c.pack = "cat".into();
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
    }
    #[test] fn load_invalid_json_returns_defaults() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.json");
        std::fs::write(&p, "nope").unwrap();
        assert_eq!(Config::load(&p).port, 7433);
    }
    #[test] fn urls_and_bind() {
        let mut c = Config::load(std::path::Path::new("/nonexistent/x.json"));
        assert_eq!(c.hook_url(), "http://127.0.0.1:7433/hook");
        assert_eq!(c.bind_addr().to_string(), "127.0.0.1:7433");
        c.bind_all = true;
        assert_eq!(c.bind_addr().to_string(), "0.0.0.0:7433");
    }
    #[cfg(unix)]
    #[test] fn save_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.json");
        Config::load(&p).save(&p).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
