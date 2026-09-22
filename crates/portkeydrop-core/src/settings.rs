//! Application settings: defaults, JSON persistence, and startup resolution.
//!
//! Loading is deliberately forgiving. A malformed or partially-written
//! `settings.json` yields defaults rather than an error, because failing to
//! start over a bad preferences file would be worse than losing preferences.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::portable;
use crate::sound_events::normalize_known_muted_sound_events;

/// File name of the settings document inside the config directory.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// How to handle a transfer whose destination already exists.
pub mod overwrite_mode {
    pub const ASK: &str = "ask";
    pub const OVERWRITE: &str = "overwrite";
    pub const SKIP: &str = "skip";
    pub const RENAME: &str = "rename";
}

/// Default download directory: `~/Downloads`.
fn default_download_dir() -> String {
    portable::home_dir()
        .join("Downloads")
        .to_string_lossy()
        .into_owned()
}

fn default_true() -> bool {
    true
}

/// Transfer behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransferSettings {
    /// Number of transfers that may run at once.
    pub concurrent_transfers: usize,
    /// One of [`overwrite_mode`].
    pub overwrite_mode: String,
    /// Resume interrupted downloads instead of restarting them.
    pub resume_partial: bool,
    /// Carry modification times across where the protocol allows it.
    pub preserve_timestamps: bool,
    /// Follow symbolic links when walking directories.
    pub follow_symlinks: bool,
    /// Where downloads land when no other destination is chosen.
    pub default_download_dir: String,
}

impl Default for TransferSettings {
    fn default() -> Self {
        Self {
            concurrent_transfers: 2,
            overwrite_mode: overwrite_mode::ASK.to_string(),
            resume_partial: true,
            preserve_timestamps: true,
            follow_symlinks: false,
            default_download_dir: default_download_dir(),
        }
    }
}

/// File list presentation and announcement verbosity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    /// Announce "N items" after a directory listing loads.
    pub announce_file_count: bool,
    /// Announce transfer progress every N percent.
    pub progress_interval: u32,
    /// Include dotfiles and hidden entries in listings.
    pub show_hidden_files: bool,
    /// One of `name`, `size`, `modified`, `type`.
    pub sort_by: String,
    pub sort_ascending: bool,
    /// One of `relative`, `absolute`.
    pub date_format: String,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            announce_file_count: true,
            progress_interval: 25,
            show_hidden_files: false,
            sort_by: "name".to_string(),
            sort_ascending: true,
            date_format: "relative".to_string(),
        }
    }
}

/// Defaults applied to new connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionDefaults {
    /// One of the values in `protocols::SUPPORTED_PROTOCOL_VALUES`.
    pub protocol: String,
    /// Connect timeout in seconds.
    pub timeout: u64,
    /// Keepalive interval in seconds.
    pub keepalive: u64,
    pub max_retries: u32,
    /// FTP only: use passive mode.
    pub passive_mode: bool,
    /// FTP only: upgrade the control connection with `AUTH SSL`.
    pub ftp_explicit_ssl: bool,
    /// One of `ask`, `always`, `never`.
    pub verify_host_keys: String,
}

impl Default for ConnectionDefaults {
    fn default() -> Self {
        Self {
            protocol: "sftp".to_string(),
            timeout: 30,
            keepalive: 60,
            max_retries: 3,
            passive_mode: true,
            ftp_explicit_ssl: false,
            verify_host_keys: "ask".to_string(),
        }
    }
}

/// Stored values for [`SpeechSettings::backend`].
pub mod speech_backend {
    /// Never speak, even when a screen reader is running.
    pub const OFF: &str = "off";
    /// Speak only through a screen reader. A system voice is not used.
    pub const SCREEN_READER: &str = "screen_reader";
    /// A screen reader when one is running, otherwise a system voice.
    pub const AUTOMATIC: &str = "automatic";
}

/// Speech output tuning, in 0–100 units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechSettings {
    /// One of [`speech_backend`]. Anything else is treated as automatic.
    pub backend: String,
    /// Set once the user has chosen a backend, including by answering the
    /// startup question. Until then, automatic does not speak through a
    /// system voice.
    pub backend_chosen: bool,
    pub rate: i32,
    pub volume: i32,
    /// One of `minimal`, `normal`, `verbose`.
    pub verbosity: String,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            backend: speech_backend::AUTOMATIC.to_string(),
            backend_chosen: false,
            rate: 50,
            volume: 100,
            verbosity: "normal".to_string(),
        }
    }
}

impl SpeechSettings {
    /// The backend preference, with an unknown stored value read as automatic.
    pub fn backend_preference(&self) -> &str {
        match self.backend.as_str() {
            speech_backend::OFF | speech_backend::SCREEN_READER | speech_backend::AUTOMATIC => {
                self.backend.as_str()
            }
            _ => speech_backend::AUTOMATIC,
        }
    }

    /// Whether announcements should reach a backend.
    ///
    /// A system voice is used only after the user has chosen automatic.
    /// Leaving the preference untouched, which is what an existing settings
    /// file looks like, does not speak through one.
    pub fn announcements_enabled(&self, screen_reader: bool, has_backend: bool) -> bool {
        if !has_backend {
            return false;
        }
        match self.backend_preference() {
            speech_backend::OFF => false,
            speech_backend::SCREEN_READER => screen_reader,
            _ => screen_reader || self.backend_chosen,
        }
    }

    /// Whether to ask about spoken progress through a system voice.
    ///
    /// Asked once, and only when nothing is speaking through a screen reader.
    pub fn offer_spoken_progress(&self, screen_reader: bool, has_backend: bool) -> bool {
        has_backend
            && !screen_reader
            && !self.backend_chosen
            && self.backend_preference() != speech_backend::OFF
    }

    /// Record the answer to the spoken-progress question.
    pub fn answer_spoken_progress(&mut self, yes: bool) {
        self.backend_chosen = true;
        self.backend = if yes {
            speech_backend::AUTOMATIC.to_string()
        } else {
            speech_backend::SCREEN_READER.to_string()
        };
    }
}

/// Whether `name` is a screen reader rather than a text-to-speech engine.
///
/// Prism names its own backends ("NVDA", "SAPI", "VoiceOver (macOS)"). A
/// voice such as SAPI, OneCore, or AVSpeech is not a screen reader, and
/// speaking through it is a choice rather than the default.
pub fn is_screen_reader_backend(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    const READERS: &[&str] = &[
        "nvda",
        "jaws",
        "voiceover",
        "orca",
        "zoomtext",
        "zdsr",
        "sensereader",
        "system access",
        "window-eyes",
        "windoweyes",
        "uia",
        "pc-talker",
        "boypcreader",
        "talkback",
    ];
    READERS
        .iter()
        .any(|reader| name == *reader || name.starts_with(&format!("{reader} ")))
}

/// Sound pack selection and per-event muting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub sound_enabled: bool,
    /// Directory name of the active pack.
    pub sound_pack: String,
    /// Event keys that should stay silent.
    pub muted_sound_events: Vec<String>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_pack: "default".to_string(),
            muted_sound_events: crate::sound_events::DEFAULT_MUTED_SOUND_EVENTS
                .iter()
                .map(|event| event.to_string())
                .collect(),
        }
    }
}

/// Window, startup, and update-channel preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Reopen the last local folder instead of the home directory.
    pub remember_last_local_folder_on_startup: bool,
    /// The folder to reopen; cleared when the preference above is off.
    pub last_local_folder: Option<String>,
    pub auto_update_enabled: bool,
    pub update_check_interval_hours: u32,
    /// One of `stable`, `nightly`.
    pub update_channel: String,
    pub show_notification_area_icon: bool,
    pub minimize_to_notification_area_on_close: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            remember_last_local_folder_on_startup: true,
            last_local_folder: None,
            auto_update_enabled: true,
            update_check_interval_hours: 24,
            update_channel: "stable".to_string(),
            show_notification_area_icon: true,
            minimize_to_notification_area_on_close: false,
        }
    }
}

/// The complete settings document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub transfer: TransferSettings,
    pub display: DisplaySettings,
    pub connection: ConnectionDefaults,
    pub speech: SpeechSettings,
    pub audio: AudioSettings,
    pub app: AppSettings,
}

// `default_true` is referenced by serde-generated code for booleans that
// default to on in future revisions; keep it available without a warning.
#[allow(dead_code)]
fn _unused_default_true() -> bool {
    default_true()
}

impl Settings {
    /// Parse settings from JSON text, filling in defaults for anything absent.
    ///
    /// Unknown fields are ignored, and unknown muted-sound keys are dropped.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let mut settings: Settings = serde_json::from_str(text)?;
        settings.audio.muted_sound_events =
            normalize_known_muted_sound_events(&settings.audio.muted_sound_events);
        Ok(settings)
    }

    /// Serialise to the on-disk JSON shape.
    ///
    /// When the app is not remembering the last local folder, the stored value
    /// is written as `null` so a stale path never survives the preference
    /// being turned off.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut copy = self.clone();
        if !copy.app.remember_last_local_folder_on_startup {
            copy.app.last_local_folder = None;
        }
        serde_json::to_string_pretty(&copy)
    }
}

/// Path of the settings document inside `config_dir`.
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE_NAME)
}

/// Load settings from `config_dir`, returning defaults when absent or invalid.
pub fn load_settings(config_dir: &Path) -> Settings {
    let path = settings_path(config_dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    match Settings::from_json(&text) {
        Ok(settings) => settings,
        Err(err) => {
            log::warn!("failed to load settings from {}: {err}", path.display());
            Settings::default()
        }
    }
}

/// Write settings into `config_dir`, creating the directory if needed.
pub fn save_settings(settings: &Settings, config_dir: &Path) -> std::io::Result<()> {
    crate::private_files::ensure_private_dir(config_dir)?;
    let text = settings
        .to_json()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    std::fs::write(settings_path(config_dir), text)
}

/// Decide which local folder to open at startup.
///
/// Falls back to `fallback` (or the home directory) when the preference is off,
/// nothing was stored, or the stored folder no longer exists. A stale entry is
/// cleared from `settings` as a side effect so it is not retried next launch.
pub fn resolve_startup_local_folder(settings: &mut Settings, fallback: Option<&Path>) -> PathBuf {
    let fallback = fallback
        .map(Path::to_path_buf)
        .unwrap_or_else(portable::home_dir);

    if !settings.app.remember_last_local_folder_on_startup {
        return fallback;
    }

    let Some(saved) = settings
        .app
        .last_local_folder
        .as_deref()
        .filter(|s| !s.is_empty())
    else {
        return fallback;
    };

    let path = expand_user(saved);
    if path.is_dir() {
        return path.canonicalize().unwrap_or(path);
    }

    log::warn!("saved local folder is unavailable: {saved}");
    settings.app.last_local_folder = None;
    fallback
}

/// Record `path` as the last local folder, honouring the user's preference.
///
/// Returns whether `settings` actually changed, so callers only rewrite the
/// file when there is something new to store.
pub fn update_last_local_folder(settings: &mut Settings, path: &Path) -> bool {
    if !settings.app.remember_last_local_folder_on_startup {
        if settings.app.last_local_folder.is_some() {
            settings.app.last_local_folder = None;
            return true;
        }
        return false;
    }

    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let resolved = resolved.to_string_lossy().into_owned();
    if settings.app.last_local_folder.as_deref() == Some(resolved.as_str()) {
        return false;
    }
    settings.app.last_local_folder = Some(resolved);
    true
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_user(path: &str) -> PathBuf {
    if path == "~" {
        return portable::home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return portable::home_dir().join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn defaults_match_the_documented_values() {
        let settings = Settings::default();
        assert_eq!(settings.transfer.concurrent_transfers, 2);
        assert_eq!(settings.transfer.overwrite_mode, "ask");
        assert!(settings.transfer.resume_partial);
        assert!(!settings.transfer.follow_symlinks);
        assert_eq!(settings.display.progress_interval, 25);
        assert_eq!(settings.connection.protocol, "sftp");
        assert_eq!(settings.connection.timeout, 30);
        assert_eq!(settings.speech.backend, speech_backend::AUTOMATIC);
        assert!(!settings.speech.backend_chosen);
        assert_eq!(settings.speech.rate, 50);
        assert_eq!(settings.speech.volume, 100);
        assert!(settings.audio.sound_enabled);
        assert_eq!(settings.audio.sound_pack, "default");
        assert_eq!(settings.app.update_channel, "stable");
        assert!(settings.app.remember_last_local_folder_on_startup);
        assert_eq!(settings.app.last_local_folder, None);
    }

    #[test]
    fn loading_a_missing_file_yields_defaults() {
        let dir = TempDir::new().unwrap();
        assert_eq!(load_settings(dir.path()), Settings::default());
    }

    #[test]
    fn loading_malformed_json_yields_defaults_instead_of_failing() {
        let dir = TempDir::new().unwrap();
        std::fs::write(settings_path(dir.path()), "{not json").unwrap();
        assert_eq!(load_settings(dir.path()), Settings::default());
    }

    #[test]
    fn partial_documents_keep_defaults_for_absent_sections() {
        let settings = Settings::from_json(r#"{"speech": {"rate": 80}}"#).unwrap();
        assert_eq!(settings.speech.rate, 80);
        // Untouched fields in the same section keep their defaults...
        assert_eq!(settings.speech.volume, 100);
        assert_eq!(settings.speech.backend, speech_backend::AUTOMATIC);
        assert!(!settings.speech.backend_chosen);
        // ...as do entirely absent sections.
        assert_eq!(settings.connection.protocol, "sftp");
    }

    #[test]
    fn an_unanswered_automatic_preference_does_not_speak_through_a_voice() {
        let speech = SpeechSettings::default();
        assert!(!speech.announcements_enabled(false, true));
        assert!(speech.offer_spoken_progress(false, true));
        // A screen reader is used without asking.
        assert!(speech.announcements_enabled(true, true));
        assert!(!speech.offer_spoken_progress(true, true));
        // Nothing to speak through means no question either.
        assert!(!speech.offer_spoken_progress(false, false));
    }

    #[test]
    fn the_spoken_progress_answer_is_remembered() {
        let mut speech = SpeechSettings::default();
        speech.answer_spoken_progress(false);
        assert_eq!(speech.backend, speech_backend::SCREEN_READER);
        assert!(speech.backend_chosen);
        assert!(!speech.announcements_enabled(false, true));
        assert!(speech.announcements_enabled(true, true));
        assert!(!speech.offer_spoken_progress(false, true));

        speech.answer_spoken_progress(true);
        assert_eq!(speech.backend, speech_backend::AUTOMATIC);
        assert!(speech.announcements_enabled(false, true));
    }

    #[test]
    fn turning_speech_off_silences_a_screen_reader_too() {
        let speech = SpeechSettings {
            backend: speech_backend::OFF.to_string(),
            backend_chosen: true,
            ..SpeechSettings::default()
        };
        assert!(!speech.announcements_enabled(true, true));
        assert!(!speech.offer_spoken_progress(false, true));
    }

    #[test]
    fn screen_reader_names_are_not_system_voices() {
        assert!(is_screen_reader_backend("NVDA"));
        assert!(is_screen_reader_backend("JAWS"));
        assert!(is_screen_reader_backend("VoiceOver (macOS)"));
        assert!(is_screen_reader_backend("Orca"));
        assert!(!is_screen_reader_backend("SAPI"));
        assert!(!is_screen_reader_backend("OneCore"));
        assert!(!is_screen_reader_backend("AVSpeech"));
        assert!(!is_screen_reader_backend("speech-dispatcher"));
    }

    #[test]
    fn unknown_fields_are_ignored_rather_than_rejected() {
        let settings =
            Settings::from_json(r#"{"speech": {"rate": 30, "colour": "blue"}, "moon": 1}"#)
                .unwrap();
        assert_eq!(settings.speech.rate, 30);
    }

    #[test]
    fn unknown_muted_sound_events_are_dropped_on_load() {
        let settings = Settings::from_json(
            r#"{"audio": {"muted_sound_events": ["success", "bogus_event", "success"]}}"#,
        )
        .unwrap();
        assert_eq!(settings.audio.muted_sound_events, vec!["success"]);
    }

    #[test]
    fn settings_round_trip_through_disk() {
        let dir = TempDir::new().unwrap();
        let mut settings = Settings::default();
        settings.speech.rate = 75;
        settings.audio.muted_sound_events = vec!["transfer_failed".to_string()];
        save_settings(&settings, dir.path()).unwrap();
        assert_eq!(load_settings(dir.path()), settings);
    }

    #[test]
    fn saving_clears_the_last_folder_when_the_preference_is_off() {
        let dir = TempDir::new().unwrap();
        let mut settings = Settings::default();
        settings.app.remember_last_local_folder_on_startup = false;
        settings.app.last_local_folder = Some("/somewhere".to_string());
        save_settings(&settings, dir.path()).unwrap();
        assert_eq!(load_settings(dir.path()).app.last_local_folder, None);
    }

    #[test]
    fn startup_folder_uses_the_fallback_when_remembering_is_off() {
        let fallback = TempDir::new().unwrap();
        let saved = TempDir::new().unwrap();
        let mut settings = Settings::default();
        settings.app.remember_last_local_folder_on_startup = false;
        settings.app.last_local_folder = Some(saved.path().to_string_lossy().into_owned());
        assert_eq!(
            resolve_startup_local_folder(&mut settings, Some(fallback.path())),
            fallback.path().to_path_buf()
        );
    }

    #[test]
    fn startup_folder_uses_the_saved_directory_when_it_still_exists() {
        let fallback = TempDir::new().unwrap();
        let saved = TempDir::new().unwrap();
        let mut settings = Settings::default();
        settings.app.last_local_folder = Some(saved.path().to_string_lossy().into_owned());
        let resolved = resolve_startup_local_folder(&mut settings, Some(fallback.path()));
        assert_eq!(
            resolved.canonicalize().unwrap(),
            saved.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn a_vanished_saved_folder_falls_back_and_is_forgotten() {
        let fallback = TempDir::new().unwrap();
        let mut settings = Settings::default();
        settings.app.last_local_folder = Some("/definitely/not/a/real/folder".to_string());
        let resolved = resolve_startup_local_folder(&mut settings, Some(fallback.path()));
        assert_eq!(resolved, fallback.path().to_path_buf());
        assert_eq!(settings.app.last_local_folder, None);
    }

    #[test]
    fn recording_the_local_folder_reports_whether_anything_changed() {
        let dir = TempDir::new().unwrap();
        let mut settings = Settings::default();
        assert!(update_last_local_folder(&mut settings, dir.path()));
        // Recording the same folder again is a no-op.
        assert!(!update_last_local_folder(&mut settings, dir.path()));
    }

    #[test]
    fn recording_clears_the_folder_when_remembering_is_off() {
        let dir = TempDir::new().unwrap();
        let mut settings = Settings::default();
        settings.app.remember_last_local_folder_on_startup = false;
        settings.app.last_local_folder = Some("/old".to_string());
        assert!(update_last_local_folder(&mut settings, dir.path()));
        assert_eq!(settings.app.last_local_folder, None);
        // Already cleared: nothing further to change.
        assert!(!update_last_local_folder(&mut settings, dir.path()));
    }

    #[test]
    fn tilde_expansion_resolves_against_the_home_directory() {
        let home = portable::home_dir();
        assert_eq!(expand_user("~"), home);
        assert_eq!(expand_user("~/Downloads"), home.join("Downloads"));
        assert_eq!(
            expand_user("/absolute/path"),
            PathBuf::from("/absolute/path")
        );
    }
}
