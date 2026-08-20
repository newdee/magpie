# magpie

Spotlight-style launcher for searching things you've saved and forgotten:
your GitHub stars, and local folders you explicitly add. Press `Alt+Space`,
type what you vaguely remember, hit `Enter`.

## Search

Hybrid retrieval, fully local after sync:

- **Keyword**: SQLite FTS5 (BM25) over name / description / topics / README / file content
- **Semantic**: `multilingual-e5-small` embeddings via fastembed (384-dim,
  fp32 ONNX ~470 MB, downloaded once to app data, then offline). Chinese
  queries match English READMEs.
- Fused with Reciprocal Rank Fusion + a small cosine-similarity term.
- Vectors are brute-forced (≤10k items → <5 ms); if the collection ever grows
  past ~100k, swap in `sqlite-vec` on the same database.

If the model is missing (first run, no network), keyword search still works;
vectors backfill automatically once the model loads.

## Sources

- **GitHub Stars** — needs a PAT with no scopes (public star list + READMEs).
  Sync pulls the full star list (detects unstars), fetches READMEs with ETag
  conditional requests, and re-embeds only changed docs.
- **Local Files** — only folders you add are scanned. `.gitignore` respected
  (even outside git repos), hidden files skipped, symlinks not followed,
  binaries rejected, 4 MB/file cap. `Enter` reveals the file in
  Explorer/Finder.
- `Tab` switches sources. More sources (e.g. Twitter likes) plug into the same
  pattern: a table + FTS + embeddings + a `search_*` command.

## Architecture

```
core/       Rust lib: db.rs (SQLite+FTS5), github.rs, files.rs, embed.rs,
            search.rs (hybrid ranking), sync.rs — no Tauri dependency
src-tauri/  thin shell: commands, tray, global shortcut, window management
src/        React palette UI (one window, frosted, follows system theme)
```

Data lives in the per-user app data dir (`com.dfine.magpie/stars.db`,
`models/`). The GitHub token is stored in that local database, plaintext.

## Build

```sh
pnpm install
pnpm tauri dev            # development
pnpm tauri build          # release bundle
cargo test -p magpie-core   # core unit tests
```

## Keys

`Alt+Space` toggle · `↑↓` navigate · `PgUp/PgDn` page · `Enter` open ·
`Tab` switch source · `Esc` hide. Closing the window hides it; quit from the
tray menu.
