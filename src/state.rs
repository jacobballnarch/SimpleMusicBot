use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DownloadMode {
    Direct,
    Resolve,
}
fn chat_modes() -> &'static Mutex<HashMap<i64, DownloadMode>> {
    static MODES: OnceLock<Mutex<HashMap<i64, DownloadMode>>> = OnceLock::new();
    MODES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_mode(chat_id: i64) -> DownloadMode {
    chat_modes().lock().unwrap().get(&chat_id).copied().unwrap_or(DownloadMode::Direct)
}

pub fn toggle_mode(chat_id: i64) -> DownloadMode {
    let mut modes = chat_modes().lock().unwrap();
    let new_mode = match modes.get(&chat_id) {
        Some(DownloadMode::Direct) => DownloadMode::Resolve,
        _ => DownloadMode::Direct,
    };
    modes.insert(chat_id, new_mode);
    new_mode
}

#[derive(Debug, Clone)]
pub struct ProblemTrack {
    pub url: String,
    pub chat_id: i64,
    pub context: String,
}

fn problem_tracks() -> &'static Mutex<HashMap<String, ProblemTrack>> {
    static TRACKS: OnceLock<Mutex<HashMap<String, ProblemTrack>>> = OnceLock::new();
    TRACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_problem_track(url: String, chat_id: i64, context: String) -> String {
    let pid = uuid::Uuid::new_v4().to_string()[..10].to_string();
    problem_tracks().lock().unwrap().insert(pid.clone(), ProblemTrack { url, chat_id, context });
    pid
}

pub fn pop_problem_track(pid: &str) -> Option<ProblemTrack> {
    problem_tracks().lock().unwrap().remove(pid)
}

