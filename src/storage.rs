use crate::model::Wallpaper;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub api_key: String,
    #[serde(default)]
    pub reddit_client_id: String,
    #[serde(default)]
    pub reddit_client_secret: String,
    #[serde(default)]
    pub reddit_username: String,
    #[serde(default)]
    pub reddit_download_rights_confirmed: bool,
    pub download_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub sfw_only: bool,
    pub grid_density: f32,
    pub cache_limit_mb: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let paths = ProjectDirs::from("org", "WallBrowse", "WallBrowse");
        let download_dir = directories::UserDirs::new()
            .and_then(|d| d.picture_dir().map(|p| p.join("Wallpapers")))
            .unwrap_or_else(|| PathBuf::from("Wallpapers"));
        let cache_dir = paths
            .as_ref()
            .map(|p| p.cache_dir().to_owned())
            .unwrap_or_else(|| PathBuf::from(".cache/wallbrowse"));
        Self {
            api_key: String::new(),
            reddit_client_id: String::new(),
            reddit_client_secret: String::new(),
            reddit_username: String::new(),
            reddit_download_rights_confirmed: false,
            download_dir,
            cache_dir,
            sfw_only: true,
            grid_density: 220.0,
            cache_limit_mb: 512,
        }
    }
}

fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("org", "WallBrowse", "WallBrowse")
}
fn read<T: for<'de> Deserialize<'de>>(path: PathBuf) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
fn write<T: Serialize>(path: PathBuf, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

pub fn load_settings() -> Settings {
    dirs()
        .and_then(|d| read(d.config_dir().join("settings.json")))
        .unwrap_or_default()
}
pub fn save_settings(settings: &Settings) -> anyhow::Result<()> {
    if let Some(d) = dirs() {
        write(d.config_dir().join("settings.json"), settings)?;
    }
    Ok(())
}
pub fn load_favorites() -> Vec<Wallpaper> {
    dirs()
        .and_then(|d| read(d.data_dir().join("favorites.json")))
        .unwrap_or_default()
}
pub fn save_favorites(items: &[Wallpaper]) -> anyhow::Result<()> {
    if let Some(d) = dirs() {
        write(d.data_dir().join("favorites.json"), &items)?;
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub wallpaper: Wallpaper,
    pub path: PathBuf,
    pub downloaded_at: String,
}
pub fn load_history() -> Vec<HistoryEntry> {
    dirs()
        .and_then(|d| read(d.data_dir().join("downloads.json")))
        .unwrap_or_default()
}
pub fn save_history(items: &[HistoryEntry]) -> anyhow::Result<()> {
    if let Some(d) = dirs() {
        write(d.data_dir().join("downloads.json"), &items)?;
    }
    Ok(())
}
