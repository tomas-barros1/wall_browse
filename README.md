# WallBrowse

WallBrowse is a native Rust wallpaper discovery and download app. It does not change the desktop background. Wallhaven and Reddit are implemented. Unsplash's undocumented web endpoint returned HTTP 401 in normal app use, so its search provider has been removed. Pexels and Pixabay are unavailable because their published API terms conflict with wallpaper-app use (see provider notes below).

## Current features

- Wallhaven search with pagination, sort order, SFW default, category, minimum resolution, aspect ratio, and color filters.
- Reddit OAuth search, cursor pagination, image-post filtering, SFW classification, author/subreddit attribution, and source-page links. Reddit downloads require an explicit rights confirmation in Settings.
- All-sources search that returns provider results independently, retains provider-specific pagination, and reports partial failures. Combined results are not represented as globally ranked.
- Responsive grid and list views, wallpaper details, favorites, and source-page links.
- Asynchronous thumbnail loading with bounded disk caching and original image downloads (100 MiB maximum), image validation, duplicate-safe filenames, temporary-file writes, and cancellation.
- JSON settings, favorites, and download history in the platform user directories. On Linux these follow XDG paths through `directories`.
- Downloads default to `~/Pictures/Wallpapers` when the Pictures directory is available.

Favorites remain available offline with their saved metadata and cached thumbnails. Download history links to locally saved files when the source is offline.

## Build and run (Arch Linux / CachyOS)

Install Rust via rustup and the system graphics development libraries required by eframe/Wayland or X11, then run:

```sh
cargo run --release
```

To install a local release binary, desktop launcher, and application icon under `~/.local`:

```sh
make install
```

The launcher appears in the desktop application menu. Use `make uninstall` to remove those installed files. Set `PREFIX`, `BINDIR`, `DATADIR`, or `DESTDIR` to customize installation paths. `make run`, `make check`, `make fmt`, and `make clippy` are also available.

For a local verification build use `cargo check`; tests use `cargo test`. Cargo requires access to crates.io on first build unless dependencies are already cached.

## Wallhaven API key

Public search is attempted without a key. A key can be entered in **Settings** to authenticate requests and access the account's higher API limits. Create one in the API section of your Wallhaven account settings. The key is stored in the user's config directory and is never written to logs.

## Reddit Data API

In **Settings → Reddit Data API**, enter an approved OAuth client's ID and secret, plus your Reddit username for the required descriptive user-agent. Reddit API access is approval-based; request access through [Reddit's developer support form](https://support.reddithelp.com/hc/en-us/requests/new?ticket_form_id=360001103212) and configure credentials only after approval. WallBrowse uses OAuth and Reddit's official search endpoint; it does not scrape Reddit. Search returns image posts only, and SFW mode excludes posts marked adult or missing an adult-content classification. You must confirm that you have rights to download selected Reddit images before enabling those downloads. Reddit data may be deleted or access revoked, so saved Reddit metadata and images should be managed accordingly.

## Provider availability and API terms

- **Pexels:** not integrated. Its [official API guidelines](https://www.pexels.com/api/documentation/) say developers may not make Pexels content available as a wallpaper app. That directly conflicts with WallBrowse's purpose.
- **Pixabay:** not integrated. Its [API docs](https://pixabay.com/api/docs/) require caching API responses for 24 hours, disallow permanent hotlinking, and prohibit systematic mass downloads. Its [Terms of Service](https://pixabay.com/service/terms/) prohibit compiling content to create a similar or competing service. Since WallBrowse is specifically a wallpaper discovery and download app, this use is not clearly permitted; integration is withheld unless Pixabay grants permission.
- **Unsplash:** not currently integrated. WallYou's unofficial `unsplash.com/napi` endpoints returned HTTP 401 when used by WallBrowse. A CLI check could not resolve `unsplash.com` in this environment, so a live retry was unavailable. The documented API requires an application access key.
- **Reddit:** implemented with official OAuth. Reddit's current [Responsible Builder Policy](https://support.reddithelp.com/hc/en-us/articles/42728983564564-Responsible-Builder-Policy) requires explicit approval before Data API access. Its [Data API Terms](https://redditinc.com/policies/data-api-terms) require compliance with creator restrictions, and Reddit's [Data API guidance](https://support.reddithelp.com/hc/en-us/articles/16160319875092-Reddit-Data-API-Wiki) says deleted posts and related data must be removed. Reddit metadata is persisted with favorites and history; WallBrowse does not automatically revalidate or purge Reddit items when a post is deleted.

In All Sources, only implemented providers are queried. Provider failures are shown independently; other sources continue to work.

## Data locations

Application settings are stored in the platform configuration directory. Favorites and download history are stored in its application data directory. Downloaded wallpaper files remain in the user-selected download folder; removing a history row does not delete the file.

## Known limitations

- The detail window loads Wallhaven's larger preview image separately from the card thumbnail; while offline it falls back to the cached card image.
- Cache eviction is based on file modification times and can briefly exceed its configured limit while several thumbnails finish writing concurrently.
- Provider credentials are stored in the configuration JSON in plain text, protected only by normal user filesystem permissions.
- Cancellation is checked between download chunks; an in-flight network read may take until the next chunk/timeout to stop.
- Full runtime/build verification depends on fetching the Rust crate dependencies.
