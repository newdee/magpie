# magpie

> Spotlight-style search for everything you saved and forgot.

[简体中文](README.zh-CN.md) · **[Feature tour →](https://dfine.tech/magpie/)**

Magpies hoard shiny things and famously forget where they put them. So do we:
GitHub stars we never open again, files buried in project folders, screenshots
we can describe but cannot find, bookmarks and history lost in the browser,
that snippet we copied an hour ago, the app three menus deep. **magpie** is a
tiny desktop launcher that brings them all back. Press a hotkey, type what you
vaguely remember (or drop in an image), hit Enter.

<p align="center">
  <a href="https://github.com/newdee/magpie/releases/download/v0.1.24/magpie-demo.mp4">
    <img src="docs/img/magpie-demo.gif" width="640" alt="magpie in 60 seconds: summon, search everything, OCR your screenshots and videos, and a query-box toolbelt"></a>
  <br>
  <sub>▶ <a href="https://github.com/newdee/magpie/releases/download/v0.1.24/magpie-demo.mp4">full-quality demo with sound</a> · 60s · 2.4 MB</sub>
</p>

<p align="center"><img src="docs/img/palette-local.png" width="760" alt="magpie searching local files, an app hit on top, snippets highlighted"></p>

<table>
  <tr>
    <td><img src="docs/img/palette-stars.png" alt="searching GitHub stars"><br><sub>GitHub stars with full-README search, sorted and dated</sub></td>
    <td><img src="docs/img/web-badges.png" alt="bookmarks and history searched together"><br><sub>Bookmarks and browser history in one Web tab</sub></td>
  </tr>
  <tr>
    <td><img src="docs/img/clipboard.png" alt="clipboard history"><br><sub>Clipboard history: copy back, multi-select, delete</sub></td>
    <td><img src="docs/img/image-similarity.jpeg" alt="image-to-image search with similarity percentages"><br><sub>Drop in an image and similar files rank with percentages</sub></td>
  </tr>
</table>

## Why magpie

**Privacy you can check for yourself.**

- **No full-disk scanning, ever.** magpie only indexes folders you explicitly
  add, so it reads your working directories and nothing else. Scanning is
  recursive within those folders, `.gitignore` rules are respected (even
  outside git repos), hidden files are skipped, and symlinks are never
  followed out of the folders you chose. Nested or duplicate folders are
  rejected up front.
- **Everything stays on your machine.** The index is a single SQLite file in
  your user profile. Embedding models run locally via ONNX; after the one-time
  download the whole thing works offline. Bookmarks are read straight from
  local browser files, with no browser APIs and no accounts. Nothing is sent
  anywhere.
- **You can see exactly what is indexed** and remove any folder with one
  click; its files, search index, and vectors are wiped together. Every
  folder (and the star index) also has a rebuild-from-scratch button.

**Search that understands what you mean.**

- Hybrid retrieval: SQLite FTS5 (BM25) keyword search fused with local
  semantic embeddings (`multilingual-e5-small`) via reciprocal rank fusion.
- Cross-language: query in Chinese, match English READMEs and code, and the
  other way round (100+ languages).
- Long content is chunked (~1600 chars, overlapping) with one vector per
  chunk, so a sentence 100 pages deep still surfaces; documents and READMEs
  rank by their best chunk.
- Keyword hits show a highlighted context snippet with ellipsis.
- Millisecond-fast: vectors live in memory, a query embeds in ~5 ms, and
  keyword search keeps working while models warm up.

**One launcher for everything you keep.**

- **Six things, one keystroke**: local files (full-text + by name), images
  by content, GitHub stars, browser bookmarks and history, your clipboard,
  and installed apps, all behind `Alt+Space`.
- **Search inside your videos**: shots are detected and embedded, so a
  dropped image or a description finds the exact scene, with its time range.
- **A real app launcher**: type an app name (prefix, substring, or acronym
  like `vsc`) and `Enter` launches it from the Start Menu, `/Applications`,
  or a Linux `.desktop` entry. Chinese app names match by pinyin too: `wx`,
  `weixin`, or `txhy` launch 微信 and 腾讯会议 without switching your input
  method.
- **Speaks your language**: the whole UI (tray menu included) is available in
  English and 简体中文, following your OS by default and switchable in
  settings.
- **Clipboard history that respects secrets**: opt-in, stored locally, and
  clips a password manager marks confidential are never recorded (honored on
  Windows and macOS). Cap it by count and age, multi-select and delete, and
  `Shift+Enter` pastes straight into the app you came from. `Ctrl+P` pins a
  clip: it sorts first and survives count/age pruning.
- **Copied images are history too**: screenshots and copied pictures join
  the clipboard history with thumbnails, and SigLIP runs on your clips, so
  you can find them later by *describing* them ("that error dialog").
- **Search the text inside your screenshots and your videos**: opt-in
  OCR (PP-OCRv4, ~15 MB, Chinese + English) reads indexed images and each
  video shot's frame, so subtitles, slides, and on-screen code become
  searchable; a video hit jumps straight to the moment the text appears.
  Pick PP-OCRv4 (15 MB) or the sharper PP-OCRv6 small (30 MB) in settings,
  off by default. A separate sub-switch also reads scanned PDFs (pages with
  no text layer), left to you because large scans take a while.
- **It learns your habits**: results you actually open float up over time
  (frequency + recency, decaying). Those statistics stay on your machine and
  never outweigh a better match.
- **Videos open at the scene**: when your default player is VLC / mpv /
  PotPlayer / MPC, `Enter` on a shot starts playback at its time range.
- **Multi-monitor aware**: the palette summons on the display your cursor
  is on.
- **Settings travel with you**: one-click export/import (everything except
  the GitHub token).
- **A calculator lives in the query box**: `3*(5+2)^2`, `0xff + 1`,
  `100 mb to gb`, `32 f to c`. The answer appears as the top row and `Enter`
  copies it. Its siblings: `uuid`, `now` / `ts 1700000000`, `pwd 24` (CSPRNG
  passwords), `b64` / `unb64` / `url` / `unurl`, and `#ff6600` shows a color
  chip with rgb/hsl conversions.
- **Dates too**: `today + 30d`, `2026-10-01 - today`, `until 2026-10-01`,
  `tomorrow + 3 months`. A lone `2026-10-01` tells you the weekday and how
  far off it is.
- **Text verbs work on your clipboard**: `json` alone pretty-prints whatever
  you just copied; `upper`, `lower`, `trim`, `slug`, `lines` (dedupe and
  sort) and `count` (chars, words, lines) likewise. Give them an argument
  instead (`slug Hello World`) and they work on that. `Enter` puts the result
  back on the clipboard.
- **Narrow a file search inline**: `ext:pdf` or `.md`, `>10mb` / `<500kb`,
  `7d` (modified this week; `2w`, `3m`, `1y`), `in:projects`. Filters mix
  with words: `invoice ext:pdf 30d`.
- **Quick notes**: `note buy milk` appends one timestamped line to a markdown
  file of your choosing (Settings → Notes file). `Enter` writes it and the
  palette goes away.
- **Look up what you have selected**: `Ctrl+Alt+Space` (`Option+Shift+Space`
  on macOS) copies the selection in whatever app you are in and summons
  magpie with it as the query. Rebindable or removable in Settings.
- **Recent opens on the empty box**: switch it on and each tab lists what you
  last opened from it, so getting back to a file from a minute ago is two
  keystrokes.
- **Bang-style web shortcuts**: `gh magpie` searches GitHub, `g …` Google,
  `bd …` Baidu. Prefixes are editable rules in settings (`prefix = URL
  with {q}`).
- **Emoji lookup**: type `:` then a keyword (`:fire`, `:火`) and click or
  `Enter` to copy.
- **Every row has a copy**: `Ctrl+C` copies what identifies the row, a
  file's path, a repo's or page's URL, a clip's text. `Ctrl+Shift+C` puts a
  file itself on the clipboard, ready to paste into chat or mail as an
  attachment.
- **Stays current on its own**: signed, verified in-place auto-updates from
  this version on, with no reinstalling. Checked at launch and every 24 h
  while resident; a pending release shows a red dot on the tray icon and next
  to the settings hint, plus a tray menu entry. Installing stays your call.
- **A preview pane, one keypress away**: `→` shows the selected result in
  place: file text with your terms highlighted, full-size images, a video's
  shot strip, README head for repos, complete clips. Confirm before you open.
- **Yours to arrange**: reorder the tabs and pick which one opens on launch.
- **It teaches itself**: the empty palette shows a one-line tip, a shortcut
  or a hidden trick, fresh on every summon (off-switch in settings).
- **Works where the network doesn't**: one-click switch to `hf-mirror.com`
  for the model download, with resumable transfers and live progress. If both
  Hugging Face and the mirror are unreachable, the models fall back to
  magpie's own GitHub release assets automatically (same as ffmpeg).
- **Cross-platform**: Windows, macOS (Apple Silicon), and Linux.

## Sources (`Tab` cycles them)

### Local Files

Add your everyday working folders; every file in them is findable by
name, and understood formats are searched in full text:

- ~80 plain-text and code formats, read whole (size cap configurable, up to
  unlimited)
- PDF via [pdf-inspector](https://github.com/firecrawl/pdf-inspector)
  (scanned/garbled PDFs stay findable by name)
- Word / Excel / PowerPoint (docx, xlsx, pptx)
- Everything else (video, archives, binaries) indexes by filename
- Scope pills (or `Shift+Tab`) narrow results: all / text / images / videos
- Linked git worktrees are skipped when the checkout they belong to is
  already indexed, so a project with six worktrees is not indexed six times.
  A worktree that is the only copy is indexed as usual; there is an
  off-switch in settings.
- Indexing is capped at 4 CPU threads per model (text, image and OCR
  alike), so an embed pass never owns the machine. Settings offer 1, 2, 4,
  8 or every core; a change applies right away.

Images are embedded with **SigLIP 2** and searchable by content:

- *Text → image*: type "sunset over the sea" in any language; matching photos
  rank alongside other results, thumbnails included.
- *Image → image*: drag an image onto the palette, paste a screenshot, or
  click the pick-image button, and magpie returns the most similar indexed
  images with cosine-similarity percentages.

Videos get the same treatment, one level deeper: each video in your folders is
split into shots (histogram-based scene-change detection, a pure-Rust pass
over 2 fps frames), every shot's representative frames are SigLIP-embedded,
and both image and text queries can land *inside* a video: the result row
shows the matching shot's thumbnail and its exact time range (`3:24 – 3:42`).
The Local tab's videos scope searches them by filename and by scene
description in one list. Decoding uses ffmpeg: a system install is picked up
automatically, or a static build is fetched once from magpie's own releases
(reachable wherever you downloaded magpie from;
[GPL builds](https://ffmpeg.org/download.html) by gyan.dev / evermeet.cx /
johnvansickle.com, sources at ffmpeg.org). Toggleable in Settings.

The index is incremental: startup, manual refresh, and a 30-minute timer pick
up new, changed, and deleted files automatically.

### GitHub Stars

Paste a personal access token (no scopes needed) and magpie syncs your entire
star list: names, descriptions, topics, and full READMEs (chunk-embedded).
Unstars are detected, README fetches are ETag-incremental, and only changed
content re-embeds. Sort matches by relevance, starred date, or star count
(pills or `Shift+Tab`); every row shows last-push time so abandoned projects
are obvious.

### Applications

Typing in the Local tab surfaces matching installed apps as top hits (an
`App` badge marks them) and `Enter` launches. Names match by prefix,
substring, or acronym (`vsc` → Visual Studio Code). Chinese names also match
by full pinyin or initials (`wx` / `weixin` → 微信, `wangyiyun` → 网易云音乐),
heteronyms included (`cq` and `zq` both find 重庆…), so you never switch input
methods to launch an app. Toggleable in settings. Apps come from the Start
Menu on Windows, `/Applications` on macOS, and `.desktop` entries on Linux.

Apps also answer to aliases: a built-in bilingual table bridges Chinese and
English product names (`lark` finds 飞书, `weixin` finds an app installed as
*WeChat*, whichever direction your system has), Linux picks up `.desktop`
`Keywords=`/`GenericName=` for free, and you can add your own rules in
Settings (`proxy = clash`, one per line). Aliases match like second names,
pinyin included.

### Web (bookmarks + history)

The Web tab searches browser bookmarks and history together; `Shift+Tab`
narrows to all / bookmarks / history. History covers page titles and URLs
(not just addresses) with a visit-count boost so pages you open often rank
higher; only the most-visited pages per profile are kept. Bookmarks come from
any Chromium-based browser (Chrome, Edge, Brave, Vivaldi, Arc, and
lesser-known forks are auto-discovered by their on-disk profile layout) plus
Firefox. Read directly from local stores (all profiles), searchable by title,
URL, and folder path, with semantic matching on top. `Enter` opens the
bookmark in your default browser.

### Quick web launcher

magpie also doubles as the fastest route to the web. Summon the palette from
inside any app, type a URL or whatever you want to look up, and hit
`Ctrl+Enter`: URL-looking input opens directly in your default browser,
anything else becomes a web search. No switching windows, no reaching for
the browser's address bar first.

### Clipboard History

Off by default. When you enable it in Settings, copied text is recorded to a
local database (not the system clipboard history) and searchable from the
Clipboard tab. This is the one source where an empty query is useful, listing
what you most recently copied. `Enter` copies an entry back; `Ctrl+Delete`
removes the selected entries; `Shift`+arrows multi-select (and `Enter` then
copies them joined). Text marked confidential by password managers is never
recorded. Cap history by count (500 / 2000 / unlimited) and age (7 / 30 days
/ forever), or clear it entirely.

## Keyboard

| Key | Action |
|---|---|
| `Alt+Space` | summon / dismiss the palette (rebindable in settings) |
| `↑` `↓` / `PgUp` `PgDn` | move / page through results |
| `Enter` | context action: open a repo/bookmark/history page in the browser, reveal a file in Explorer/Finder, launch an app, or copy a clip |
| `Ctrl+Enter` | hand the query to the browser: URL-looking input opens directly, anything else web-searches |
| `Ctrl+C` | copy what identifies the row: a path, a URL, a clip's text (`Ctrl+Shift+C`: the file itself) |
| `Ctrl+Alt+Space` | in any app: look up the selected text (`Option+Shift+Space` on macOS; rebindable) |
| `Tab` | next source (Local / Stars / Web / Clipboard, order set in settings) |
| `Shift+Tab` | cycle the active source's mode: local scope (all/text/images), web scope (all/bookmarks/history), or star sort |
| `Shift+Enter` | paste the selected clip(s) into the previous app (Clipboard tab) |
| `Shift`+`↑` `↓` | extend a multi-selection (Clipboard tab) |
| `Ctrl+Delete` | delete the selected clips (Clipboard tab) |
| `→` / `←` | open / close the preview pane (`→` with the cursor at the end of the query) |
| `Alt+,` (or `Ctrl+,`) | toggle Settings ↔ search |
| `Esc` | clear image query → close settings → hide window |
| drop / paste / pick an image | search local images by similarity |

The palette sits top-center, stays above every window, never hides on focus
loss (so drag-and-drop works), and can be dragged by its tab strip. Tab order
and the tab that opens on launch are both configurable.

## Settings (tray icon → Settings…)

GitHub token (with connection badge) · indexed folders (add / remove /
rebuild) · appearance (auto / light / dark) · UI language (auto / English /
中文) · pinyin app matching · app aliases · summon shortcut (recordable) ·
search-selection shortcut · notes file · recent opens on the empty box ·
model download source (huggingface.co or hf-mirror.com for networks where
HF is unreachable) · max file size (4/16/64 MB or unlimited) · video shot
search toggle · decode limits (threads / hardware decode) · indexing
threads (1 to every core) · tab order and
default tab · clipboard history controls · model download status · one-click
in-place updates (signed, verified) · settings export/import · MCP server
for AI assistants (off by default) · open-log-folder (local activity log for
bug reports; queries never logged) · app version.

## AI assistants (MCP)

magpie can serve its index to Claude Code, Cursor and any other MCP client,
over HTTP on the loopback interface. Off by default. Switch it on in Settings
→ "MCP server for AI assistants", press "Copy Claude Code command" and paste
into a terminal:

```
claude mcp add --transport http magpie http://127.0.0.1:PORT/mcp --header "Authorization: Bearer TOKEN"
```

Other clients take the same URL with the same `Authorization` header.

Three read-only tools:

- `search`: `source` is one of `local`, `stars`, `bookmarks`, `history`,
  `clips`; local queries take the palette's filters (`ext:pdf`, `>10mb`,
  `7d`, `in:folder`). Hits are ranked exactly as the palette ranks them.
- `read_file`: the indexed text of one local hit (PDF, Office and OCR text
  included), 20 000 characters by default. Only paths inside the indexed
  folders.
- `recent`: what you opened lately through magpie.

What keeps it contained: the server binds `127.0.0.1` on a random port that
is then remembered; every request needs the bearer token (minted when first
enabled, "New token" replaces it); foreign `Host` and browser `Origin`
headers are refused; nothing writes. Token and port never enter a settings
export. If your shell sets `HTTP_PROXY`, add `127.0.0.1` to `NO_PROXY` so the
client reaches the server directly.

## Install

Grab the latest build from [Releases](https://github.com/newdee/magpie/releases):
Windows NSIS installer, macOS dmg (Apple Silicon), Linux AppImage/deb/rpm.

macOS builds are Developer ID signed and notarized (since v0.1.24), so they
open like any other app and folder-permission grants survive updates.

First launch downloads the embedding models (~700 MB total); keyword search
works immediately while they warm up.

## Build from source

```sh
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # release bundle
cargo test -p magpie-core    # core tests
```

Requires Rust, Node + pnpm, and a WebView2/WebKit runtime (bundled on
Windows 11 and macOS).

## Architecture

```
core/       Rust library: SQLite + FTS5, GitHub sync, folder indexing,
            bookmark + history + clipboard + app sources, e5 + SigLIP
            embeddings, hybrid ranking
src-tauri/  thin Tauri shell: commands, tray, global hotkey, window,
            clipboard watcher, auto-updater
src/        React palette UI (one window)
```

The vector "database" is deliberately boring: L2-normalized f32 BLOBs in
SQLite, brute-forced in memory (<15 ms at tens of thousands of chunks, 100%
recall). If a corpus ever outgrows that, `sqlite-vec` drops into the same
file.

## Roadmap

- Twitter/X likes as a source (via your data-export archive, no paid API)

## License

[MIT](LICENSE)
