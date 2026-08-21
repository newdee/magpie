# magpie

> Spotlight-style search for everything you saved and forgot.

[简体中文](README.zh-CN.md)

Magpies hoard shiny things and famously forget where they put them. So do we:
GitHub stars we never open again, files buried in project folders, screenshots
we can describe but cannot find. **magpie** is a tiny desktop launcher that
brings them back — press a hotkey, type what you vaguely remember (or drop in
an image), hit Enter.

## Why magpie

**Privacy first, by architecture — not by promise.**

- **No full-disk scanning, ever.** magpie only indexes folders you explicitly
  add — point it at your working directories and nothing else is ever read.
  `.gitignore` rules are respected (even outside git repos), hidden files are
  skipped, and symlinks are never followed out of the folders you chose.
- **Everything stays on your machine.** The index is a single SQLite file in
  your user profile. Embedding models run locally via ONNX; after the one-time
  model download the whole thing works offline. Nothing is sent anywhere.
- **You can see exactly what is indexed** and remove any folder with one
  click; its files, search index, and vectors are wiped together.

**Search that understands meaning, not just keywords.**

- Hybrid retrieval: SQLite FTS5 (BM25) keyword search fused with local
  semantic embeddings (`multilingual-e5-small`) via reciprocal rank fusion.
- Cross-language: query in Chinese, match English READMEs and code — and vice
  versa (100+ languages).
- Keyword hits show a highlighted context snippet with ellipsis, so you can
  tell at a glance why a file matched.
- Millisecond-fast: vectors live in memory, a query embeds in ~5 ms, and
  keyword search keeps working while models warm up.

## Sources

### GitHub Stars

Paste a personal access token (no scopes needed) and magpie syncs your entire
star list: names, descriptions, topics, and READMEs. Unstars are detected,
README fetches are ETag-incremental, and only changed content is re-embedded.
Sort matches by relevance, starred date, or star count; every row shows the
last-push time so you can spot abandoned projects instantly.

### Local Files

Add your everyday working folders and search their **full text**:

- ~80 plain-text and code formats, read whole (configurable size cap,
  including unlimited)
- **PDF** via [pdf-inspector](https://github.com/firecrawl/pdf-inspector)
  (scanned/garbled PDFs stay findable by name)
- **Word / Excel / PowerPoint** (docx, xlsx, pptx)
- Long documents are chunked (~1600 chars, overlapping) with one vector per
  chunk, so a sentence 100 pages deep is still found — a file ranks by its
  best chunk.

The index is incremental: startup, a manual refresh, and a 30-minute timer
pick up new, changed, and deleted files automatically.

### Images

Every image in your folders is embedded with **SigLIP 2** and searchable by
what is *in* it:

- **Text → image**: type "sunset over the sea" (in any language) and matching
  photos rank alongside your other results, thumbnails included.
- **Image → image**: drag an image onto the palette — or paste a screenshot —
  and magpie finds the most similar indexed images, with cosine similarity
  percentages.

## The palette

- Global hotkey (default `Alt+Space`, configurable) summons a frosted
  always-on-top palette; `Esc` dismisses it. It never hides on its own — drag
  files in without losing the window.
- `↑↓` navigate · `PgUp/PgDn` page · `Enter` opens (repos in your browser,
  files revealed in Explorer/Finder) · `Tab` switches source.
- Light/dark/auto theme, live progress line while indexing, tray icon with
  quick actions.
- Settings panel holds everything: GitHub token, indexed folders, theme,
  hotkey, file-size cap.

## Install

Grab the latest build from [Releases](https://github.com/newdee/magpie/releases):
Windows NSIS installer, macOS dmg (Apple Silicon), Linux AppImage/deb/rpm.

**macOS tip** — builds are unsigned, so a browser-downloaded dmg triggers
Gatekeeper. Installing via curl skips the quarantine flag entirely:

```sh
curl -L https://github.com/newdee/magpie/releases/latest/download/magpie_aarch64.app.tar.gz | tar xz -C /Applications
```

(Or: right-click the app → Open; if it says "damaged", run
`xattr -cr /Applications/magpie.app`.)

## Build from source

```sh
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # release bundle
cargo test -p magpie-core    # core tests
```

Requires Rust, Node + pnpm, and a WebView2/WebKit runtime (bundled on
Windows 11 and macOS). Embedding models (~700 MB total) download on first
launch; keyword search is available immediately while they warm up.

## Architecture

```
core/       Rust library: SQLite + FTS5, GitHub sync, folder indexing,
            e5 + SigLIP embeddings, hybrid ranking — no UI dependency
src-tauri/  thin Tauri shell: commands, tray, global hotkey, window
src/        React palette UI (one window)
```

The vector "database" is deliberately boring: L2-normalized f32 BLOBs in
SQLite, brute-forced in memory (<15 ms at tens of thousands of chunks, 100%
recall). If a corpus ever outgrows that, `sqlite-vec` drops into the same
file.

## Roadmap

- Twitter/X likes as a source (via your data-export archive — no paid API)
- OCR for scanned PDFs
- Result preview pane

## License

[MIT](LICENSE)
