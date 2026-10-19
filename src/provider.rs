use crate::model::{SearchFilters, Wallpaper};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{future::Future, pin::Pin};

const API: &str = "https://wallhaven.cc/api/v1/search";
const REDDIT_OAUTH: &str = "https://www.reddit.com/api/v1/access_token";
const REDDIT_SEARCH: &str = "https://oauth.reddit.com/search.json";

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<ApiWallpaper>,
    meta: Meta,
}
#[derive(Deserialize)]
struct Meta {
    last_page: Option<u32>,
}
#[derive(Deserialize)]
struct ApiWallpaper {
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    purity: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    resolution: String,
    #[serde(default)]
    file_size: Option<u64>,
    #[serde(default)]
    path: String,
    #[serde(default)]
    thumbs: Thumbs,
    #[serde(default)]
    tags: Vec<ApiTag>,
}
#[derive(Default, Deserialize)]
struct Thumbs {
    #[serde(default)]
    small: String,
    #[serde(default)]
    large: String,
}
#[derive(Deserialize)]
struct ApiTag {
    #[serde(default)]
    name: String,
}

#[derive(Debug)]
pub struct Page {
    pub wallpapers: Vec<Wallpaper>,
    pub last_page: u32,
    pub next_cursor: Option<String>,
}

/// Provider interface used by the application search coordinator. New providers
/// can normalize their own API response into the shared `Page` and `Wallpaper` types.
pub trait WallpaperProvider: Send + Sync {
    fn search<'a>(
        &'a self,
        filters: &'a SearchFilters,
        api_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Page>> + Send + 'a>>;
}

pub struct Wallhaven;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Wallhaven,
    Reddit,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Self::Wallhaven => "Wallhaven",
            Self::Reddit => "Reddit",
        }
    }
}

impl WallpaperProvider for Wallhaven {
    fn search<'a>(
        &'a self,
        filters: &'a SearchFilters,
        api_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Page>> + Send + 'a>> {
        Box::pin(search_wallhaven(filters, api_key))
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RedditCredentials {
    pub client_id: String,
    pub client_secret: String,
    pub username: String,
}

pub async fn search(filters: &SearchFilters, api_key: &str) -> Result<Page> {
    Wallhaven.search(filters, api_key).await
}

async fn search_wallhaven(filters: &SearchFilters, api_key: &str) -> Result<Page> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("WallBrowse/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let (sorting, order) = filters.sort.api();
    tracing::debug!(page = filters.page, sorting, "Searching Wallhaven");
    let mut req = client.get(API).query(&[
        ("page", filters.page.to_string()),
        ("per_page", filters.per_page.to_string()),
        ("sorting", sorting.to_owned()),
        ("order", order.to_owned()),
    ]);
    if !filters.query.trim().is_empty() {
        req = req.query(&[("q", filters.query.trim().to_owned())]);
    }
    let cats = filters
        .categories
        .iter()
        .map(|v| if *v { "1" } else { "0" })
        .collect::<String>();
    req = req.query(&[
        ("categories", cats),
        (
            "purity",
            if filters.sfw_only { "100" } else { "111" }.to_owned(),
        ),
    ]);
    if let (Some(w), Some(h)) = (filters.min_width, filters.min_height) {
        req = req.query(&[("atleast", format!("{w}x{h}"))]);
    }
    if let Some(ratio) = &filters.ratio {
        req = req.query(&[("ratios", ratio)]);
    }
    if let Some(color) = &filters.color {
        req = req.query(&[("colors", color)]);
    }
    if !api_key.is_empty() {
        req = req.query(&[("apikey", api_key)]);
    }
    let response = req
        .send()
        .await
        .map_err(|e| e.without_url())
        .context("Could not connect to Wallhaven")?;
    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        bail!("Wallhaven rate limit reached. Please wait and try again.");
    }
    if !status.is_success() {
        bail!("Wallhaven returned HTTP {status}");
    }
    let parsed: SearchResponse = response
        .json()
        .await
        .map_err(|e| e.without_url())
        .context("Wallhaven returned an invalid response")?;
    let wallpapers = parsed
        .data
        .into_iter()
        .filter_map(normalize)
        .filter(|wallpaper| !filters.sfw_only || wallpaper.purity == "sfw")
        .collect();
    Ok(Page {
        wallpapers,
        last_page: parsed.meta.last_page.unwrap_or(1),
        next_cursor: None,
    })
}

fn normalize(item: ApiWallpaper) -> Option<Wallpaper> {
    let (width, height) = item
        .resolution
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))?;
    if item.path.is_empty() || item.thumbs.small.is_empty() {
        return None;
    }
    Some(Wallpaper {
        id: format!("wallhaven:{}", item.id),
        provider: "Wallhaven".into(),
        title: item
            .tags
            .first()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "Untitled wallpaper".into()),
        tags: item.tags.into_iter().map(|t| t.name).collect(),
        thumbnail_url: item.thumbs.small,
        preview_url: if item.thumbs.large.is_empty() {
            item.path.clone()
        } else {
            item.thumbs.large
        },
        download_url: item.path,
        source_url: if item.url.is_empty() {
            format!("https://wallhaven.cc/w/{}", item.id)
        } else {
            item.url
        },
        category: item.category,
        width,
        height,
        file_size: item.file_size,
        purity: item.purity,
        attribution: String::new(),
        license: None,
        download_tracking_url: None,
        photographer_url: None,
    })
}

pub async fn search_source(
    source: Source,
    filters: &SearchFilters,
    credential: &str,
    cursor: Option<&str>,
) -> Result<Page> {
    match source {
        Source::Wallhaven => search(filters, credential).await,
        Source::Reddit => {
            let credentials: RedditCredentials = serde_json::from_str(credential)
                .context("Configure Reddit OAuth credentials in Settings.")?;
            search_reddit(filters, &credentials, cursor).await
        }
    }
}

#[derive(Deserialize)]
struct RedditToken {
    access_token: String,
}

#[derive(Deserialize)]
struct RedditListing {
    data: RedditListingData,
}
#[derive(Deserialize)]
struct RedditListingData {
    #[serde(default)]
    children: Vec<RedditChild>,
    after: Option<String>,
}
#[derive(Deserialize)]
struct RedditChild {
    data: RedditPost,
}
#[derive(Deserialize)]
struct RedditPost {
    id: String,
    title: String,
    author: String,
    subreddit: String,
    permalink: String,
    url: String,
    #[serde(default)]
    is_self: bool,
    #[serde(default)]
    over_18: Option<bool>,
    #[serde(default)]
    post_hint: Option<String>,
    #[serde(default)]
    preview: Option<RedditPreview>,
}
#[derive(Deserialize)]
struct RedditPreview {
    #[serde(default)]
    images: Vec<RedditImage>,
}
#[derive(Deserialize)]
struct RedditImage {
    source: RedditImageVariant,
    #[serde(default)]
    resolutions: Vec<RedditImageVariant>,
}
#[derive(Deserialize)]
struct RedditImageVariant {
    url: String,
    width: u32,
    height: u32,
}

async fn search_reddit(
    filters: &SearchFilters,
    credentials: &RedditCredentials,
    cursor: Option<&str>,
) -> Result<Page> {
    if credentials.client_id.trim().is_empty()
        || credentials.client_secret.trim().is_empty()
        || credentials.username.trim().is_empty()
    {
        bail!("Configure Reddit's approved OAuth client ID, secret, and account name in Settings.");
    }
    if filters.query.trim().is_empty() {
        bail!("Enter a keyword to search Reddit.");
    }
    let username = credentials.username.trim().trim_start_matches("u/");
    if username.is_empty()
        || !username.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        bail!("Enter a valid Reddit username for the API user agent.");
    }
    let user_agent = format!(
        "linux:wallbrowse:{} (by /u/{})",
        env!("CARGO_PKG_VERSION"),
        username
    );
    let client = reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(std::time::Duration::from_secs(25))
        .build()?;
    let token_response = client
        .post(REDDIT_OAUTH)
        .basic_auth(
            credentials.client_id.trim(),
            Some(credentials.client_secret.trim()),
        )
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body("grant_type=client_credentials")
        .send()
        .await
        .map_err(|e| e.without_url())
        .context("Could not connect to Reddit OAuth")?;
    check_reddit_status(token_response.status(), true)?;
    let token: RedditToken = token_response
        .json()
        .await
        .map_err(|e| e.without_url())
        .context("Reddit returned an invalid OAuth response")?;

    let sort = match filters.sort {
        crate::model::SortOrder::DateAdded => "new",
        crate::model::SortOrder::Views => "hot",
        crate::model::SortOrder::Favorites => "top",
        crate::model::SortOrder::Relevance | crate::model::SortOrder::Random => "relevance",
    };
    let mut request = client
        .get(REDDIT_SEARCH)
        .bearer_auth(token.access_token)
        .query(&[
            ("q", filters.query.trim()),
            ("sort", sort),
            ("type", "link"),
            ("limit", &filters.per_page.min(100).to_string()),
            ("include_over_18", "false"),
        ]);
    if let Some(after) = cursor {
        request = request.query(&[("after", after)]);
    }
    let response = request
        .send()
        .await
        .map_err(|e| e.without_url())
        .context("Could not search Reddit")?;
    check_reddit_status(response.status(), false)?;
    let listing: RedditListing = response
        .json()
        .await
        .map_err(|e| e.without_url())
        .context("Reddit returned an invalid search response")?;
    let next_cursor = listing.data.after.filter(|s| !s.is_empty());
    let wallpapers = listing
        .data
        .children
        .into_iter()
        .filter_map(|item| normalize_reddit(item.data, filters.sfw_only))
        .filter(|w| {
            filters.min_width.is_none_or(|n| w.width >= n)
                && filters.min_height.is_none_or(|n| w.height >= n)
        })
        .collect();
    let last_page = if next_cursor.is_some() {
        filters.page.saturating_add(1)
    } else {
        filters.page
    };
    Ok(Page {
        wallpapers,
        last_page,
        next_cursor,
    })
}

fn check_reddit_status(status: reqwest::StatusCode, oauth: bool) -> Result<()> {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        bail!(if oauth {
            "Reddit rejected the OAuth credentials. Verify the registered client and approved access."
        } else {
            "Reddit rejected the access token. Try the search again."
        });
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        bail!(
            "Reddit denied this request. Confirm the app has approved Data API access and the requested content is available."
        );
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        bail!("Reddit rate limit reached. Please wait before searching again.");
    }
    if !status.is_success() {
        bail!("Reddit returned HTTP {status}");
    }
    Ok(())
}

fn normalize_reddit(post: RedditPost, sfw_only: bool) -> Option<Wallpaper> {
    if post.is_self || (sfw_only && post.over_18 != Some(false)) {
        return None;
    }
    let is_image_post = post.post_hint.as_deref() == Some("image");
    let preview_image = post
        .preview
        .and_then(|preview| preview.images.into_iter().next());
    let image = preview_image?;
    if !is_image_post || image.source.width == 0 || image.source.height == 0 {
        return None;
    }
    let thumbnail = image
        .resolutions
        .first()
        .map(|r| reddit_url(&r.url))
        .unwrap_or_else(|| reddit_url(&image.source.url));
    let preview_url = reddit_url(&image.source.url);
    let direct_url = reddit_url(&post.url);
    if thumbnail.is_empty() || preview_url.is_empty() || direct_url.is_empty() {
        return None;
    }
    let source_url = format!("https://www.reddit.com{}", post.permalink);
    let tags = vec![format!("r/{}", post.subreddit)];
    let purity = match post.over_18 {
        Some(false) => "sfw",
        Some(true) => "nsfw",
        None => "unknown",
    };
    Some(Wallpaper {
        id: format!("reddit:{}", post.id),
        provider: "Reddit".into(),
        title: post.title,
        tags,
        thumbnail_url: thumbnail,
        preview_url,
        download_url: direct_url,
        source_url,
        category: format!("r/{}", post.subreddit),
        width: image.source.width,
        height: image.source.height,
        file_size: None,
        purity: purity.into(),
        attribution: format!("u/{} in r/{} on Reddit", post.author, post.subreddit),
        license: None,
        download_tracking_url: None,
        photographer_url: Some(format!("https://www.reddit.com/user/{}", post.author)),
    })
}

fn reddit_url(url: &str) -> String {
    url.replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_preserves_dimensions_and_provider_id() {
        let api = ApiWallpaper {
            id: "abc".into(),
            url: "https://wallhaven.cc/w/abc".into(),
            purity: "sfw".into(),
            category: "general".into(),
            resolution: "1920x1080".into(),
            file_size: Some(42),
            path: "https://i.test/a.jpg".into(),
            thumbs: Thumbs {
                small: "s".into(),
                large: "l".into(),
            },
            tags: vec![ApiTag {
                name: "night".into(),
            }],
        };
        let item = normalize(api).unwrap();
        assert_eq!(item.id, "wallhaven:abc");
        assert_eq!(item.width, 1920);
        assert_eq!(item.tags[0], "night");
    }
    #[test]
    fn malformed_resolution_is_skipped() {
        let api = ApiWallpaper {
            id: "x".into(),
            url: String::new(),
            purity: String::new(),
            category: String::new(),
            resolution: "bad".into(),
            file_size: None,
            path: "url".into(),
            thumbs: Thumbs {
                small: "s".into(),
                ..Default::default()
            },
            tags: vec![],
        };
        assert!(normalize(api).is_none());
    }

    fn reddit_post(over_18: Option<bool>) -> RedditPost {
        RedditPost {
            id: "abc123".into(),
            title: "Mountain night sky".into(),
            author: "wallpaper_artist".into(),
            subreddit: "wallpapers".into(),
            permalink: "/r/wallpapers/comments/abc123/night_sky/".into(),
            url: "https://i.redd.it/original.jpg".into(),
            is_self: false,
            over_18,
            post_hint: Some("image".into()),
            preview: Some(RedditPreview {
                images: vec![RedditImage {
                    source: RedditImageVariant {
                        url: "https://preview.redd.it/original.jpg?x=1&amp;y=2".into(),
                        width: 3840,
                        height: 2160,
                    },
                    resolutions: vec![RedditImageVariant {
                        url: "https://preview.redd.it/thumb.jpg".into(),
                        width: 320,
                        height: 180,
                    }],
                }],
            }),
        }
    }

    #[test]
    fn reddit_normalization_qualifies_identity_and_keeps_author_and_source() {
        let item = normalize_reddit(reddit_post(Some(false)), true).unwrap();
        assert_eq!(item.id, "reddit:abc123");
        assert_eq!(item.provider, "Reddit");
        assert_eq!(item.width, 3840);
        assert_eq!(item.thumbnail_url, "https://preview.redd.it/thumb.jpg");
        assert_eq!(
            item.preview_url,
            "https://preview.redd.it/original.jpg?x=1&y=2"
        );
        assert!(item.attribution.contains("wallpaper_artist"));
        assert!(
            item.source_url
                .starts_with("https://www.reddit.com/r/wallpapers/")
        );
    }

    #[test]
    fn reddit_sfw_filter_rejects_nsfw_and_unclassified_posts() {
        assert!(normalize_reddit(reddit_post(Some(true)), true).is_none());
        assert!(normalize_reddit(reddit_post(None), true).is_none());
        assert!(normalize_reddit(reddit_post(Some(true)), false).is_some());
    }

    #[tokio::test]
    async fn reddit_missing_oauth_credentials_fail_before_network_access() {
        let credentials = RedditCredentials {
            client_id: String::new(),
            client_secret: String::new(),
            username: String::new(),
        };
        let error = search_reddit(&SearchFilters::default(), &credentials, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("approved OAuth client"));
    }

    #[test]
    fn reddit_reports_auth_and_rate_limit_errors_without_response_bodies() {
        assert!(
            check_reddit_status(reqwest::StatusCode::UNAUTHORIZED, true)
                .unwrap_err()
                .to_string()
                .contains("approved access")
        );
        assert!(
            check_reddit_status(reqwest::StatusCode::FORBIDDEN, false)
                .unwrap_err()
                .to_string()
                .contains("denied this request")
        );
        assert!(
            check_reddit_status(reqwest::StatusCode::TOO_MANY_REQUESTS, false)
                .unwrap_err()
                .to_string()
                .contains("rate limit")
        );
    }

    #[test]
    fn reddit_listing_parses_cursor_for_provider_specific_pagination() {
        let listing: RedditListing =
            serde_json::from_str(r#"{"data":{"children":[],"after":"t3_next-post"}}"#).unwrap();
        assert_eq!(listing.data.after.as_deref(), Some("t3_next-post"));
    }
}
