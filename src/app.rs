use crate::{
    model::{SearchFilters, SortOrder, Wallpaper},
    provider::{self, Source},
    storage::{self, HistoryEntry, Settings},
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use futures_util::StreamExt;
use image::ImageReader;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Semaphore, mpsc};

const BG: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x26);
const SURFACE: Color32 = Color32::from_rgb(0x24, 0x28, 0x3b);
const BORDER: Color32 = Color32::from_rgb(0x41, 0x48, 0x68);
const TEXT: Color32 = Color32::from_rgb(0xc0, 0xca, 0xf5);
const MUTED: Color32 = Color32::from_rgb(0x56, 0x5f, 0x89);
const BLUE: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xf7);
const PURPLE: Color32 = Color32::from_rgb(0xbb, 0x9a, 0xf7);
const GREEN: Color32 = Color32::from_rgb(0x9e, 0xce, 0x6a);
const RED: Color32 = Color32::from_rgb(0xf7, 0x76, 0x8e);
const MAX_THUMBNAIL_TEXTURES: usize = 96;

enum Message {
    Search(
        u64,
        Source,
        u32,
        Result<(Vec<Wallpaper>, u32, Option<String>), String>,
    ),
    Thumbnail(String, Result<egui::ColorImage, String>),
    Preview(String, Result<egui::ColorImage, String>),
    DownloadProgress(String, u64, Option<u64>),
    DownloadComplete(String, Result<PathBuf, String>),
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Discover,
    Favorites,
    Downloads,
    Settings,
}
#[derive(Clone, Copy)]
enum Icon {
    Search,
    Favorite,
    Download,
    Settings,
    Filter,
    Grid,
    List,
    Refresh,
    Image,
}

fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.8, color);
    let center = rect.center();
    let line = |a, b| painter.line_segment([a, b], stroke);
    match icon {
        Icon::Search => {
            painter.circle_stroke(center - Vec2::new(2.0, 2.0), 5.5, stroke);
            line(center + Vec2::new(2.0, 2.0), center + Vec2::new(7.0, 7.0));
        }
        Icon::Favorite => {
            let points = vec![
                egui::pos2(center.x, center.y + 7.0),
                egui::pos2(center.x - 8.0, center.y),
                egui::pos2(center.x - 7.0, center.y - 6.0),
                egui::pos2(center.x - 3.0, center.y - 7.0),
                egui::pos2(center.x, center.y - 3.0),
                egui::pos2(center.x + 3.0, center.y - 7.0),
                egui::pos2(center.x + 7.0, center.y - 6.0),
                egui::pos2(center.x + 8.0, center.y),
                egui::pos2(center.x, center.y + 7.0),
            ];
            painter.add(egui::Shape::line(points, stroke));
        }
        Icon::Download => {
            line(center - Vec2::new(0.0, 8.0), center + Vec2::new(0.0, 3.0));
            line(center + Vec2::new(-4.0, 0.0), center + Vec2::new(0.0, 4.0));
            line(center + Vec2::new(4.0, 0.0), center + Vec2::new(0.0, 4.0));
            line(center + Vec2::new(-7.0, 5.0), center + Vec2::new(-7.0, 8.0));
            line(center + Vec2::new(-7.0, 8.0), center + Vec2::new(7.0, 8.0));
            line(center + Vec2::new(7.0, 8.0), center + Vec2::new(7.0, 5.0));
        }
        Icon::Settings => {
            painter.circle_stroke(center, 6.0, stroke);
            painter.circle_filled(center, 2.0, color);
            for angle in (0..8).map(|i| i as f32 * std::f32::consts::TAU / 8.0) {
                let v = Vec2::angled(angle);
                line(center + v * 7.0, center + v * 9.0);
            }
        }
        Icon::Filter => {
            line(
                rect.left_top() + Vec2::new(2.0, 3.0),
                rect.right_top() - Vec2::new(2.0, -3.0),
            );
            line(
                rect.left_top() + Vec2::new(2.0, 3.0),
                center + Vec2::new(0.0, 1.0),
            );
            line(
                rect.right_top() - Vec2::new(2.0, -3.0),
                center + Vec2::new(0.0, 1.0),
            );
            line(center + Vec2::new(0.0, 1.0), center + Vec2::new(0.0, 8.0));
        }
        Icon::Grid => {
            for x in [-5.0, 2.0] {
                for y in [-5.0, 2.0] {
                    painter.rect_stroke(
                        egui::Rect::from_min_size(center + Vec2::new(x, y), Vec2::splat(4.0)),
                        egui::CornerRadius::same(1),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        Icon::List => {
            for y in [-5.0, 0.0, 5.0] {
                line(center + Vec2::new(-7.0, y), center + Vec2::new(7.0, y));
            }
        }
        Icon::Refresh => {
            painter.circle_stroke(center, 6.5, stroke);
            line(center + Vec2::new(1.0, -8.0), center + Vec2::new(7.0, -8.0));
            line(center + Vec2::new(7.0, -8.0), center + Vec2::new(7.0, -2.0));
            line(center + Vec2::new(-1.0, 8.0), center + Vec2::new(-7.0, 8.0));
            line(center + Vec2::new(-7.0, 8.0), center + Vec2::new(-7.0, 2.0));
        }
        Icon::Image => {
            let image_rect = rect.shrink(2.0);
            painter.rect_stroke(
                image_rect,
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(
                egui::pos2(image_rect.right() - 4.0, image_rect.top() + 4.0),
                1.4,
                color,
            );
            line(
                egui::pos2(image_rect.left() + 2.0, image_rect.bottom() - 3.0),
                egui::pos2(center.x - 1.0, center.y),
            );
            line(
                egui::pos2(center.x - 1.0, center.y),
                egui::pos2(image_rect.right() - 2.0, image_rect.bottom() - 3.0),
            );
        }
    }
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str, size: Vec2) -> egui::Response {
    let response = ui.add_sized(size, egui::Button::new(""));
    let color = if response.hovered() { BLUE } else { TEXT };
    draw_icon(ui.painter(), response.rect.shrink(5.0), icon, color);
    response.on_hover_text(tooltip)
}

fn nav_button(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(8),
            if selected {
                Color32::from_rgb(0x30, 0x3a, 0x5b)
            } else {
                SURFACE
            },
        );
    }
    let color = if selected { BLUE } else { TEXT };
    draw_icon(
        ui.painter(),
        egui::Rect::from_center_size(rect.left_center() + Vec2::new(24.0, 0.0), Vec2::splat(20.0)),
        icon,
        color,
    );
    ui.painter().text(
        rect.left_center() + Vec2::new(46.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        color,
    );
    response
}
struct Download {
    item: Wallpaper,
    received: u64,
    total: Option<u64>,
    cancel: Arc<AtomicBool>,
    status: DownloadState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum DownloadState {
    Starting,
    Downloading,
    Complete,
    Failed(String),
    Cancelled,
}
impl DownloadState {
    fn label(&self) -> &str {
        match self {
            Self::Starting => "Starting",
            Self::Downloading => "Downloading",
            Self::Complete => "Complete",
            Self::Failed(message) => message,
            Self::Cancelled => "Cancelled",
        }
    }
}

pub struct WallBrowseApp {
    context: egui::Context,
    tx: mpsc::UnboundedSender<Message>,
    rx: mpsc::UnboundedReceiver<Message>,
    runtime: Option<tokio::runtime::Runtime>,
    page: Page,
    settings: Settings,
    filters: SearchFilters,
    results: Vec<Wallpaper>,
    source_choice: SourceChoice,
    source_pages: HashMap<Source, (u32, u32, Option<String>)>,
    source_page_cursors: HashMap<Source, HashMap<u32, Option<String>>>,
    pending_sources: HashSet<Source>,
    source_errors: HashMap<Source, String>,
    favorites: Vec<Wallpaper>,
    history: Vec<HistoryEntry>,
    selected: Option<Wallpaper>,
    textures: HashMap<String, egui::TextureHandle>,
    thumbnail_last_used: HashMap<String, Instant>,
    visible_thumbnails: HashSet<String>,
    thumbnail_errors: HashMap<String, String>,
    thumbnail_retry_after: HashMap<String, Instant>,
    previews: HashMap<String, egui::TextureHandle>,
    preview_errors: HashMap<String, String>,
    requested_thumbnails: HashSet<String>,
    image_requests: Arc<Semaphore>,
    loading: bool,
    has_searched: bool,
    error: Option<String>,
    search_generation: u64,
    show_filters: bool,
    list_view: bool,
    content_scroll_offset: f32,
    last_query_edit: Instant,
    last_searched_query: String,
    downloads: HashMap<String, Download>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceChoice {
    All,
    Wallhaven,
    Reddit,
}

fn sort_options(source: SourceChoice) -> &'static [(SortOrder, &'static str)] {
    match source {
        SourceChoice::Wallhaven => &[
            (SortOrder::Relevance, "Relevance"),
            (SortOrder::DateAdded, "Latest"),
            (SortOrder::Views, "Most viewed"),
            (SortOrder::Favorites, "Most favorited"),
            (SortOrder::Random, "Random"),
        ],
        SourceChoice::All => &[
            (SortOrder::Relevance, "Relevance"),
            (SortOrder::DateAdded, "Latest"),
        ],
        SourceChoice::Reddit => &[
            (SortOrder::Relevance, "Relevance"),
            (SortOrder::DateAdded, "New"),
            (SortOrder::Views, "Hot"),
            (SortOrder::Favorites, "Top"),
        ],
    }
}

fn sort_label(source: SourceChoice, sort: SortOrder) -> &'static str {
    sort_options(source)
        .iter()
        .find(|(value, _)| *value == sort)
        .map(|(_, label)| *label)
        .unwrap_or("Relevance")
}

impl WallBrowseApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals.dark_mode = true;
        style.visuals.window_fill = BG;
        style.visuals.panel_fill = BG;
        style.visuals.extreme_bg_color = BG;
        style.visuals.widgets.inactive.bg_fill = SURFACE;
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2d, 0x34, 0x55);
        style.visuals.widgets.active.bg_fill = BLUE;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        style.visuals.selection.bg_fill = BLUE;
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let (tx, rx) = mpsc::unbounded_channel();
        let settings = storage::load_settings();
        let filters = SearchFilters {
            sfw_only: settings.sfw_only,
            ..SearchFilters::default()
        };
        let mut app = Self {
            context: cc.egui_ctx.clone(),
            tx,
            rx,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .ok(),
            page: Page::Discover,
            settings,
            filters,
            results: Vec::new(),
            source_choice: SourceChoice::Wallhaven,
            source_pages: HashMap::new(),
            source_page_cursors: HashMap::new(),
            pending_sources: HashSet::new(),
            source_errors: HashMap::new(),
            favorites: storage::load_favorites(),
            history: storage::load_history(),
            selected: None,
            textures: HashMap::new(),
            thumbnail_last_used: HashMap::new(),
            visible_thumbnails: HashSet::new(),
            thumbnail_errors: HashMap::new(),
            thumbnail_retry_after: HashMap::new(),
            previews: HashMap::new(),
            preview_errors: HashMap::new(),
            requested_thumbnails: HashSet::new(),
            image_requests: Arc::new(Semaphore::new(4)),
            loading: false,
            has_searched: false,
            error: None,
            search_generation: 0,
            show_filters: false,
            list_view: false,
            content_scroll_offset: 0.0,
            last_query_edit: Instant::now(),
            last_searched_query: String::new(),
            downloads: HashMap::new(),
        };
        app.search();
        app
    }
    fn search(&mut self) {
        self.filters.page = 1;
        self.last_searched_query = self.filters.query.clone();
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        self.loading = true;
        self.error = None;
        self.results.clear();
        self.source_pages.clear();
        self.source_page_cursors.clear();
        self.source_errors.clear();
        self.pending_sources.clear();
        let filters = self.filters.clone();
        let sources: Vec<Source> = match self.source_choice {
            SourceChoice::Wallhaven => vec![Source::Wallhaven],
            SourceChoice::Reddit => vec![Source::Reddit],
            SourceChoice::All => vec![Source::Wallhaven, Source::Reddit],
        };
        self.pending_sources.extend(sources.iter().copied());
        for source in &sources {
            self.source_page_cursors
                .entry(*source)
                .or_default()
                .insert(1, None);
        }
        let reddit_credentials = provider::RedditCredentials {
            client_id: self.settings.reddit_client_id.clone(),
            client_secret: self.settings.reddit_client_secret.clone(),
            username: self.settings.reddit_username.clone(),
        };
        let keys = [
            (Source::Wallhaven, self.settings.api_key.clone()),
            (
                Source::Reddit,
                serde_json::to_string(&reddit_credentials).unwrap_or_default(),
            ),
        ];
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        if let Some(rt) = &self.runtime {
            for source in sources {
                let filters = filters.clone();
                let tx = tx.clone();
                let ctx = ctx.clone();
                let key = keys
                    .iter()
                    .find(|(s, _)| *s == source)
                    .map(|(_, k)| k.clone())
                    .unwrap_or_default();
                rt.spawn(async move {
                    let res = provider::search_source(source, &filters, &key, None)
                        .await
                        .map(|p| (p.wallpapers, p.last_page, p.next_cursor))
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Message::Search(generation, source, 1, res));
                    ctx.request_repaint();
                });
            }
        } else {
            self.loading = false;
            self.error = Some("Could not initialize the async runtime".into());
        }
    }
    fn request_thumbnail(&mut self, item: &Wallpaper) {
        let id = item.id.clone();
        self.visible_thumbnails.insert(id.clone());
        if self.textures.contains_key(&id) {
            self.thumbnail_last_used.insert(id, Instant::now());
            return;
        }
        if self.requested_thumbnails.contains(&id) {
            return;
        }
        let now = Instant::now();
        if self
            .thumbnail_retry_after
            .get(&id)
            .is_some_and(|retry_at| *retry_at > now)
        {
            return;
        }
        self.thumbnail_retry_after.remove(&id);
        self.thumbnail_errors.remove(&id);

        // Visible thumbnails are the only ones queued. Keep the wait queue bounded
        // as well as the decoded texture cache when users scroll rapidly.
        if self.requested_thumbnails.len() >= 128 {
            return;
        }
        if !make_thumbnail_room(
            &mut self.textures,
            &mut self.thumbnail_last_used,
            &mut self.requested_thumbnails,
            &self.visible_thumbnails,
            self.selected.as_ref().map(|selected| selected.id.as_str()),
        ) {
            return;
        }

        self.requested_thumbnails.insert(id.clone());
        let url = item.thumbnail_url.clone();
        let tx = self.tx.clone();
        let semaphore = self.image_requests.clone();
        let ctx = self.context.clone();
        let cache_dir = self.settings.cache_dir.clone();
        let cache_limit = self.settings.cache_limit_mb;
        if let Some(rt) = &self.runtime {
            rt.spawn(async move {
                let result = async {
                    let _permit = semaphore.acquire_owned().await.map_err(|e| e.to_string())?;
                    let cache_path = cache_dir.join(format!("{}.thumb", sanitize_filename(&id)));
                    let read_path = cache_path.clone();
                    let cached = tokio::task::spawn_blocking(move || fs::read(read_path).ok())
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(bytes) = cached.filter(|bytes| bytes.len() <= 12 * 1024 * 1024) {
                        match decode_thumbnail(bytes).await {
                            Ok(image) => return Ok(image),
                            Err(error) => {
                                tracing::debug!(wallpaper_id = %id, %error, "Discarding invalid thumbnail cache entry");
                                let remove_path = cache_path.clone();
                                let _ = tokio::task::spawn_blocking(move || {
                                    let _ = fs::remove_file(remove_path);
                                })
                                .await;
                            }
                        }
                    }

                    let response = get_http(url).await?;
                    if !response.status().is_success() {
                        return Err(format!("HTTP {}", response.status()));
                    }
                    if response
                        .content_length()
                        .is_some_and(|length| length > 12 * 1024 * 1024)
                    {
                        return Err("Thumbnail exceeds size limit".into());
                    }
                    let mut stream = response.bytes_stream();
                    let mut bytes = Vec::new();
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.map_err(|error| error.to_string())?;
                        if bytes.len().saturating_add(chunk.len()) > 12 * 1024 * 1024 {
                            return Err("Thumbnail exceeds size limit".into());
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    let cache_bytes = bytes.clone();
                    let image = decode_thumbnail(bytes).await?;
                    let write_path = cache_path;
                    let limit = u64::from(cache_limit).saturating_mul(1024 * 1024);
                    let _ = tokio::task::spawn_blocking(move || {
                        write_thumbnail_cache(&write_path, &cache_bytes, limit)
                    })
                    .await;
                    Ok::<_, String>(image)
                }
                .await;
                let _ = tx.send(Message::Thumbnail(id, result));
                ctx.request_repaint();
            });
        } else {
            self.requested_thumbnails.remove(&id);
        }
    }
    fn select_wallpaper(&mut self, item: Wallpaper) {
        self.selected = Some(item.clone());
        if self.previews.contains_key(&item.id) {
            return;
        }
        let id = item.id;
        let url = item.preview_url;
        let semaphore = self.image_requests.clone();
        let ctx = self.context.clone();
        let tx = self.tx.clone();
        if let Some(rt) = &self.runtime {
            rt.spawn(async move {
                let result = async {
                    let _permit = semaphore.acquire_owned().await.map_err(|e| e.to_string())?;
                    let response = get_http(url).await?;
                    if !response.status().is_success() {
                        return Err(format!("HTTP {}", response.status()));
                    }
                    let mut stream = response.bytes_stream();
                    let mut bytes = Vec::new();
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.map_err(|e| e.to_string())?;
                        if bytes.len().saturating_add(chunk.len()) > 32 * 1024 * 1024 {
                            return Err("Preview image exceeds size limit".into());
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    tokio::task::spawn_blocking(move || {
                        let image = ImageReader::new(std::io::Cursor::new(bytes))
                            .with_guessed_format()
                            .map_err(|e| e.to_string())?
                            .decode()
                            .map_err(|e| e.to_string())?
                            .thumbnail(1600, 1000)
                            .to_rgba8();
                        let size = [image.width() as usize, image.height() as usize];
                        Ok::<_, String>(egui::ColorImage::from_rgba_unmultiplied(
                            size,
                            image.as_raw(),
                        ))
                    })
                    .await
                    .map_err(|e| e.to_string())?
                }
                .await;
                let _ = tx.send(Message::Preview(id, result));
                ctx.request_repaint();
            });
        }
    }
    fn toggle_favorite(&mut self, item: &Wallpaper) {
        if let Some(i) = self.favorites.iter().position(|x| x.id == item.id) {
            self.favorites.remove(i);
        } else {
            self.favorites.push(item.clone());
        }
        if let Err(e) = storage::save_favorites(&self.favorites) {
            self.error = Some(e.to_string());
        }
    }
    fn start_download(&mut self, item: Wallpaper) {
        if item.provider == "Reddit" && !self.settings.reddit_download_rights_confirmed {
            self.error = Some("Confirm in Settings that you have permission to download the selected Reddit image.".into());
            return;
        }
        if self.downloads.contains_key(&item.id) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.downloads.insert(
            item.id.clone(),
            Download {
                item: item.clone(),
                received: 0,
                total: None,
                cancel: cancel.clone(),
                status: DownloadState::Starting,
            },
        );
        let key = item.id.clone();
        let url = item.download_url.clone();
        let folder = self.settings.download_dir.clone();
        let tx = self.tx.clone();
        let semaphore = self.image_requests.clone();
        let ctx = self.context.clone();
        if let Some(rt) = &self.runtime {
            rt.spawn(async move {
                let result = download(
                    url,
                    folder,
                    cancel,
                    tx.clone(),
                    key.clone(),
                    semaphore,
                    ctx.clone(),
                )
                .await;
                let _ = tx.send(Message::DownloadComplete(key, result));
                ctx.request_repaint();
            });
        }
    }
    fn thumb(&mut self, ui: &mut egui::Ui, item: &Wallpaper, size: Vec2) {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            self.request_thumbnail(item);
        }
        if let Some(tex) = self.textures.get(&item.id) {
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(8), SURFACE);
            draw_icon(ui.painter(), rect.shrink(8.0), Icon::Image, MUTED);
            if self.thumbnail_errors.contains_key(&item.id) {
                ui.painter()
                    .circle_filled(rect.right_top() + Vec2::new(-10.0, 10.0), 4.0, RED);
            }
        }
    }
    fn card(&mut self, ui: &mut egui::Ui, item: &Wallpaper, w: f32) {
        ui.allocate_ui_with_layout(
            Vec2::new(w, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let h = w * 0.63;
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());
                if ui.is_rect_visible(rect) {
                    self.request_thumbnail(item);
                }
                if let Some(tex) = self.textures.get(&item.id) {
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter()
                        .rect_filled(rect, egui::CornerRadius::same(8), SURFACE);
                    draw_icon(ui.painter(), rect.shrink(12.0), Icon::Image, MUTED);
                    if self.thumbnail_errors.contains_key(&item.id) {
                        ui.painter().circle_filled(
                            rect.right_top() + Vec2::new(-11.0, 11.0),
                            4.0,
                            RED,
                        );
                    }
                }
                if response.clicked() {
                    self.select_wallpaper(item.clone());
                }
                if response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::same(8),
                        Stroke::new(1.0, BLUE),
                        egui::StrokeKind::Inside,
                    );
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&item.provider).size(10.).strong().color(BLUE));
                    ui.label(RichText::new(item.resolution()).size(12.).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let is_favorite = self.favorites.iter().any(|f| f.id == item.id);
                        if icon_button(
                            ui,
                            Icon::Favorite,
                            if is_favorite {
                                "Remove favorite"
                            } else {
                                "Add favorite"
                            },
                            Vec2::splat(28.0),
                        )
                        .clicked()
                        {
                            self.toggle_favorite(item);
                        }
                        if icon_button(ui, Icon::Download, "Download original", Vec2::splat(28.0))
                            .clicked()
                        {
                            self.start_download(item.clone());
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        item.tags
                            .iter()
                            .take(3)
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(" · "),
                    )
                    .size(11.)
                    .color(PURPLE),
                );
            },
        );
    }
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("nav")
            .exact_size(204.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("WB").size(16.0).strong().color(BLUE));
                    ui.label(RichText::new("WallBrowse").size(20.0).strong().color(TEXT));
                });
                ui.add_space(26.);
                for (page, label, icon) in [
                    (Page::Discover, "Discover", Icon::Search),
                    (Page::Favorites, "Favorites", Icon::Favorite),
                    (Page::Downloads, "Downloads", Icon::Download),
                    (Page::Settings, "Settings", Icon::Settings),
                ] {
                    if nav_button(ui, icon, label, self.page == page).clicked() {
                        self.page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(RichText::new("Wallhaven · Reddit").size(10.).color(MUTED));
                });
            });
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(match self.page {
                    Page::Discover => "Discover",
                    Page::Favorites => "Favorites",
                    Page::Downloads => "Downloads",
                    Page::Settings => "Settings",
                })
                .size(22.)
                .strong()
                .color(TEXT),
            );
            if self.page == Page::Discover {
                ui.add_space(18.);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.filters.query)
                        .hint_text("Search wallpapers and tags…")
                        .desired_width(260.),
                );
                if edit.changed() {
                    self.last_query_edit = Instant::now();
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.search();
                }
                if ui.button("Search").clicked() {
                    self.search();
                }
                ui.add_space(8.);
                egui::ComboBox::from_id_salt("source")
                    .selected_text(match self.source_choice {
                        SourceChoice::All => "All sources",
                        SourceChoice::Wallhaven => "Wallhaven",
                        SourceChoice::Reddit => "Reddit",
                    })
                    .show_ui(ui, |ui| {
                        for (choice, label) in [
                            (SourceChoice::All, "All sources"),
                            (SourceChoice::Wallhaven, "Wallhaven"),
                            (SourceChoice::Reddit, "Reddit"),
                        ] {
                            if ui
                                .selectable_value(&mut self.source_choice, choice, label)
                                .clicked()
                            {
                                if choice == SourceChoice::Reddit
                                    && self.filters.sort == SortOrder::Random
                                {
                                    self.filters.sort = SortOrder::Relevance;
                                }
                                if choice == SourceChoice::All
                                    && !matches!(
                                        self.filters.sort,
                                        SortOrder::Relevance | SortOrder::DateAdded
                                    )
                                {
                                    self.filters.sort = SortOrder::Relevance;
                                }
                                self.search();
                            }
                        }
                        ui.separator();
                        ui.label(
                            RichText::new(
                                "Pexels / Pixabay remain unavailable under their API terms",
                            )
                            .small()
                            .color(MUTED),
                        );
                    });
                egui::ComboBox::from_id_salt("sort")
                    .selected_text(sort_label(self.source_choice, self.filters.sort))
                    .show_ui(ui, |ui| {
                        for (v, l) in sort_options(self.source_choice) {
                            if ui
                                .selectable_value(&mut self.filters.sort, *v, *l)
                                .clicked()
                            {
                                self.search();
                            }
                        }
                    });
                let filters_clicked = ui
                    .horizontal(|ui| {
                        let clicked =
                            icon_button(ui, Icon::Filter, "Open filters", Vec2::splat(30.0))
                                .clicked();
                        ui.label("Filters");
                        clicked
                    })
                    .inner;
                if filters_clicked {
                    self.show_filters = !self.show_filters;
                }
                if icon_button(
                    ui,
                    if self.list_view {
                        Icon::Grid
                    } else {
                        Icon::List
                    },
                    "Toggle grid/list",
                    Vec2::splat(30.0),
                )
                .clicked()
                {
                    self.list_view = !self.list_view;
                }
                if icon_button(ui, Icon::Refresh, "Refresh results", Vec2::splat(30.0)).clicked() {
                    self.search();
                }
            }
        });
    }
    fn filter_panel(&mut self, ctx: &egui::Context) {
        if self.show_filters {
            egui::Window::new("Search filters")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    if self.source_choice != SourceChoice::Wallhaven {
                        ui.label(RichText::new("Reddit applies keyword, safe-content, and minimum-size filters; its sort options map to Reddit sorts. Aspect ratio, color, and Wallhaven categories apply only to Wallhaven.").color(MUTED));
                    }
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.filters.sfw_only, "SFW only");
                    });
                    ui.label("Results per page");
                    egui::ComboBox::from_id_salt("per-page")
                        .selected_text(self.filters.per_page.to_string())
                        .show_ui(ui, |ui| {
                            let counts: &[u8] = &[12, 24, 36, 48, 64];
                            for count in counts.iter().copied() {
                                ui.selectable_value(
                                    &mut self.filters.per_page,
                                    count,
                                    count.to_string(),
                                );
                            }
                        });
                    ui.label("Minimum resolution");
                    ui.horizontal(|ui| {
                        let mut w = self.filters.min_width.unwrap_or(1920);
                        let mut h = self.filters.min_height.unwrap_or(1080);
                        ui.add(egui::DragValue::new(&mut w).range(0..=16384).prefix("W "));
                        ui.add(egui::DragValue::new(&mut h).range(0..=16384).prefix("H "));
                        self.filters.min_width = if w == 0 { None } else { Some(w) };
                        self.filters.min_height = if h == 0 { None } else { Some(h) };
                    });
                    ui.label("Aspect ratio");
                    egui::ComboBox::from_id_salt("ratio")
                        .selected_text(self.filters.ratio.as_deref().unwrap_or("Any"))
                        .show_ui(ui, |ui| {
                            for (val, label) in [
                                (None, "Any"),
                                (Some("16x9"), "16:9"),
                                (Some("16x10"), "16:10"),
                                (Some("21x9"), "21:9"),
                                (Some("9x16"), "9:16"),
                                (Some("1x1"), "Square"),
                            ] {
                                if ui
                                    .selectable_label(self.filters.ratio.as_deref() == val, label)
                                    .clicked()
                                {
                                    self.filters.ratio = val.map(str::to_owned);
                                }
                            }
                        });
                    ui.label("Dominant color");
                    egui::ComboBox::from_id_salt("color")
                        .selected_text(self.filters.color.as_deref().unwrap_or("Any"))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (None, "Any"),
                                (Some("660000"), "Red"),
                                (Some("006600"), "Green"),
                                (Some("000066"), "Blue"),
                                (Some("666600"), "Yellow"),
                                (Some("660066"), "Purple"),
                                (Some("666666"), "Gray"),
                            ] {
                                if ui
                                    .selectable_label(self.filters.color.as_deref() == value, label)
                                    .clicked()
                                {
                                    self.filters.color = value.map(str::to_owned);
                                }
                            }
                        });
                    ui.label("Categories");
                    ui.horizontal(|ui| {
                        for (i, l) in ["General", "Anime", "People"].iter().enumerate() {
                            ui.checkbox(&mut self.filters.categories[i], *l);
                        }
                    });
                    if ui.button("Apply filters").clicked() {
                        self.search();
                        self.show_filters = false;
                    }
                });
        }
    }
}

fn write_thumbnail_cache(path: &std::path::Path, bytes: &[u8], limit: u64) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)?;
    if limit == 0 {
        return Ok(());
    }
    let mut entries = fs::read_dir(parent)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            Some((entry.path(), metadata.len(), metadata.modified().ok()?))
        })
        .collect::<Vec<_>>();
    let mut used = entries.iter().map(|(_, size, _)| *size).sum::<u64>();
    entries.sort_by_key(|(_, _, modified)| *modified);
    for (old_path, size, _) in entries {
        if used.saturating_add(bytes.len() as u64) <= limit {
            break;
        }
        if old_path != path && fs::remove_file(old_path).is_ok() {
            used = used.saturating_sub(size);
        }
    }
    if used.saturating_add(bytes.len() as u64) > limit {
        return Ok(());
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)?;
    fs::rename(temp, path)
}

async fn download(
    url: String,
    folder: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<Message>,
    id: String,
    semaphore: Arc<Semaphore>,
    ctx: egui::Context,
) -> Result<PathBuf, String> {
    let _permit = semaphore.acquire_owned().await.map_err(|e| e.to_string())?;
    let response = get_http(url)
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Server returned {}", response.status()));
    }
    let total = response.content_length();
    if total.is_some_and(|n| n > 100 * 1024 * 1024) {
        return Err("Image exceeds 100 MiB download limit".into());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let chunk = chunk.map_err(|e| format!("Download failed: {e}"))?;
        if bytes.len().saturating_add(chunk.len()) > 100 * 1024 * 1024 {
            return Err("Image exceeds 100 MiB download limit".into());
        }
        bytes.extend_from_slice(&chunk);
        let _ = tx.send(Message::DownloadProgress(
            id.clone(),
            bytes.len() as u64,
            total,
        ));
        ctx.request_repaint();
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| "Downloaded file is not a supported image".to_string())?;
    if !matches!(
        format,
        image::ImageFormat::Jpeg
            | image::ImageFormat::Png
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
    ) {
        return Err("Downloaded file is not a supported image".into());
    }
    let save_cancel = cancel.clone();
    tokio::task::spawn_blocking(move || {
        let dimensions = ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .into_dimensions()
            .map_err(|e| format!("Invalid image dimensions: {e}"))?;
        if dimensions.0 == 0
            || dimensions.1 == 0
            || u64::from(dimensions.0) * u64::from(dimensions.1) > 50_000_000
        {
            return Err("Image dimensions exceed the supported limit".to_string());
        }
        image::load_from_memory_with_format(&bytes, format)
            .map_err(|e| format!("Downloaded image is corrupt: {e}"))?;
        if save_cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let extension = match format {
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::Png => "png",
            image::ImageFormat::WebP => "webp",
            _ => "gif",
        };
        let stem = sanitize_filename(&id.replace(':', "_"));
        let mut path = folder.join(format!("{stem}.{extension}"));
        let mut n = 2;
        while path.exists() {
            path = folder.join(format!("{stem}-{n}.{extension}"));
            n += 1;
        }
        let temp = path.with_extension(format!("{extension}.part"));
        let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
        let write_result = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
        if save_cancel.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&temp);
            return Err("Cancelled".to_string());
        }
        if let Err(error) = fs::hard_link(&temp, &path) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
        let _ = fs::remove_file(&temp);
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn get_http(url: String) -> Result<reqwest::Response, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("WallBrowse/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn decode_thumbnail(bytes: Vec<u8>) -> Result<egui::ColorImage, String> {
    if bytes.is_empty() || bytes.len() > 12 * 1024 * 1024 {
        return Err("Thumbnail is empty or exceeds the size limit".into());
    }
    tokio::task::spawn_blocking(move || {
        let image = ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| error.to_string())?
            .decode()
            .map_err(|error| error.to_string())?
            .thumbnail(480, 320)
            .to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        Ok::<_, String>(egui::ColorImage::from_rgba_unmultiplied(
            size,
            image.as_raw(),
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn sanitize_filename(name: &str) -> String {
    let s = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect::<String>();
    if s.is_empty() { "wallpaper".into() } else { s }
}

impl eframe::App for WallBrowseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.visible_thumbnails.clear();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Message::Search(generation, source, page, res) => {
                    if generation != self.search_generation {
                        continue;
                    }
                    self.pending_sources.remove(&source);
                    self.loading = !self.pending_sources.is_empty();
                    self.has_searched = true;
                    match res {
                        Ok((items, last, next_cursor)) => {
                            self.results.retain(|item| item.provider != source.name());
                            let cursors = self.source_page_cursors.entry(source).or_default();
                            if let Some(cursor) = &next_cursor {
                                cursors.insert(page.saturating_add(1), Some(cursor.clone()));
                            }
                            let last = if source == Source::Reddit {
                                cursors.keys().copied().max().unwrap_or(page).max(page)
                            } else {
                                last
                            };
                            let mut known: HashSet<String> =
                                self.results.iter().map(|item| item.id.clone()).collect();
                            self.results.extend(
                                items
                                    .into_iter()
                                    .filter(|item| known.insert(item.id.clone())),
                            );
                            self.source_pages.insert(source, (page, last, next_cursor));
                            self.source_errors.remove(&source);
                        }
                        Err(e) => {
                            self.source_errors.insert(source, e);
                        }
                    }
                    self.error = None;
                }
                Message::Thumbnail(id, res) => match res {
                    Ok(image) => {
                        self.thumbnail_errors.remove(&id);
                        self.thumbnail_retry_after.remove(&id);
                        self.thumbnail_last_used.insert(id.clone(), Instant::now());
                        self.textures.insert(
                            id,
                            ctx.load_texture("wallpaper", image, egui::TextureOptions::LINEAR),
                        );
                    }
                    Err(error) => {
                        self.requested_thumbnails.remove(&id);
                        self.thumbnail_errors.insert(id.clone(), error.clone());
                        self.thumbnail_retry_after
                            .insert(id.clone(), Instant::now() + Duration::from_secs(30));
                        tracing::warn!(wallpaper_id = %id, %error, "Could not load wallpaper thumbnail");
                    }
                },
                Message::Preview(id, res) => match res {
                    Ok(image) => {
                        self.preview_errors.remove(&id);
                        if self.previews.len() >= 32
                            && let Some(old) = self.previews.keys().next().cloned()
                        {
                            self.previews.remove(&old);
                        }
                        self.previews.insert(
                            id,
                            ctx.load_texture(
                                "wallpaper-preview",
                                image,
                                egui::TextureOptions::LINEAR,
                            ),
                        );
                    }
                    Err(error) => {
                        self.preview_errors.insert(id, error);
                    }
                },
                Message::DownloadProgress(id, n, total) => {
                    if let Some(d) = self.downloads.get_mut(&id) {
                        d.received = n;
                        d.total = total;
                        d.status = DownloadState::Downloading;
                    }
                }
                Message::DownloadComplete(id, res) => {
                    if let Some(d) = self.downloads.get_mut(&id) {
                        match res {
                            Ok(path) => {
                                d.status = DownloadState::Complete;
                                self.history.insert(
                                    0,
                                    HistoryEntry {
                                        wallpaper: d.item.clone(),
                                        path,
                                        downloaded_at: SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs()
                                            .to_string(),
                                    },
                                );
                                let _ = storage::save_history(&self.history);
                            }
                            Err(e) => {
                                d.status = if e == "Cancelled" {
                                    DownloadState::Cancelled
                                } else {
                                    DownloadState::Failed(e.clone())
                                };
                                if e != "Cancelled" {
                                    self.error = Some(e);
                                }
                            }
                        }
                    }
                }
            }
        }
        if self.page == Page::Discover
            && self.filters.query != self.last_searched_query
            && self.last_query_edit.elapsed() > Duration::from_millis(700)
        {
            self.search();
        }
        self.sidebar(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin {
                left: 22,
                right: 22,
                top: 18,
                bottom: 12,
            }))
            .show(ui, |ui| {
                self.toolbar(ui);
                ui.add_space(16.);
                match self.page {
                    Page::Discover => self.discovery(ui, &ctx),
                    Page::Favorites => self.items_view(ui, &ctx, self.favorites.clone()),
                    Page::Downloads => self.downloads_view(ui),
                    Page::Settings => self.settings_view(ui),
                }
            });
        self.filter_panel(&ctx);
        self.detail(&ctx);
        if self.loading {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl WallBrowseApp {
    fn discovery(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let source_errors: Vec<_> = self
            .source_errors
            .iter()
            .map(|(s, e)| (*s, e.clone()))
            .collect();
        for (source, message) in source_errors {
            ui.horizontal(|ui| {
                ui.colored_label(RED, format!("{}: {message}", source.name()));
                if source == Source::Reddit && ui.button("Settings").clicked() {
                    self.page = Page::Settings;
                }
                if ui.button("Retry").clicked() {
                    self.search();
                }
            });
        }
        if let Some(e) = &self.error {
            ui.colored_label(RED, e);
        }
        if self.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "Searching {}…",
                    self.pending_sources
                        .iter()
                        .map(|s| s.name())
                        .collect::<Vec<_>>()
                        .join(" and ")
                ));
            });
        }
        if self.source_choice == SourceChoice::All
            && !self.source_errors.is_empty()
            && !self.results.is_empty()
        {
            ui.colored_label(
                MUTED,
                "Partial results: one or more sources could not be reached.",
            );
        }
        if self.results.is_empty()
            && !self.loading
            && self.error.is_none()
            && self.source_errors.is_empty()
        {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 96.0),
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.label(
                        RichText::new(if self.has_searched {
                            "No wallpapers matched these filters."
                        } else {
                            "Search for a wallpaper to get started"
                        })
                        .size(17.)
                        .color(MUTED),
                    );
                },
            );
            self.pagination_footer(ui);
        } else {
            self.items_view(ui, ctx, self.results.clone());
        }
    }
    fn search_page(&mut self, source: Source, page: u32) {
        self.loading = true;
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        let filters = self.filters.clone();
        let mut filters = filters;
        filters.page = page;
        self.pending_sources.clear();
        self.pending_sources.insert(source);
        let key = match source {
            Source::Wallhaven => self.settings.api_key.clone(),
            Source::Reddit => serde_json::to_string(&provider::RedditCredentials {
                client_id: self.settings.reddit_client_id.clone(),
                client_secret: self.settings.reddit_client_secret.clone(),
                username: self.settings.reddit_username.clone(),
            })
            .unwrap_or_default(),
        };
        let cursor = if source == Source::Reddit {
            self.source_page_cursors
                .get(&source)
                .and_then(|pages| pages.get(&page))
                .cloned()
                .flatten()
        } else {
            None
        };
        if source == Source::Reddit
            && page > 1
            && !self
                .source_page_cursors
                .get(&source)
                .is_some_and(|pages| pages.contains_key(&page))
        {
            self.loading = false;
            self.pending_sources.clear();
            return;
        }
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        if let Some(rt) = &self.runtime {
            rt.spawn(async move {
                let r = provider::search_source(source, &filters, &key, cursor.as_deref())
                    .await
                    .map(|p| (p.wallpapers, p.last_page, p.next_cursor))
                    .map_err(|e| e.to_string());
                let _ = tx.send(Message::Search(generation, source, page, r));
                ctx.request_repaint();
            });
        }
    }
    fn items_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, items: Vec<Wallpaper>) {
        if items.is_empty() {
            return;
        }
        if self.selected.is_none() && !ctx.egui_wants_keyboard_input() {
            let page_step = ui.available_height().max(240.0) * 0.85;
            let row_step = if self.list_view {
                110.0
            } else {
                self.settings.grid_density * 0.63 + 50.0
            };
            let delta = ctx.input(|input| {
                if input.key_pressed(egui::Key::PageDown) {
                    page_step
                } else if input.key_pressed(egui::Key::PageUp) {
                    -page_step
                } else if input.key_pressed(egui::Key::ArrowDown) {
                    row_step
                } else if input.key_pressed(egui::Key::ArrowUp) {
                    -row_step
                } else {
                    0.0
                }
            });
            self.content_scroll_offset = (self.content_scroll_offset + delta).max(0.0);
        }
        if self.list_view {
            let output = egui::ScrollArea::vertical()
                .id_salt("wall-list")
                .vertical_scroll_offset(self.content_scroll_offset)
                .show(ui, |ui| {
                    for item in &items {
                        ui.horizontal(|ui| {
                            self.thumb(ui, item, Vec2::new(156.0, 92.0));
                            ui.vertical(|ui| {
                                if ui.link(&item.title).clicked() {
                                    self.select_wallpaper(item.clone());
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}",
                                        item.resolution(),
                                        item.provider
                                    ))
                                    .color(MUTED),
                                );
                                ui.label(RichText::new(item.tags.join(" · ")).color(MUTED));
                            });
                            if ui.button("Favorite").clicked() {
                                self.toggle_favorite(item);
                            }
                            if ui.button("Download").clicked() {
                                self.start_download(item.clone());
                            }
                        });
                        ui.separator();
                    }
                    self.pagination_footer(ui);
                });
            self.content_scroll_offset = output.state.clamped_offset().y;
            let _ = ctx;
            return;
        }
        let width = ui.available_width();
        let max_card_w = self.settings.grid_density.clamp(160., 400.);
        let gap = ui.spacing().item_spacing.x;
        let natural_columns = ((width + gap) / (max_card_w + gap)).floor().max(1.) as usize;
        // Avoid a sparse final row when a nearby column count can fit the
        // current result set exactly (for example, 24 results in 4 × 6).
        let columns = [
            natural_columns.saturating_sub(1).max(1),
            natural_columns + 1,
        ]
        .into_iter()
        .filter(|candidate| *candidate != natural_columns && items.len() % candidate == 0)
        .min_by_key(|candidate| (candidate.abs_diff(natural_columns), *candidate))
        .unwrap_or(natural_columns);
        let card_w =
            ((width - gap * columns.saturating_sub(1) as f32) / columns as f32).clamp(160., 400.);
        let output = egui::ScrollArea::vertical()
            .id_salt("wall-grid")
            .auto_shrink([false, false])
            .vertical_scroll_offset(self.content_scroll_offset)
            .show(ui, |ui| {
                for row in items.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        for item in row {
                            self.card(ui, item, card_w);
                        }
                    });
                    ui.add_space(12.);
                }
                self.pagination_footer(ui);
            });
        self.content_scroll_offset = output.state.clamped_offset().y;
        let _ = ctx;
    }
    fn pagination_footer(&mut self, ui: &mut egui::Ui) {
        let sources: &[Source] = match self.source_choice {
            SourceChoice::All => &[Source::Wallhaven, Source::Reddit],
            SourceChoice::Wallhaven => &[Source::Wallhaven],
            SourceChoice::Reddit => &[Source::Reddit],
        };
        for source in sources.iter().copied() {
            let Some((current, last, _)) = self.source_pages.get(&source).cloned() else {
                continue;
            };
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.source_choice == SourceChoice::All {
                        ui.label(RichText::new(source.name()).size(11.0).color(MUTED));
                    }
                    let arrow = |text: &str| {
                        egui::Button::new(RichText::new(text).size(17.0).strong())
                            .min_size(Vec2::new(34.0, 34.0))
                    };
                    let can_go_previous = current > 1
                        && (source == Source::Wallhaven
                            || self
                                .source_page_cursors
                                .get(&source)
                                .is_some_and(|pages| pages.contains_key(&(current - 1))));
                    if ui
                        .add_enabled(can_go_previous && !self.loading, arrow("‹"))
                        .clicked()
                    {
                        self.search_page(source, current - 1);
                    }

                    for number in pagination_numbers(current, last) {
                        if let Some(number) = number {
                            let selected = number == current;
                            let available = source == Source::Wallhaven
                                || self
                                    .source_page_cursors
                                    .get(&source)
                                    .is_some_and(|pages| pages.contains_key(&number));
                            let button = egui::Button::new(
                                RichText::new(number.to_string())
                                    .size(14.0)
                                    .strong()
                                    .color(if selected { BG } else { TEXT }),
                            )
                            .fill(if selected { BLUE } else { SURFACE })
                            .min_size(Vec2::new(34.0, 34.0));
                            if ui.add_enabled(available && !self.loading, button).clicked()
                                && !selected
                            {
                                self.search_page(source, number);
                            }
                        } else {
                            ui.label(RichText::new("…").size(14.0).color(MUTED));
                        }
                    }

                    let can_go_next = current < last
                        && (source == Source::Wallhaven
                            || self
                                .source_page_cursors
                                .get(&source)
                                .is_some_and(|pages| pages.contains_key(&(current + 1))));
                    if ui
                        .add_enabled(can_go_next && !self.loading, arrow("›"))
                        .clicked()
                    {
                        self.search_page(source, current + 1);
                    }
                });
            });
        }
    }
    fn downloads_view(&mut self, ui: &mut egui::Ui) {
        if self.downloads.is_empty() && self.history.is_empty() {
            ui.label(RichText::new("Downloaded wallpapers will appear here.").color(MUTED));
        }
        let keys = self.downloads.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            if let Some(d) = self.downloads.get(&key) {
                ui.horizontal(|ui| {
                    ui.label(&d.item.title);
                    ui.label(RichText::new(d.status.label()).color(
                        if d.status == DownloadState::Complete {
                            GREEN
                        } else {
                            MUTED
                        },
                    ));
                    if let Some(total) = d.total {
                        ui.label(format!(
                            "{}%",
                            d.received.saturating_mul(100) / total.max(1)
                        ));
                    }
                    if matches!(
                        &d.status,
                        DownloadState::Starting | DownloadState::Downloading
                    ) && ui.button("Cancel").clicked()
                    {
                        d.cancel.store(true, Ordering::Relaxed);
                    }
                });
            }
        }
        ui.separator();
        let mut remove = None;
        for (index, entry) in self.history.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {}",
                    entry.wallpaper.title,
                    entry.path.display()
                ));
                if ui.button("Reveal").clicked() {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(entry.path.parent().unwrap_or(&entry.path))
                        .spawn();
                }
                if ui.button("Remove from history").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            self.history.remove(index);
            let _ = storage::save_history(&self.history);
        }
    }
    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Wallhaven API").strong().color(TEXT));
        ui.label(RichText::new("An API key is optional for public search. Configure one for higher limits from wallhaven.cc/settings/account.").color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut self.settings.api_key)
                .password(true)
                .hint_text("API key"),
        );
        ui.add_space(14.);
        ui.add_space(14.);
        ui.label(RichText::new("Reddit Data API").strong().color(TEXT));
        ui.label(RichText::new("Register an OAuth app and obtain Reddit's explicit API approval before using this source. Search uses OAuth and a descriptive user agent; it does not scrape Reddit.").color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut self.settings.reddit_client_id)
                .hint_text("OAuth client ID"),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.settings.reddit_client_secret)
                .password(true)
                .hint_text("OAuth client secret"),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.settings.reddit_username)
                .hint_text("Reddit username (for API user agent)"),
        );
        ui.checkbox(
            &mut self.settings.reddit_download_rights_confirmed,
            "I have permission to download images I select from Reddit",
        );
        ui.label(RichText::new("Reddit posts are user content. This setting is your confirmation of rights for downloads; WallBrowse does not infer a license from a post. Reddit may block clients without approved API access.").color(MUTED));
        ui.add_space(10.);
        ui.label(RichText::new("Pexels and Pixabay are unavailable for wallpaper-app use under their published API terms.").color(MUTED));
        ui.add_space(16.);
        ui.label("Download directory");
        let mut folder = self.settings.download_dir.to_string_lossy().to_string();
        if ui.text_edit_singleline(&mut folder).changed() {
            self.settings.download_dir = PathBuf::from(folder);
        }
        ui.label(
            RichText::new("Files are validated and saved without overwriting existing images.")
                .color(MUTED),
        );
        ui.add_space(12.);
        ui.label("Thumbnail cache directory");
        let mut cache_dir = self.settings.cache_dir.to_string_lossy().to_string();
        if ui.text_edit_singleline(&mut cache_dir).changed() {
            self.settings.cache_dir = PathBuf::from(cache_dir);
        }
        ui.add(
            egui::Slider::new(&mut self.settings.cache_limit_mb, 64..=4096)
                .text("Cache limit (MiB)"),
        );
        ui.add_space(12.);
        ui.label("Grid card width");
        ui.add(egui::Slider::new(&mut self.settings.grid_density, 170.0..=340.0).suffix(" px"));
        ui.checkbox(&mut self.settings.sfw_only, "SFW only by default");
        if ui.button("Save settings").clicked() {
            self.filters.sfw_only = self.settings.sfw_only;
            if let Err(e) = storage::save_settings(&self.settings) {
                self.error = Some(e.to_string());
            } else {
                self.error = None;
            }
        }
    }
    fn detail(&mut self, ctx: &egui::Context) {
        if let Some(item) = self.selected.clone() {
            let mut open = true;
            egui::Window::new("Wallpaper details")
                .open(&mut open)
                .default_width(760.)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(&item.title);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Open source page").clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(item.source_url.clone()));
                            }
                        });
                    });
                    ui.separator();
                    if let Some(tex) = self
                        .previews
                        .get(&item.id)
                        .or_else(|| self.textures.get(&item.id))
                    {
                        ui.add(
                            egui::Image::new(tex).max_size(Vec2::new(ui.available_width(), 420.)),
                        );
                    } else if let Some(error) = self.preview_errors.get(&item.id) {
                        ui.colored_label(RED, format!("Preview unavailable: {error}"));
                    } else {
                        ui.allocate_ui(Vec2::new(ui.available_width(), 320.), |ui| {
                            ui.centered_and_justified(|ui| ui.spinner())
                        });
                    }
                    ui.label(format!(
                        "{} · {:.2}:1 · {} · {} / {}",
                        item.resolution(),
                        item.aspect_ratio(),
                        item.provider,
                        item.category,
                        item.purity
                    ));
                    if let Some(bytes) = item.file_size {
                        ui.label(format!("File size: {:.1} MiB", bytes as f64 / 1048576.));
                    }
                    ui.label(RichText::new(item.tags.join("  ·  ")).color(MUTED));
                    if !item.attribution.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(&item.attribution);
                            if let Some(url) = &item.photographer_url
                                && ui.link("Photographer profile").clicked()
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
                            }
                            if let Some(license) = &item.license {
                                ui.label(RichText::new(license).color(MUTED));
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        let can_download = item.provider != "Reddit"
                            || self.settings.reddit_download_rights_confirmed;
                        if ui
                            .add_enabled(can_download, egui::Button::new("Download original"))
                            .clicked()
                        {
                            self.start_download(item.clone());
                        }
                        if ui
                            .button(if self.favorites.iter().any(|f| f.id == item.id) {
                                "Remove favorite"
                            } else {
                                "Add favorite"
                            })
                            .clicked()
                        {
                            self.toggle_favorite(&item);
                        }
                    });
                });
            if !open {
                self.selected = None;
            }
        }
    }
}

fn make_thumbnail_room(
    textures: &mut HashMap<String, egui::TextureHandle>,
    last_used: &mut HashMap<String, Instant>,
    requested: &mut HashSet<String>,
    visible: &HashSet<String>,
    selected_id: Option<&str>,
) -> bool {
    if textures.len() < MAX_THUMBNAIL_TEXTURES {
        return true;
    }

    let oldest = last_used
        .iter()
        .filter(|(cached_id, _)| {
            !visible.contains(*cached_id) && selected_id != Some(cached_id.as_str())
        })
        .min_by_key(|(_, used)| *used)
        .map(|(cached_id, _)| cached_id.clone());

    let Some(oldest) = oldest else {
        return false;
    };
    textures.remove(&oldest);
    last_used.remove(&oldest);
    requested.remove(&oldest);
    true
}

fn pagination_numbers(current: u32, last: u32) -> Vec<Option<u32>> {
    if last == 0 {
        return Vec::new();
    }
    let current = current.clamp(1, last);
    let mut numbers = (1..=last.min(5)).collect::<Vec<_>>();
    numbers.extend(current.saturating_sub(1).max(1)..=current.saturating_add(1).min(last));
    numbers.extend(last.saturating_sub(4).max(1)..=last);
    numbers.sort_unstable();
    numbers.dedup();

    let mut display = Vec::with_capacity(numbers.len() + 2);
    for number in numbers {
        if let Some(previous) = display.last().and_then(|entry| *entry) {
            if number.saturating_sub(previous) > 2 {
                display.push(None);
            } else if number.saturating_sub(previous) == 2 {
                display.push(Some(previous + 1));
            }
        }
        display.push(Some(number));
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_thumbnail_cache_evicts_lru_offscreen_item_and_admits_new_one() {
        let context = egui::Context::default();
        let mut textures = HashMap::new();
        let mut last_used = HashMap::new();
        let mut requested = HashSet::new();
        let visible = HashSet::from(["thumbnail-94".to_owned()]);
        let selected = "thumbnail-95";
        let now = Instant::now();

        for index in 0..MAX_THUMBNAIL_TEXTURES {
            let id = format!("thumbnail-{index}");
            let texture = context.load_texture(
                id.clone(),
                egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                egui::TextureOptions::LINEAR,
            );
            textures.insert(id.clone(), texture);
            last_used.insert(
                id.clone(),
                now - Duration::from_secs((MAX_THUMBNAIL_TEXTURES - index) as u64),
            );
            requested.insert(id);
        }

        assert!(make_thumbnail_room(
            &mut textures,
            &mut last_used,
            &mut requested,
            &visible,
            Some(selected),
        ));
        assert_eq!(textures.len(), MAX_THUMBNAIL_TEXTURES - 1);
        assert!(!textures.contains_key("thumbnail-0"));
        assert!(!last_used.contains_key("thumbnail-0"));
        assert!(!requested.contains("thumbnail-0"));
        assert!(textures.contains_key("thumbnail-94"));
        assert!(textures.contains_key(selected));

        let new_id = "thumbnail-new".to_owned();
        textures.insert(
            new_id.clone(),
            context.load_texture(
                new_id.clone(),
                egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                egui::TextureOptions::LINEAR,
            ),
        );
        assert_eq!(textures.len(), MAX_THUMBNAIL_TEXTURES);
    }

    #[test]
    fn filename_is_safe() {
        assert_eq!(
            sanitize_filename("wallhaven:abc/../X"),
            "wallhaven_abc____X"
        );
        assert_eq!(sanitize_filename("/"), "_");
    }

    #[test]
    fn sort_controls_only_offer_provider_supported_labels() {
        assert!(
            sort_options(SourceChoice::All)
                .iter()
                .all(|(sort, _)| matches!(sort, SortOrder::Relevance | SortOrder::DateAdded))
        );
        assert_eq!(sort_label(SourceChoice::Reddit, SortOrder::Views), "Hot");
        assert_eq!(
            sort_label(SourceChoice::Reddit, SortOrder::Favorites),
            "Top"
        );
    }
}
