use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Wallpaper {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub tags: Vec<String>,
    pub thumbnail_url: String,
    pub preview_url: String,
    pub download_url: String,
    pub source_url: String,
    pub category: String,
    pub width: u32,
    pub height: u32,
    pub file_size: Option<u64>,
    pub purity: String,
    /// Creator or provider attribution displayed with the wallpaper.
    #[serde(default)]
    pub attribution: String,
    /// Informational license label supplied by the provider, when available.
    #[serde(default)]
    pub license: Option<String>,
    /// Provider endpoint that must be called when the user downloads the image.
    #[serde(default)]
    pub download_tracking_url: Option<String>,
    #[serde(default)]
    pub photographer_url: Option<String>,
}

impl Wallpaper {
    pub fn resolution(&self) -> String {
        format!("{} × {}", self.width, self.height)
    }
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            0.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortOrder {
    Relevance,
    #[default]
    DateAdded,
    Views,
    Favorites,
    Random,
}

impl SortOrder {
    pub fn api(self) -> (&'static str, &'static str) {
        match self {
            Self::Relevance => ("relevance", "desc"),
            Self::DateAdded => ("date_added", "desc"),
            Self::Views => ("views", "desc"),
            Self::Favorites => ("favorites", "desc"),
            Self::Random => ("random", "desc"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchFilters {
    pub query: String,
    pub page: u32,
    pub sort: SortOrder,
    pub min_width: Option<u32>,
    pub min_height: Option<u32>,
    pub ratio: Option<String>,
    pub color: Option<String>,
    pub categories: [bool; 3],
    pub sfw_only: bool,
    pub per_page: u8,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            query: String::new(),
            page: 1,
            sort: SortOrder::DateAdded,
            min_width: None,
            min_height: None,
            ratio: None,
            color: None,
            categories: [true; 3],
            sfw_only: true,
            per_page: 24,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sfw_and_start_at_first_page() {
        let filters = SearchFilters::default();
        assert!(filters.sfw_only);
        assert_eq!(filters.page, 1);
        assert_eq!(filters.per_page, 24);
        assert_eq!(filters.sort, SortOrder::DateAdded);
    }

    #[test]
    fn sort_options_map_to_wallhaven_values() {
        assert_eq!(SortOrder::Favorites.api(), ("favorites", "desc"));
        assert_eq!(SortOrder::Random.api(), ("random", "desc"));
    }

    #[test]
    fn old_saved_wallpapers_load_with_default_provider_metadata() {
        let old = serde_json::json!({
            "id":"wallhaven:old", "provider":"Wallhaven", "title":"Old favorite", "tags":[],
            "thumbnail_url":"https://example.test/thumb", "preview_url":"https://example.test/preview",
            "download_url":"https://example.test/image.jpg", "source_url":"https://wallhaven.cc/w/old",
            "category":"general", "width":1920, "height":1080, "file_size":null, "purity":"sfw"
        });
        let item: Wallpaper = serde_json::from_value(old).unwrap();
        assert!(item.attribution.is_empty());
        assert!(item.download_tracking_url.is_none());
    }
}
