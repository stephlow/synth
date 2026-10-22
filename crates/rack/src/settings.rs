//! Persistent user settings (theme, last patch path).
//!
//! Config lives under the platform config dir via the `directories` crate
//! (XDG on Linux, AppData on Windows, Application Support on macOS).

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::draw::style::ThemeId;
use crate::model::{DEFAULT_PATCH_DIR, DEFAULT_PATCH_FILE};

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "synth";
const APPLICATION: &str = "rack";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserSettings {
    /// Theme label matching [`ThemeId::label`] prefix (stable id key).
    #[serde(default = "default_theme_key")]
    pub theme: String,
    /// Last successful save/load path for the path editor.
    #[serde(default)]
    pub last_patch_path: Option<String>,
}

fn default_theme_key() -> String {
    "nocturne".into()
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: default_theme_key(),
            last_patch_path: Some(repo_relative_default_patch()),
        }
    }
}

impl UserSettings {
    pub fn theme_id(&self) -> ThemeId {
        theme_from_key(&self.theme).unwrap_or(ThemeId::Nocturne)
    }

    pub fn set_theme_id(&mut self, id: ThemeId) {
        self.theme = theme_key(id).into();
    }

    pub fn last_path_or_default(&self) -> String {
        self.last_patch_path
            .clone()
            .unwrap_or_else(repo_relative_default_patch)
    }
}

/// Prefer repo `patches/empty.json` when running from a checkout; else bare name.
fn repo_relative_default_patch() -> String {
    let candidate = Path::new(DEFAULT_PATCH_DIR).join("empty.json");
    if candidate.is_file() {
        return candidate.to_string_lossy().into_owned();
    }
    Path::new(DEFAULT_PATCH_DIR)
        .join(DEFAULT_PATCH_FILE)
        .to_string_lossy()
        .into_owned()
}

fn theme_key(id: ThemeId) -> &'static str {
    match id {
        ThemeId::Nocturne => "nocturne",
        ThemeId::Phosphor => "phosphor",
        ThemeId::Ember => "ember",
        ThemeId::Violet => "violet",
        ThemeId::Mono => "mono",
        ThemeId::Ice => "ice",
        ThemeId::Ocean => "ocean",
        ThemeId::Rose => "rose",
        ThemeId::Crimson => "crimson",
        ThemeId::Forest => "forest",
        ThemeId::Copper => "copper",
        ThemeId::Plasma => "plasma",
        ThemeId::Paper => "paper",
        ThemeId::Indigo => "indigo",
        ThemeId::Toxic => "toxic",
        ThemeId::Aurora => "aurora",
        ThemeId::Midnight => "midnight",
        ThemeId::Dusk => "dusk",
        ThemeId::Sunset => "sunset",
        ThemeId::Fog => "fog",
        ThemeId::Graphite => "graphite",
        ThemeId::Sand => "sand",
        ThemeId::Jade => "jade",
        ThemeId::Magenta => "magenta",
        ThemeId::Honey => "honey",
        ThemeId::Storm => "storm",
        ThemeId::Wine => "wine",
        ThemeId::Cobalt => "cobalt",
        ThemeId::Lava => "lava",
        ThemeId::Bone => "bone",
        ThemeId::Ink => "ink",
        ThemeId::Moss => "moss",
        ThemeId::Steel => "steel",
        ThemeId::Peach => "peach",
        ThemeId::Void => "void",
    }
}

fn theme_from_key(key: &str) -> Option<ThemeId> {
    let k = key.to_ascii_lowercase();
    ThemeId::ALL
        .iter()
        .copied()
        .find(|id| theme_key(*id) == k)
}

/// Platform config directory for rack (`~/.config/synth/rack` on Linux).
pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).map(|p| p.config_dir().to_path_buf())
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

pub fn load() -> UserSettings {
    let Some(path) = settings_path() else {
        warn!("could not resolve config dir; using defaults");
        return UserSettings::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(s) => {
                debug!(path = %path.display(), "loaded settings");
                s
            }
            Err(err) => {
                warn!(%err, path = %path.display(), "settings parse failed; defaults");
                UserSettings::default()
            }
        },
        Err(_) => {
            debug!(path = %path.display(), "no settings file yet");
            UserSettings::default()
        }
    }
}

pub fn save(settings: &UserSettings) {
    let Some(path) = settings_path() else {
        warn!("could not resolve config dir; settings not saved");
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            warn!(%err, "mkdir config dir failed");
            return;
        }
    }
    match serde_json::to_string_pretty(settings) {
        Ok(text) => {
            if let Err(err) = fs::write(&path, text) {
                warn!(%err, path = %path.display(), "write settings failed");
            } else {
                debug!(path = %path.display(), "saved settings");
            }
        }
        Err(err) => warn!(%err, "serialize settings failed"),
    }
}
