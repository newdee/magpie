import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface RepoHit {
  kind: "repo";
  id: number;
  full_name: string;
  description: string | null;
  language: string | null;
  stars: number;
  html_url: string;
  archived: boolean;
  score: number;
}

interface FileHit {
  kind: "file";
  id: number;
  path: string;
  name: string;
  ext: string | null;
  size: number;
  mtime: number;
  score: number;
  thumb: string | null;
}

type Hit = RepoHit | FileHit;

interface FolderInfo {
  id: number;
  path: string;
  file_count: number;
}

interface Status {
  repo_count: number;
  file_count: number;
  embedded_count: number;
  last_sync: string | null;
  username: string | null;
  has_token: boolean;
  model: string;
  image_model: string;
  syncing: boolean;
  local_indexing: boolean;
}

interface StarsProgress {
  stage: "listing" | "readmes" | "embedding";
  page?: number;
  total: number;
  done?: number;
}

interface LocalProgress {
  stage: "scan" | "embed" | "embed-images";
  done: number;
  total?: number;
}

const PAGE = 8;
const WINDOW_WIDTH = 720;

const SOURCES = [
  { id: "local", label: "Local Files" },
  { id: "github-stars", label: "GitHub Stars" },
] as const;

const IMAGE_EXT_RE = /\.(jpe?g|png|webp|bmp|gif)$/i;

interface ImageQuery {
  label: string;
  path?: string;
  bytesB64?: string;
}

type RepoSort = "relevance" | "starred" | "stars";
const SORTS: { id: RepoSort; label: string }[] = [
  { id: "relevance", label: "match" },
  { id: "starred", label: "recent" },
  { id: "stars", label: "★" },
];

function formatStars(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
  return String(n);
}

function formatSize(n: number): string {
  if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)}MB`;
  if (n >= 1024) return `${Math.round(n / 1024)}KB`;
  return `${n}B`;
}

function parentDir(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : path;
}

function starsProgressLabel(p: StarsProgress): string {
  switch (p.stage) {
    case "listing":
      return `listing stars… ${p.total}`;
    case "readmes":
      return p.total === 0 ? "readmes up to date" : `readmes ${p.done}/${p.total}`;
    case "embedding":
      return p.total === 0 ? "index up to date" : `indexing ${p.done}/${p.total}`;
  }
}

function localProgressLabel(p: LocalProgress): string {
  if (p.stage === "scan") return `scanning files… ${p.done}`;
  if ((p.total ?? 0) === 0) return "index up to date";
  return p.stage === "embed-images"
    ? `indexing images ${p.done}/${p.total}`
    : `indexing ${p.done}/${p.total}`;
}

export default function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Hit[]>([]);
  const [selected, setSelected] = useState(0);
  // default to local files; remember the last used source across restarts
  const [sourceIdx, setSourceIdx] = useState(() => {
    const saved = localStorage.getItem("magpie.source");
    const idx = SOURCES.findIndex((s) => s.id === saved);
    return idx >= 0 ? idx : SOURCES.findIndex((s) => s.id === "local");
  });
  const [status, setStatus] = useState<Status | null>(null);
  const [starsProgress, setStarsProgress] = useState<StarsProgress | null>(null);
  const [localProgress, setLocalProgress] = useState<LocalProgress | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [showFolders, setShowFolders] = useState(false);
  const [showTokenCard, setShowTokenCard] = useState(false);
  const [tokenInput, setTokenInput] = useState("");
  const [tokenBusy, setTokenBusy] = useState(false);
  const [tokenError, setTokenError] = useState<string | null>(null);
  const [imageQuery, setImageQuery] = useState<ImageQuery | null>(null);
  const [repoSort, setRepoSort] = useState<RepoSort>(() => {
    const saved = localStorage.getItem("magpie.sort") as RepoSort | null;
    return saved && SORTS.some((s) => s.id === saved) ? saved : "relevance";
  });

  const inputRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const queryRef = useRef(query);
  queryRef.current = query;
  const sourceRef = useRef(sourceIdx);
  sourceRef.current = sourceIdx;
  const needsTokenRef = useRef(false);
  const imageQueryRef = useRef<ImageQuery | null>(null);
  imageQueryRef.current = imageQuery;

  const source = SOURCES[sourceIdx].id;
  const needsToken = source === "github-stars" && status !== null && !status.has_token;
  needsTokenRef.current = status !== null && !status.has_token;

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await invoke<Status>("get_status"));
    } catch {
      /* backend not ready yet */
    }
  }, []);

  const refreshFolders = useCallback(async () => {
    try {
      setFolders(await invoke<FolderInfo[]>("list_folders"));
    } catch {
      /* ignore */
    }
  }, []);

  const repoSortRef = useRef(repoSort);
  repoSortRef.current = repoSort;

  const runSearch = useCallback(async (q: string, srcIdx: number) => {
    try {
      let hits: Hit[];
      if (SOURCES[srcIdx].id === "github-stars") {
        const rs = await invoke<Omit<RepoHit, "kind">[]>("search_stars", {
          query: q,
          sort: repoSortRef.current,
        });
        hits = rs.map((r) => ({ ...r, kind: "repo" as const }));
      } else {
        const fs = await invoke<Omit<FileHit, "kind">[]>("search_local", { query: q });
        hits = fs.map((f) => ({ ...f, kind: "file" as const }));
      }
      if (queryRef.current === q && sourceRef.current === srcIdx) {
        setResults(hits);
        setSelected(0);
      }
    } catch {
      /* transient: db busy during migration */
    }
  }, []);

  const acceptImageQuery = useCallback((iq: ImageQuery) => {
    setQuery("");
    setImageQuery(iq);
    setSourceIdx(0); // images live in the local source
    setShowFolders(false);
    inputRef.current?.focus();
  }, []);

  // live search, debounced; an active image query searches by similarity instead
  useEffect(() => {
    if (imageQuery) {
      invoke<Omit<FileHit, "kind">[]>("search_by_image", {
        path: imageQuery.path ?? null,
        bytesB64: imageQuery.bytesB64 ?? null,
      })
        .then((fs) => {
          if (imageQueryRef.current !== imageQuery) return; // stale
          setResults(fs.map((f) => ({ ...f, kind: "file" as const })));
          setSelected(0);
          setLastError(null);
        })
        .catch((e) => setLastError(String(e)));
      return;
    }
    const t = setTimeout(() => runSearch(query, sourceIdx), 120);
    return () => clearTimeout(t);
  }, [query, sourceIdx, imageQuery, repoSort, runSearch]);

  // paste an image from the clipboard to search by it
  useEffect(() => {
    const onPaste = async (e: ClipboardEvent) => {
      const item = Array.from(e.clipboardData?.items ?? []).find((i) =>
        i.type.startsWith("image/"),
      );
      if (!item) return;
      e.preventDefault();
      const file = item.getAsFile();
      if (!file) return;
      const buf = new Uint8Array(await file.arrayBuffer());
      let bin = "";
      const chunk = 0x8000;
      for (let i = 0; i < buf.length; i += chunk) {
        bin += String.fromCharCode(...buf.subarray(i, i + chunk));
      }
      acceptImageQuery({ label: "pasted image", bytesB64: btoa(bin) });
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [acceptImageQuery]);

  // backend events
  useEffect(() => {
    refreshStatus();
    refreshFolders();
    const subs = [
      listen<StarsProgress>("sync-progress", (e) => {
        setStarsProgress(e.payload);
        setLastError(null);
      }),
      listen("sync-done", () => {
        setStarsProgress(null);
        refreshStatus();
        runSearch(queryRef.current, sourceRef.current);
      }),
      listen<string>("sync-error", (e) => {
        setStarsProgress(null);
        setLastError(e.payload);
        refreshStatus();
      }),
      listen<LocalProgress>("local-progress", (e) => setLocalProgress(e.payload)),
      listen("local-done", () => {
        setLocalProgress(null);
        refreshStatus();
        refreshFolders();
        runSearch(queryRef.current, sourceRef.current);
      }),
      listen<string>("local-error", (e) => {
        setLocalProgress(null);
        setLastError(e.payload);
      }),
      listen("model-status", () => refreshStatus()),
      listen("embed-caught-up", () => {
        refreshStatus();
        runSearch(queryRef.current, sourceRef.current);
      }),
      listen("palette-shown", () => {
        inputRef.current?.focus();
        inputRef.current?.select();
        refreshStatus();
      }),
      // drop an image file anywhere on the palette to search by it
      getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          const p = e.payload.paths.find((x) => IMAGE_EXT_RE.test(x));
          if (p) {
            acceptImageQuery({ label: p.split(/[\\/]/).pop() ?? "image", path: p });
          }
        }
      }),
      // no hide-on-blur: the palette stays until the user dismisses it
      // explicitly (Esc, Alt+Space, tray) — dragging files in needs the
      // window to survive losing focus
    ];
    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, [refreshStatus, refreshFolders, runSearch]);

  // window height follows content
  useLayoutEffect(() => {
    const el = panelRef.current;
    if (!el) return;
    const h = Math.min(Math.max(el.offsetHeight, 96), 620);
    getCurrentWindow().setSize(new LogicalSize(WINDOW_WIDTH, h)).catch(() => {});
  });

  // keep selection visible
  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-idx="${selected}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selected, results]);

  const openHit = useCallback(async (hit: Hit | undefined) => {
    if (!hit) return;
    try {
      if (hit.kind === "repo") {
        await invoke("open_repo", { url: hit.html_url });
      } else {
        await invoke("open_file", { path: hit.path });
      }
      await getCurrentWindow().hide();
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const switchSource = useCallback((idx: number) => {
    setSourceIdx(idx);
    localStorage.setItem("magpie.source", SOURCES[idx].id);
    setSelected(0);
    setShowFolders(false);
    inputRef.current?.focus();
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const max = results.length - 1;
      const nav = ["ArrowDown", "ArrowUp", "PageDown", "PageUp"].includes(e.key);
      if (nav && max < 0) {
        e.preventDefault();
        return;
      }
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelected((s) => Math.min(s + 1, max));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelected((s) => Math.max(s - 1, 0));
          break;
        case "PageDown":
          e.preventDefault();
          setSelected((s) => Math.min(s + PAGE, max));
          break;
        case "PageUp":
          e.preventDefault();
          setSelected((s) => Math.max(s - PAGE, 0));
          break;
        case "Enter":
          e.preventDefault();
          openHit(results[selected]);
          break;
        case "Escape":
          e.preventDefault();
          if (imageQuery) {
            setImageQuery(null); // first Esc clears the image query
          } else {
            getCurrentWindow().hide();
          }
          break;
        case "Tab":
          e.preventDefault();
          switchSource((sourceIdx + 1) % SOURCES.length);
          break;
      }
    },
    [results, selected, sourceIdx, imageQuery, openHit, switchSource],
  );

  const refresh = useCallback(async () => {
    setLastError(null);
    try {
      await invoke(source === "github-stars" ? "start_sync" : "index_local");
      refreshStatus();
    } catch (e) {
      setLastError(String(e));
    }
  }, [source, refreshStatus]);

  const submitToken = useCallback(async () => {
    if (!tokenInput.trim() || tokenBusy) return;
    setTokenBusy(true);
    setTokenError(null);
    try {
      await invoke<string>("set_token", { token: tokenInput.trim() });
      setTokenInput("");
      await refreshStatus();
    } catch (e) {
      setTokenError(String(e));
    } finally {
      setTokenBusy(false);
    }
  }, [tokenInput, tokenBusy, refreshStatus]);

  const addFolder = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    try {
      setFolders(await invoke<FolderInfo[]>("add_folder", { path: dir }));
      setLastError(null);
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const removeFolder = useCallback(async (id: number) => {
    try {
      setFolders(await invoke<FolderInfo[]>("remove_folder", { folderId: id }));
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const busy =
    source === "github-stars"
      ? starsProgress !== null || (status?.syncing ?? false)
      : localProgress !== null || (status?.local_indexing ?? false);
  const modelWarming = status !== null && status.model === "loading";
  const modelFailed = status !== null && status.model.startsWith("failed");
  const manageFolders = source === "local" && showFolders;

  const footerStatus = lastError
    ? `error: ${lastError}`
    : source === "github-stars" && starsProgress
      ? starsProgressLabel(starsProgress)
      : source === "local" && localProgress
        ? localProgressLabel(localProgress)
        : modelFailed
          ? "semantic index unavailable, keyword search only"
          : modelWarming
            ? "semantic index warming up"
            : source === "local" && status?.image_model === "loading"
              ? "image search warming up"
              : status
              ? source === "github-stars"
                ? `${status.repo_count} repos indexed`
                : `${status.file_count} files indexed`
              : "";

  return (
    <div className="panel" ref={panelRef} onKeyDown={onKeyDown}>
      <div className="source-row">
        {SOURCES.map((s, i) => (
          <button
            key={s.id}
            className={`source ${i === sourceIdx ? "active" : ""}`}
            onClick={() => switchSource(i)}
            tabIndex={-1}
          >
            {s.label}
          </button>
        ))}
        {source === "github-stars" && (
          <span className="sort-group">
            {SORTS.map((s) => (
              <button
                key={s.id}
                className={`source ${repoSort === s.id ? "active" : ""}`}
                onClick={() => {
                  setRepoSort(s.id);
                  localStorage.setItem("magpie.sort", s.id);
                  inputRef.current?.focus();
                }}
                tabIndex={-1}
                title={`Sort by ${s.id}`}
              >
                {s.label}
              </button>
            ))}
          </span>
        )}
      </div>

      <div className="input-row">
        <svg className="search-icon" viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="6.5" cy="6.5" r="4.75" fill="none" stroke="currentColor" strokeWidth="1.5" />
          <line x1="10.5" y1="10.5" x2="14" y2="14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
        {imageQuery && (
          <span className="img-chip">
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
              <circle cx="5.5" cy="6.5" r="1.25" fill="currentColor" />
              <path d="M2.5 12l3.5-3.5 2.5 2.5 3-3 2 2" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
            </svg>
            <span className="img-chip-label">{imageQuery.label}</span>
            <button onClick={() => setImageQuery(null)} aria-label="Clear image query">
              ✕
            </button>
          </span>
        )}
        <input
          ref={inputRef}
          className="query"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            if (imageQuery) setImageQuery(null); // typing replaces the image query
          }}
          placeholder={
            imageQuery
              ? "Searching by image similarity"
              : source === "github-stars"
                ? status && status.repo_count > 0
                  ? `Search ${status.repo_count} starred repos`
                  : "Search your stars"
                : status && status.file_count > 0
                  ? `Search ${status.file_count} local files, drop or paste an image`
                  : "Search indexed folders"
          }
          autoFocus
          spellCheck={false}
          autoCorrect="off"
          autoCapitalize="off"
        />
        {source === "local" && (
          <button
            className={`icon-btn ${showFolders ? "active" : ""}`}
            onClick={() => setShowFolders((v) => !v)}
            title="Manage indexed folders"
            aria-label="Manage folders"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M1.75 4.25c0-.55.45-1 1-1h3l1.5 1.5h6c.55 0 1 .45 1 1v6c0 .55-.45 1-1 1h-10.5c-.55 0-1-.45-1-1z"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinejoin="round"
              />
            </svg>
          </button>
        )}
        <button
          className={`icon-btn ${busy ? "spinning" : ""}`}
          onClick={refresh}
          disabled={busy || needsToken}
          title={source === "github-stars" ? "Re-fetch starred repos" : "Re-scan folders"}
          aria-label="Refresh"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path
              d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 1.5v3h-3"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </div>

      {needsToken && !showTokenCard && (
        <button className="collapse-bar" onClick={() => setShowTokenCard(true)}>
          <span>Connect GitHub to sync your stars</span>
          <svg className="chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}

      {needsToken && showTokenCard && (
        <div className="card">
          <div className="card-head">
            <p className="card-title">Connect GitHub</p>
            <button
              className="icon-btn"
              onClick={() => setShowTokenCard(false)}
              title="Collapse"
              aria-label="Collapse"
            >
              <svg className="chevron up" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          </div>
          <p className="card-body">
            Paste a personal access token. No scopes needed, it only reads your public stars.
          </p>
          <div className="token-row">
            <input
              className="token-input"
              type="password"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.stopPropagation();
                  submitToken();
                }
              }}
              placeholder="ghp_…"
              spellCheck={false}
            />
            <button className="primary-btn" onClick={submitToken} disabled={tokenBusy}>
              {tokenBusy ? "Checking" : "Connect"}
            </button>
          </div>
          {tokenError && <p className="error-line">{tokenError}</p>}
          <button
            className="link-btn"
            onClick={() =>
              invoke("open_repo", {
                url: "https://github.com/settings/tokens/new?description=magpie",
              })
            }
          >
            Create one on github.com
          </button>
        </div>
      )}

      {source === "local" && !showFolders && folders.length === 0 && (
        <button className="collapse-bar" onClick={() => setShowFolders(true)}>
          <span>No folders indexed yet, add some to search locally</span>
          <svg className="chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}

      {manageFolders ? (
        <div className="card">
          <div className="card-head">
            <p className="card-title">Indexed folders</p>
            <button
              className="icon-btn"
              onClick={() => setShowFolders(false)}
              title="Collapse"
              aria-label="Collapse"
            >
              <svg className="chevron up" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          </div>
          <p className="card-body">
            Only folders you add are scanned. Hidden files and gitignored paths are skipped.
          </p>
          {folders.length > 0 && (
            <div className="folder-list">
              {folders.map((f) => (
                <div key={f.id} className="folder-row">
                  <span className="folder-path" title={f.path}>
                    {f.path}
                  </span>
                  <span className="folder-count">{f.file_count} files</span>
                  <button
                    className="folder-remove"
                    onClick={() => removeFolder(f.id)}
                    title="Remove from index"
                    aria-label={`Remove ${f.path}`}
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          )}
          <button className="primary-btn self-start" onClick={addFolder}>
            Add folder
          </button>
        </div>
      ) : (
        results.length > 0 && (
          <div className="results" ref={listRef}>
            {results.map((r, i) => (
              <div
                key={`${r.kind}-${r.id}`}
                data-idx={i}
                className={`row ${i === selected ? "selected" : ""}`}
                onMouseMove={() => setSelected(i)}
                onClick={() => openHit(r)}
              >
                {r.kind === "repo" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">
                        <span className="dim-prefix">{r.full_name.split("/")[0]}/</span>
                        {r.full_name.split("/")[1]}
                        {r.archived && <span className="badge">archived</span>}
                      </span>
                      {r.description && <span className="row-sub">{r.description}</span>}
                    </div>
                    <div className="row-meta">
                      {r.language && <span>{r.language}</span>}
                      <span className="stars">★ {formatStars(r.stars)}</span>
                    </div>
                  </>
                ) : (
                  <>
                    <div className="row-lead">
                      {r.thumb && (
                        <img
                          className="thumb"
                          src={`data:image/jpeg;base64,${r.thumb}`}
                          alt=""
                        />
                      )}
                      <div className="row-main">
                        <span className="row-title">{r.name}</span>
                        <span className="row-sub">{parentDir(r.path)}</span>
                      </div>
                    </div>
                    <div className="row-meta">
                      {r.ext && <span>{r.ext}</span>}
                      <span className="mono">{formatSize(r.size)}</span>
                      {imageQuery && (
                        <span className="sim">
                          {Math.max(0, Math.round(r.score * 100))}%
                        </span>
                      )}
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>
        )
      )}

      {!needsToken && !manageFolders && results.length === 0 && query.trim() !== "" && (
        <div className="empty">
          {source === "github-stars" ? "No matches in your stars" : "No matches in indexed folders"}
        </div>
      )}

      <div className="footer">
        <span className="hints">
          <span>
            <kbd>↑↓</kbd> navigate
          </span>
          <span>
            <kbd>⏎</kbd> open
          </span>
          <span>
            <kbd>tab</kbd> source
          </span>
          <span>
            <kbd>esc</kbd> hide
          </span>
        </span>
        <span className="status">{footerStatus}</span>
      </div>
    </div>
  );
}
