//! What Craft Hub has installed, saved as JSON in `~/.local/share/crafthub/state.json`.
//! The format is the same one the earlier Python version used, so installs carry over.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Installed {
    /// Release tag (e.g. `v0.5.0`), or a version string for source builds.
    pub version: String,
    /// The installed AppImage or binary.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Whether the download matched the release's SHA256SUMS.txt.
    #[serde(default)]
    pub verified: bool,
    /// Built from source rather than downloaded.
    #[serde(default)]
    pub source: bool,
    #[serde(default)]
    pub installed_at: u64,
}

impl Installed {
    pub fn exists(&self) -> bool {
        Path::new(&self.path).is_file()
    }
}

pub type State = BTreeMap<String, Installed>;

static LOCK: Mutex<()> = Mutex::new(());

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load() -> State {
    fs::read_to_string(paths::state_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(state: &State) -> std::io::Result<()> {
    let file = paths::state_file();
    fs::create_dir_all(paths::hub_dir())?;
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(state).unwrap_or_default())?;
    fs::rename(tmp, file)
}

/// Set (`Some`) or remove (`None`) one app's entry.
pub fn update(app_id: &str, info: Option<Installed>) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = load();
    match info {
        Some(i) => {
            st.insert(app_id.to_string(), i);
        }
        None => {
            st.remove(app_id);
        }
    }
    save(&st)
}

/// The app's install record, only if its file is still there.
pub fn installed(app_id: &str) -> Option<Installed> {
    load().remove(app_id).filter(Installed::exists)
}
