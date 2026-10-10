use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pos { pub x: i32, pub y: i32 }

pub struct Positions { map: BTreeMap<String, Pos>, path: PathBuf }

fn key(host: &str, name: &str) -> String { format!("{host}:{name}") }

impl Positions {
    pub fn load(path: PathBuf) -> Self {
        let map = std::fs::read_to_string(&path).ok()
            .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { map, path }
    }
    pub fn get(&self, host: &str, name: &str) -> Option<Pos> { self.map.get(&key(host, name)).copied() }
    pub fn set(&mut self, host: &str, name: &str, p: Pos) {
        self.map.insert(key(host, name), p);
        if let Some(parent) = self.path.parent() { let _ = std::fs::create_dir_all(parent); }
        if let Err(e) = std::fs::write(&self.path, serde_json::to_string_pretty(&self.map).unwrap()) {
            log::warn!("cannot save positions: {e}");
        }
    }
    pub fn cascade(&self, index: usize, screen_w: i32, screen_h: i32, win_w: i32, win_h: i32) -> Pos {
        const MARGIN: i32 = 24; const DX: i32 = 40; const DY: i32 = 30;
        let start_x = screen_w - win_w - MARGIN;
        let start_y = screen_h - win_h - MARGIN;
        let per_row = ((start_x / DX).max(1)) as usize;
        let col = (index % per_row) as i32;
        let row = (index / per_row) as i32;
        let x = (start_x - col * DX).max(0);
        let y = (start_y - col * DY - row * (win_h + MARGIN)).max(0);
        Pos { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn set_get_and_persist() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("positions.json");
        let mut ps = Positions::load(p.clone());
        assert!(ps.get("h", "buddy-1").is_none());
        ps.set("h", "buddy-1", Pos { x: 10, y: 20 });
        let again = Positions::load(p);
        assert_eq!(again.get("h", "buddy-1"), Some(Pos { x: 10, y: 20 }));
    }
    #[test] fn same_name_new_session_finds_old_position() {
        let d = tempfile::tempdir().unwrap();
        let mut ps = Positions::load(d.path().join("p.json"));
        ps.set("h", "buddy-1", Pos { x: 1, y: 2 });
        // key is host+name, so any session_id with that name resolves
        assert_eq!(ps.get("h", "buddy-1"), Some(Pos { x: 1, y: 2 }));
        assert_eq!(ps.get("other-host", "buddy-1"), None);
    }
    #[test] fn load_invalid_file_is_empty() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("p.json");
        std::fs::write(&p, "garbage").unwrap();
        assert!(Positions::load(p).get("h", "x").is_none());
    }
    #[test] fn cascade_from_bottom_right_and_wraps() {
        let ps = Positions::load(std::path::PathBuf::from("/nonexistent/p.json"));
        let p0 = ps.cascade(0, 1000, 800, 128, 128);
        assert_eq!(p0, Pos { x: 1000 - 128 - 24, y: 800 - 128 - 24 });
        let p1 = ps.cascade(1, 1000, 800, 128, 128);
        assert_eq!(p1, Pos { x: p0.x - 40, y: p0.y - 30 });
        let far = ps.cascade(30, 1000, 800, 128, 128);
        assert!(far.x >= 0 && far.y >= 0);
    }
}
