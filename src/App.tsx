import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { check as checkUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
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
  pushed_at: string | null;
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
  snippet: string | null;
}

interface BookmarkHit {
  kind: "bookmark";
  id: number;
  url: string;
  title: string;
  folder: string;
  browser: string;
  added_at: number | null;
  score: number;
}

interface HistoryHit {
  kind: "history";
  id: number;
  url: string;
  title: string;
  browser: string;
  visit_count: number;
  last_visit: number | null;
  score: number;
}

interface AppHit {
  kind: "app";
  name: string;
  target: string;
  score: number;
}

interface ClipHit {
  kind: "clip";
  id: number;
  content: string;
  first_copied: number;
  last_copied: number;
  copy_count: number;
  score: number;
}

type Hit = RepoHit | FileHit | BookmarkHit | HistoryHit | ClipHit | AppHit;

interface FolderInfo {
  id: number;
  path: string;
  file_count: number;
}

interface Status {
  repo_count: number;
  file_count: number;
  folder_count: number;
  bookmark_count: number;
  history_count: number;
  clip_count: number;
  clipboard_enabled: boolean;
  clip_retention_days: number;
  clip_max_entries: number;
  embedded_count: number;
  last_sync: string | null;
  username: string | null;
  has_token: boolean;
  model: string;
  image_model: string;
  syncing: boolean;
  local_indexing: boolean;
  max_file_mb: number;
  hotkey: string;
  hf_endpoint: string;
  version: string;
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

const ALL_SOURCES = [
  { id: "local", label: "Local Files" },
  { id: "github-stars", label: "GitHub Stars" },
  { id: "web", label: "Web" },
  { id: "clips", label: "Clipboard" },
] as const;
type SourceDef = (typeof ALL_SOURCES)[number];
const DEFAULT_ORDER = ALL_SOURCES.map((s) => s.id);

// Order the tabs by a saved id list, dropping unknowns and appending any
// canonical source the saved list is missing (e.g. after an app update adds one).
function orderedSources(saved: string[]): SourceDef[] {
  const byId = new Map<string, SourceDef>(ALL_SOURCES.map((s) => [s.id, s]));
  const seen = new Set<string>();
  const out: SourceDef[] = [];
  for (const id of saved) {
    const s = byId.get(id);
    if (s && !seen.has(id)) {
      out.push(s);
      seen.add(id);
    }
  }
  for (const s of ALL_SOURCES) if (!seen.has(s.id)) out.push(s);
  return out;
}

function loadTabOrder(): string[] {
  try {
    const raw = localStorage.getItem("magpie.taborder");
    if (raw) return orderedSources(JSON.parse(raw)).map((s) => s.id);
  } catch {
    /* fall through to default */
  }
  return DEFAULT_ORDER;
}

const IMAGE_EXT_RE = /\.(jpe?g|png|webp|bmp|gif)$/i;

interface ImageQuery {
  label: string;
  path?: string;
  bytesB64?: string;
  /// preview for the input-row chip (blob: URL or data: URL)
  thumbSrc?: string;
}

type RepoSort = "relevance" | "starred" | "stars";
const SORTS: { id: RepoSort; label: string }[] = [
  { id: "relevance", label: "match" },
  { id: "starred", label: "recent" },
  { id: "stars", label: "stars" },
];

type LocalScope = "all" | "text" | "images";
const SCOPES: { id: LocalScope; label: string }[] = [
  { id: "all", label: "all" },
  { id: "text", label: "text" },
  { id: "images", label: "images" },
];

type WebScope = "all" | "bookmarks" | "history";
const WEB_SCOPES: { id: WebScope; label: string }[] = [
  { id: "all", label: "all" },
  { id: "bookmarks", label: "bookmarks" },
  { id: "history", label: "history" },
];

type Theme = "auto" | "light" | "dark";
const THEMES: Theme[] = ["auto", "light", "dark"];

const HF_ENDPOINTS: { url: string; label: string }[] = [
  { url: "https://huggingface.co", label: "huggingface.co" },
  { url: "https://hf-mirror.com", label: "hf-mirror.com (China)" },
];

const FILE_CAPS: { mb: number; label: string }[] = [
  { mb: 4, label: "4 MB" },
  { mb: 16, label: "16 MB" },
  { mb: 64, label: "64 MB" },
  { mb: 0, label: "Unlimited" },
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

function relTime(iso: string | null): string | null {
  if (!iso) return null;
  return relTimeFromMs(Date.now() - Date.parse(iso));
}

function relTimeUnix(secs: number): string | null {
  if (!secs) return null;
  return relTimeFromMs(Date.now() - secs * 1000);
}

function relTimeFromMs(ms: number): string | null {
  if (Number.isNaN(ms) || ms < 0) return null;
  const days = ms / 86_400_000;
  if (days < 1) return "today";
  if (days < 30) return `${Math.floor(days)}d`;
  if (days < 365) return `${Math.floor(days / 30)}mo`;
  return `${Math.floor(days / 365)}y`;
}

function parentDir(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : path;
}

/// FTS snippet with \u0001..\u0002 sentinels around matches → <mark> nodes
function renderSnippet(s: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  s.split("\u0001").forEach((seg, i) => {
    if (i === 0) {
      nodes.push(seg);
      return;
    }
    const [hit, ...rest] = seg.split("\u0002");
    nodes.push(<mark key={i}>{hit}</mark>);
    nodes.push(rest.join("\u0002"));
  });
  return nodes;
}

// null = nothing worth showing (an idle pipeline must not render a hint)
function starsProgressLabel(p: StarsProgress): string | null {
  switch (p.stage) {
    case "listing":
      return `listing stars… ${p.total}`;
    case "readmes":
      return p.total === 0 ? null : `readmes ${p.done}/${p.total}`;
    case "embedding":
      return p.total === 0 ? null : `indexing stars ${p.done}/${p.total}`;
  }
}

function localProgressLabel(p: LocalProgress): string | null {
  if (p.stage === "scan") return `scanning files… ${p.done}`;
  if ((p.total ?? 0) === 0) return null;
  return p.stage === "embed-images"
    ? `indexing images ${p.done}/${p.total}`
    : `indexing files ${p.done}/${p.total}`;
}

export default function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Hit[]>([]);
  const [selected, setSelected] = useState(0);
  // shift+arrows extend a range from this anchor (clips only); null = single
  const [selAnchor, setSelAnchor] = useState<number | null>(null);
  // user-customizable tab order and which tab opens on launch
  const [sourceOrder, setSourceOrder] = useState<string[]>(loadTabOrder);
  const sources = useMemo(() => orderedSources(sourceOrder), [sourceOrder]);
  const sourcesRef = useRef(sources);
  sourcesRef.current = sources;
  const [defaultTab, setDefaultTab] = useState<string>(
    () => localStorage.getItem("magpie.defaulttab") || "local",
  );
  const [sourceIdx, setSourceIdx] = useState(() => {
    const order = loadTabOrder();
    const want = localStorage.getItem("magpie.defaulttab") || "local";
    const idx = order.indexOf(want);
    return idx >= 0 ? idx : 0;
  });
  const [status, setStatus] = useState<Status | null>(null);
  const [starsProgress, setStarsProgress] = useState<StarsProgress | null>(null);
  const [localProgress, setLocalProgress] = useState<LocalProgress | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  // auto-update: idle -> checking -> available -> downloading -> done|error
  const [updPhase, setUpdPhase] = useState<
    "idle" | "checking" | "none" | "available" | "downloading" | "error"
  >("idle");
  const [updVersion, setUpdVersion] = useState<string | null>(null);
  const [updPct, setUpdPct] = useState(0);
  const [updError, setUpdError] = useState<string | null>(null);
  const updRef = useRef<Update | null>(null);
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [showSettings, setShowSettings] = useState(!!import.meta.env.VITE_DEMO);
  const [tokenInput, setTokenInput] = useState("");
  const [tokenBusy, setTokenBusy] = useState(false);
  const [tokenError, setTokenError] = useState<string | null>(null);
  const [imageQuery, setImageQuery] = useState<ImageQuery | null>(null);
  const [repoSort, setRepoSort] = useState<RepoSort>(() => {
    const saved = localStorage.getItem("magpie.sort") as RepoSort | null;
    return saved && SORTS.some((s) => s.id === saved) ? saved : "relevance";
  });
  const [webScope, setWebScopeState] = useState<WebScope>(
    () => (localStorage.getItem("magpie.webscope") as WebScope) || "all",
  );
  const [localScope, setLocalScope] = useState<LocalScope>(() => {
    const saved = localStorage.getItem("magpie.scope") as LocalScope | null;
    return saved && SCOPES.some((s) => s.id === saved) ? saved : "all";
  });
  const [theme, setTheme] = useState<Theme>(() => {
    const saved = localStorage.getItem("magpie.theme") as Theme | null;
    return saved && THEMES.includes(saved) ? saved : "auto";
  });
  const [hotkeyDraft, setHotkeyDraft] = useState("");
  const [hotkeyMsg, setHotkeyMsg] = useState<string | null>(null);

  // theme: auto follows the system; light/dark force via data attribute
  useEffect(() => {
    if (theme === "auto") {
      delete document.documentElement.dataset.theme;
    } else {
      document.documentElement.dataset.theme = theme;
    }
    localStorage.setItem("magpie.theme", theme);
  }, [theme]);

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

  const source = (sources[sourceIdx] ?? sources[0]).id;
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
    } catch (e) {
      // surfaced in settings; a silent failure here looks like "no folders"
      setLastError(`folder list failed: ${String(e)}`);
    }
  }, []);

  // opening settings always shows fresh state
  useEffect(() => {
    if (showSettings) {
      refreshStatus();
      refreshFolders();
    }
  }, [showSettings, refreshStatus, refreshFolders]);

  const repoSortRef = useRef(repoSort);
  repoSortRef.current = repoSort;
  const localScopeRef = useRef(localScope);
  localScopeRef.current = localScope;
  const webScopeRef = useRef(webScope);
  webScopeRef.current = webScope;

  const setScope = useCallback((s: LocalScope) => {
    setLocalScope(s);
    localStorage.setItem("magpie.scope", s);
    inputRef.current?.focus();
  }, []);

  const setWebScope = useCallback((s: WebScope) => {
    setWebScopeState(s);
    localStorage.setItem("magpie.webscope", s);
    inputRef.current?.focus();
  }, []);

  const runSearch = useCallback(async (q: string, srcIdx: number) => {
    // empty input shows nothing: the palette stays a bare search box.
    // Clipboard is the exception — its whole point is "what did I just copy",
    // so an empty query lists the most recent clips.
    const srcId = (sourcesRef.current[srcIdx] ?? sourcesRef.current[0]).id;
    if (q.trim() === "" && srcId !== "clips") {
      setResults([]);
      setSelected(0);
      return;
    }
    try {
      let hits: Hit[];
      if (srcId === "github-stars") {
        const rs = await invoke<Omit<RepoHit, "kind">[]>("search_stars", {
          query: q,
          sort: repoSortRef.current,
        });
        hits = rs.map((r) => ({ ...r, kind: "repo" as const }));
      } else if (srcId === "web") {
        // backend already tags each hit's kind ("bookmark" | "history")
        hits = await invoke<Hit[]>("search_web", {
          query: q,
          scope: webScopeRef.current,
        });
      } else if (srcId === "clips") {
        const cs = await invoke<Omit<ClipHit, "kind">[]>("search_clips", { query: q });
        hits = cs.map((c) => ({ ...c, kind: "clip" as const }));
      } else {
        // local: matching apps surface as top hits, then files
        const [apps, fs] = await Promise.all([
          invoke<Omit<AppHit, "kind">[]>("search_apps", { query: q }),
          invoke<Omit<FileHit, "kind">[]>("search_local", {
            query: q,
            scope: localScopeRef.current,
          }),
        ]);
        hits = [
          ...apps.map((a) => ({ ...a, kind: "app" as const })),
          ...fs.map((f) => ({ ...f, kind: "file" as const })),
        ];
      }
      if (queryRef.current === q && sourceRef.current === srcIdx) {
        setResults(hits);
        setSelected(0);
        setSelAnchor(null);
      }
    } catch {
      /* transient: db busy during migration */
    }
  }, []);

  const acceptImageQuery = useCallback((iq: ImageQuery) => {
    setQuery("");
    setImageQuery(iq);
    setSourceIdx(0); // images live in the local source
    setShowSettings(false);
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
  }, [query, sourceIdx, imageQuery, repoSort, localScope, webScope, runSearch]);

  // release blob preview URLs when the image query changes or clears
  useEffect(() => {
    const src = imageQuery?.thumbSrc;
    return () => {
      if (src?.startsWith("blob:")) URL.revokeObjectURL(src);
    };
  }, [imageQuery]);

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
      acceptImageQuery({
        label: "pasted image",
        bytesB64: btoa(bin),
        thumbSrc: URL.createObjectURL(file),
      });
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [acceptImageQuery]);

  // backend events
  // suppress the native right-click menu everywhere (keyboard copy/paste
  // still works in inputs); a launcher has no use for the browser context menu
  useEffect(() => {
    const block = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", block);
    return () => document.removeEventListener("contextmenu", block);
  }, []);

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
      listen("bookmarks-done", () => {
        refreshStatus();
        runSearch(queryRef.current, sourceRef.current);
      }),
      listen<string>("bookmarks-error", (e) => setLastError(e.payload)),
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
      getCurrentWebview().onDragDropEvent(async (e) => {
        if (e.payload.type === "drop") {
          const p = e.payload.paths.find((x) => IMAGE_EXT_RE.test(x));
          if (p) {
            const thumb = await invoke<string | null>("preview_thumb", { path: p }).catch(
              () => null,
            );
            acceptImageQuery({
              label: p.split(/[\\/]/).pop() ?? "image",
              path: p,
              thumbSrc: thumb ? `data:image/jpeg;base64,${thumb}` : undefined,
            });
          }
        }
      }),
      // tray menu entry point
      listen("open-settings", () => setShowSettings(true)),
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
      } else if (hit.kind === "bookmark" || hit.kind === "history") {
        await invoke("open_repo", { url: hit.url });
      } else if (hit.kind === "app") {
        await invoke("launch_app", { target: hit.target });
      } else if (hit.kind === "clip") {
        await invoke("copy_clip", { text: hit.content });
      } else {
        await invoke("open_file", { path: hit.path });
      }
      await getCurrentWindow().hide();
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  // Ctrl/Cmd+Enter hands the raw query to the default browser: a URL-looking
  // input opens directly, anything else becomes a web search
  const openWeb = useCallback(async () => {
    const q = queryRef.current.trim();
    if (!q) return;
    const hasProto = /^https?:\/\//i.test(q);
    const urlish = /^[\w-]+(\.[\w-]+)+(:\d+)?(\/\S*)?$/i.test(q);
    const url = hasProto
      ? q
      : urlish
        ? `https://${q}`
        : `https://www.google.com/search?q=${encodeURIComponent(q)}`;
    try {
      await invoke("open_repo", { url });
      await getCurrentWindow().hide();
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const switchSource = useCallback((idx: number) => {
    setSourceIdx(idx);
    setSelected(0);
    setSelAnchor(null);
    setShowSettings(false);
    inputRef.current?.focus();
  }, []);

  // move a tab one slot left/right; keeps the active tab selected by id
  const moveTab = useCallback(
    (id: string, dir: -1 | 1) => {
      setSourceOrder((prev) => {
        const order: string[] = orderedSources(prev).map((s) => s.id);
        const i = order.indexOf(id);
        const j = i + dir;
        if (i < 0 || j < 0 || j >= order.length) return prev;
        [order[i], order[j]] = [order[j], order[i]];
        localStorage.setItem("magpie.taborder", JSON.stringify(order));
        // keep the currently active source visually selected after a reorder
        const activeId = (sourcesRef.current[sourceIdx] ?? sourcesRef.current[0]).id;
        const nextIdx = order.indexOf(activeId);
        if (nextIdx >= 0) setSourceIdx(nextIdx);
        return order;
      });
    },
    [sourceIdx],
  );

  const chooseDefaultTab = useCallback((id: string) => {
    setDefaultTab(id);
    localStorage.setItem("magpie.defaulttab", id);
  }, []);

  // drag-to-reorder tabs via pointer events. HTML5 drag&drop is NOT usable
  // here: Tauri's file drag-drop handling (needed for image drops) swallows
  // the webview's native DnD drop events on Windows.
  const [dragTab, setDragTab] = useState<string | null>(null);

  const commitDrag = useCallback(
    (fromId: string, toId: string) => {
      if (fromId === toId) return;
      setSourceOrder((prev) => {
        const order: string[] = orderedSources(prev).map((s) => s.id);
        const from = order.indexOf(fromId);
        const to = order.indexOf(toId);
        if (from < 0 || to < 0) return prev;
        order.splice(to, 0, order.splice(from, 1)[0]);
        localStorage.setItem("magpie.taborder", JSON.stringify(order));
        const activeId = (sourcesRef.current[sourceIdx] ?? sourcesRef.current[0]).id;
        const nextIdx = order.indexOf(activeId);
        if (nextIdx >= 0) setSourceIdx(nextIdx);
        return order;
      });
    },
    [sourceIdx],
  );

  // a drag ends on any pointer release, wherever it happens
  useEffect(() => {
    if (!dragTab) return;
    const end = () => setDragTab(null);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [dragTab]);

  const selLo = selAnchor == null ? selected : Math.min(selAnchor, selected);
  const selHi = selAnchor == null ? selected : Math.max(selAnchor, selected);

  const deleteSelectedClips = useCallback(async () => {
    const range = results.slice(selLo, selHi + 1).filter((r) => r.kind === "clip");
    if (range.length === 0) return;
    try {
      for (const r of range) {
        await invoke("delete_clip", { clipId: r.id });
      }
      setResults((rs) => rs.filter((_, i) => i < selLo || i > selHi));
      setSelected(Math.max(0, Math.min(selLo, results.length - range.length - 1)));
      setSelAnchor(null);
      refreshStatus();
    } catch (e) {
      setLastError(String(e));
    }
  }, [results, selLo, selHi, refreshStatus]);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const max = results.length - 1;
      const nav = ["ArrowDown", "ArrowUp", "PageDown", "PageUp"].includes(e.key);
      if (nav && max < 0) {
        e.preventDefault();
        return;
      }
      const extend = e.shiftKey && source === "clips";
      const move = (next: (s: number) => number) => {
        if (extend) {
          setSelAnchor((a) => (a == null ? selected : a));
        } else {
          setSelAnchor(null);
        }
        setSelected(next);
      };
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          move((s) => Math.min(s + 1, max));
          break;
        case "ArrowUp":
          e.preventDefault();
          move((s) => Math.max(s - 1, 0));
          break;
        case "PageDown":
          e.preventDefault();
          move((s) => Math.min(s + PAGE, max));
          break;
        case "PageUp":
          e.preventDefault();
          move((s) => Math.max(s - PAGE, 0));
          break;
        case "Delete":
          if ((e.ctrlKey || e.metaKey) && source === "clips" && max >= 0) {
            e.preventDefault();
            void deleteSelectedClips();
          }
          break;
        case "Enter":
          e.preventDefault();
          if (e.ctrlKey || e.metaKey) {
            openWeb();
          } else if (source === "clips" && selAnchor != null && selHi > selLo) {
            // multi-select: copy every selected clip, list order, one per line
            const joined = results
              .slice(selLo, selHi + 1)
              .filter((r) => r.kind === "clip")
              .map((r) => (r as ClipHit).content)
              .join("\n");
            void invoke("copy_clip", { text: joined }).then(() => getCurrentWindow().hide());
          } else {
            openHit(results[selected]);
          }
          break;
        case "Escape":
          e.preventDefault();
          if (imageQuery) {
            setImageQuery(null); // first Esc clears the image query
          } else if (showSettings) {
            setShowSettings(false); // then close settings, then hide
          } else {
            getCurrentWindow().hide();
          }
          break;
        case "Tab":
          e.preventDefault();
          if (showSettings) break;
          if (e.shiftKey) {
            // Shift+Tab cycles the right-hand mode of the active source:
            // local scope, or star sort order
            if (source === "local") {
              const i = SCOPES.findIndex((s) => s.id === localScope);
              setScope(SCOPES[(i + 1) % SCOPES.length].id);
            } else if (source === "web") {
              const i = WEB_SCOPES.findIndex((s) => s.id === webScope);
              setWebScope(WEB_SCOPES[(i + 1) % WEB_SCOPES.length].id);
            } else if (source === "github-stars") {
              const i = SORTS.findIndex((s) => s.id === repoSort);
              const next = SORTS[(i + 1) % SORTS.length].id;
              setRepoSort(next);
              localStorage.setItem("magpie.sort", next);
            }
          } else {
            switchSource((sourceIdx + 1) % sources.length);
          }
          break;
      }
    },
    [results, selected, selAnchor, selLo, selHi, sourceIdx, sources, imageQuery, showSettings, source, localScope, webScope, repoSort, openHit, openWeb, switchSource, setScope, setWebScope, deleteSelectedClips],
  );

  const refresh = useCallback(async () => {
    setLastError(null);
    try {
      const cmd =
        source === "github-stars"
          ? "start_sync"
          : source === "web"
            ? "sync_bookmarks_now"
            : "index_local";
      await invoke(cmd);
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

  const pickQueryImage = useCallback(async () => {
    const file = await openDialog({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"] }],
    });
    if (typeof file !== "string") return;
    const thumb = await invoke<string | null>("preview_thumb", { path: file }).catch(() => null);
    acceptImageQuery({
      label: file.split(/[\\/]/).pop() ?? "image",
      path: file,
      thumbSrc: thumb ? `data:image/jpeg;base64,${thumb}` : undefined,
    });
  }, [acceptImageQuery]);

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

  const captureHotkey = useCallback((e: React.KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
    // bare Backspace/Delete/Escape clears the recording
    if (
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      ["Backspace", "Delete", "Escape"].includes(e.key)
    ) {
      setHotkeyDraft("");
      setHotkeyMsg(null);
      return;
    }
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    let key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
    if (e.key === " ") key = "Space";
    // Shift-only or bare chords (Shift+A, Tab…) would hijack normal typing
    // system-wide; require Ctrl/Alt/Super for anything that is not F1-F24
    const strongModifier = e.ctrlKey || e.altKey || e.metaKey;
    if (!strongModifier && !/^F([1-9]|1[0-9]|2[0-4])$/.test(key)) {
      setHotkeyMsg("use Ctrl/Alt/Win plus a key, or an F-key");
      return;
    }
    parts.push(key);
    setHotkeyDraft(parts.join("+"));
    setHotkeyMsg(null);
  }, []);

  const applyHotkey = useCallback(async () => {
    if (!hotkeyDraft) return;
    try {
      await invoke("set_hotkey", { hotkey: hotkeyDraft });
      setHotkeyMsg("saved");
      setHotkeyDraft("");
      refreshStatus();
    } catch (e) {
      setHotkeyMsg(String(e));
    }
  }, [hotkeyDraft, refreshStatus]);

  const applyFileCap = useCallback(
    async (mb: number) => {
      try {
        await invoke("set_max_file_mb", { mb });
        await refreshStatus();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshStatus],
  );

  const removeFolder = useCallback(async (id: number) => {
    try {
      setFolders(await invoke<FolderInfo[]>("remove_folder", { folderId: id }));
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const rebuildFolder = useCallback(
    async (id: number) => {
      setLastError(null);
      try {
        await invoke("rebuild_folder", { folderId: id });
        await refreshFolders();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [refreshFolders],
  );

  const doCheckUpdate = useCallback(async (silent: boolean) => {
    setUpdError(null);
    if (!silent) setUpdPhase("checking");
    try {
      const u = await checkUpdate();
      if (u) {
        updRef.current = u;
        setUpdVersion(u.version);
        setUpdPhase("available");
      } else if (!silent) {
        setUpdPhase("none");
      }
    } catch (e) {
      if (!silent) {
        setUpdPhase("error");
        setUpdError(String(e));
      }
    }
  }, []);

  const doInstallUpdate = useCallback(async () => {
    const u = updRef.current;
    if (!u) return;
    setUpdPhase("downloading");
    setUpdPct(0);
    try {
      let total = 0;
      let done = 0;
      await u.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? 0;
        } else if (ev.event === "Progress") {
          done += ev.data.chunkLength;
          if (total > 0) setUpdPct(Math.min(100, Math.round((done * 100) / total)));
        } else if (ev.event === "Finished") {
          setUpdPct(100);
        }
      });
      await relaunch();
    } catch (e) {
      setUpdPhase("error");
      setUpdError(String(e));
    }
  }, []);

  // quiet startup check, delayed so it never competes with model init
  useEffect(() => {
    const t = setTimeout(() => doCheckUpdate(true), 15_000);
    return () => clearTimeout(t);
  }, [doCheckUpdate]);

  const rebuildStars = useCallback(async () => {
    setLastError(null);
    try {
      await invoke("rebuild_stars");
      await refreshStatus();
    } catch (e) {
      setLastError(String(e));
    }
  }, [refreshStatus]);

  const busy =
    source === "github-stars"
      ? starsProgress !== null || (status?.syncing ?? false)
      : localProgress !== null || (status?.local_indexing ?? false);
  const modelWarming = status !== null && status.model === "loading";
  const modelFailed = status !== null && status.model.startsWith("failed");

  const footerStatus = lastError
    ? `error: ${lastError}`
    : modelFailed
      ? "model download failed, keyword search only (set a mirror in settings)"
      : modelWarming
        ? "preparing semantic model (first run downloads ~500 MB)"
        : source === "local" && status?.image_model === "loading"
          ? "preparing image model (first run downloads ~200 MB)"
          : status
            ? source === "github-stars"
              ? `${status.repo_count} repos indexed`
              : source === "web"
                ? `${status.bookmark_count} bookmarks · ${status.history_count} history`
                : source === "clips"
                  ? status.clipboard_enabled
                    ? `${status.clip_count} clips recorded`
                    : "clipboard history is off — enable it in settings"
                  : `${status.file_count} files indexed`
            : "";

  // progress is scoped to the active source: stars sync details only show on
  // the GitHub Stars tab, local indexing only on Local Files
  const indexHint =
    source === "github-stars"
      ? starsProgress
        ? starsProgressLabel(starsProgress)
        : null
      : localProgress
        ? localProgressLabel(localProgress)
        : null;

  return (
    <div
      className={`panel ${showSettings ? "settings-mode" : ""}`}
      ref={panelRef}
      onKeyDown={onKeyDown}
    >
      <div className="source-row" data-tauri-drag-region>
        {sources.map((s, i) => (
          <button
            key={s.id}
            className={`source ${i === sourceIdx ? "active" : ""}`}
            onClick={() => switchSource(i)}
            tabIndex={-1}
          >
            {s.label}
          </button>
        ))}
        {source === "local" && (
          <span className="sort-group">
            {SCOPES.map((s) => (
              <button
                key={s.id}
                className={`source ${localScope === s.id ? "active" : ""}`}
                onClick={() => setScope(s.id)}
                tabIndex={-1}
                title={`Search ${s.id === "all" ? "everything" : s.id} (Shift+Tab cycles)`}
              >
                {s.label}
              </button>
            ))}
          </span>
        )}
        {source === "web" && (
          <span className="sort-group">
            {WEB_SCOPES.map((s) => (
              <button
                key={s.id}
                className={`source ${webScope === s.id ? "active" : ""}`}
                onClick={() => setWebScope(s.id)}
                tabIndex={-1}
                title={`Search ${s.id} (Shift+Tab cycles)`}
              >
                {s.label}
              </button>
            ))}
          </span>
        )}
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
            {imageQuery.thumbSrc ? (
              <img className="chip-thumb" src={imageQuery.thumbSrc} alt="" />
            ) : (
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
                <circle cx="5.5" cy="6.5" r="1.25" fill="currentColor" />
                <path d="M2.5 12l3.5-3.5 2.5 2.5 3-3 2 2" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
              </svg>
            )}
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
              : source === "web"
              ? "Search bookmarks and browser history"
              : source === "clips"
              ? "Search your clipboard history"
              : source === "github-stars"
                ? status && status.repo_count > 0
                  ? `Search ${status.repo_count} starred repos`
                  : "Search your stars"
                : localScope === "images"
                  ? "Describe the image, or pick / drop / paste one"
                  : status && status.file_count > 0
                    ? `Search ${status.file_count} local files, drop or paste an image`
                    : "Search indexed folders"
          }
          autoFocus
          spellCheck={false}
          autoCorrect="off"
          autoCapitalize="off"
        />
        {source === "local" && !imageQuery && (
          <button
            className="icon-btn"
            onClick={pickQueryImage}
            title="Search with an image file"
            aria-label="Pick a query image"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
              <circle cx="5.5" cy="6.5" r="1.25" fill="currentColor" />
              <path d="M2.5 12l3.5-3.5 2.5 2.5 3-3 2 2" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
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

      {indexHint && <div className="index-hint">{indexHint}</div>}

      {needsToken && !showSettings && (
        <button className="collapse-bar" onClick={() => setShowSettings(true)}>
          <span>Connect GitHub to sync your stars</span>
          <svg className="chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}

      {source === "local" && !showSettings && folders.length === 0 && (
        <button className="collapse-bar" onClick={() => setShowSettings(true)}>
          <span>No folders indexed yet, add some to search locally</span>
          <svg className="chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}

      {showSettings ? (
        <div className="card settings-page">
          <div className="card-head" data-tauri-drag-region>
            <p className="card-title settings-title">
              Settings
              {status?.version && <span className="ver">v{status.version}</span>}
            </p>
            <button
              className="icon-btn"
              onClick={() => setShowSettings(false)}
              title="Back to search (Esc)"
              aria-label="Close settings"
            >
              ✕
            </button>
          </div>

          {/* CONNECTION */}
          <p className="set-eyebrow">Connection</p>
          <div className="set-group">
            <div className="set-row stack">
              <div className="set-head">
                <div className="set-label">
                  <span className="set-name">GitHub</span>
                  <span className="set-desc">
                    {status?.has_token
                      ? "Paste a new token to replace the current one."
                      : "A personal access token, no scopes needed — it only reads your public stars."}
                  </span>
                </div>
                {status?.has_token && status.username ? (
                  <span className="conn-badge ok">
                    <span className="conn-dot" aria-hidden="true" /> {status.username}
                  </span>
                ) : (
                  <span className="conn-badge">not connected</span>
                )}
              </div>
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
              <div className="set-links">
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
                {status?.has_token && (
                  <button
                    className="link-btn"
                    onClick={rebuildStars}
                    title="Wipe the star index and sync everything from scratch"
                  >
                    Rebuild star index
                  </button>
                )}
              </div>
            </div>
          </div>

          {/* INDEXING */}
          <p className="set-eyebrow">Indexing</p>
          <div className="set-group">
            <div className="set-row stack">
              <div className="set-head">
                <div className="set-label">
                  <span className="set-name">
                    Indexed folders
                    {status != null && status.folder_count > 0 && (
                      <span className="count-pill">{status.folder_count}</span>
                    )}
                  </span>
                  <span className="set-desc">
                    Scanned recursively; hidden and gitignored paths are skipped.
                  </span>
                </div>
                <button className="primary-btn" onClick={addFolder}>
                  Add folder
                </button>
              </div>
              {folders.length === 0 &&
                (status != null && status.folder_count > 0 ? (
                  <p className="error-line">
                    {status.folder_count} folder(s) indexed but the list failed to load —
                    please report this with the error below.
                  </p>
                ) : (
                  <p className="set-empty">No folders yet.</p>
                ))}
              {folders.length > 0 && (
                <div className="folder-list">
                  {folders.map((f) => (
                    <div key={f.id} className="folder-row">
                      <span className="folder-path" title={f.path}>
                        {f.path}
                      </span>
                      <span className="folder-count">{f.file_count}</span>
                      <button
                        className="folder-remove"
                        onClick={() => rebuildFolder(f.id)}
                        title="Rebuild this folder's index from scratch"
                        aria-label={`Rebuild index for ${f.path}`}
                      >
                        ↻
                      </button>
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
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">Max file size</span>
                <span className="set-desc">Larger files index by name only. Changing rebuilds.</span>
              </div>
              <div className="pill-row">
                {FILE_CAPS.map((c) => (
                  <button
                    key={c.mb}
                    className={`source ${status?.max_file_mb === c.mb ? "active" : ""}`}
                    onClick={() => applyFileCap(c.mb)}
                  >
                    {c.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row stack">
              <div className="set-head">
                <div className="set-label">
                  <span className="set-name">Model download source</span>
                  <span className="set-desc">
                    Pick the mirror if huggingface.co is unreachable from your network.
                  </span>
                </div>
                <div className="pill-row">
                  {HF_ENDPOINTS.map((e) => (
                    <button
                      key={e.url}
                      className={`source ${status?.hf_endpoint === e.url ? "active" : ""}`}
                      onClick={async () => {
                        try {
                          await invoke("set_hf_endpoint", { endpoint: e.url });
                          await refreshStatus();
                        } catch (er) {
                          setLastError(String(er));
                        }
                      }}
                    >
                      {e.label}
                    </button>
                  ))}
                </div>
              </div>
              <div className="model-status">
                <span>
                  <span className={`status-dot ${status?.model === "ready" ? "ok" : ""}`} />
                  Semantic model —{" "}
                  {status?.model === "ready"
                    ? "ready"
                    : status?.model === "loading"
                      ? "downloading (~500 MB, first run)…"
                      : (status?.model ?? "…")}
                </span>
                <span>
                  <span className={`status-dot ${status?.image_model === "ready" ? "ok" : ""}`} />
                  Image model —{" "}
                  {status?.image_model === "ready"
                    ? "ready"
                    : status?.image_model === "loading"
                      ? "downloading (~200 MB, first run)…"
                      : (status?.image_model ?? "…")}
                </span>
              </div>
            </div>
          </div>

          {/* APPEARANCE & BEHAVIOR */}
          <p className="set-eyebrow">Appearance &amp; behavior</p>
          <div className="set-group">
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">Theme</span>
              </div>
              <div className="pill-row">
                {THEMES.map((t) => (
                  <button
                    key={t}
                    className={`source ${theme === t ? "active" : ""}`}
                    onClick={() => setTheme(t)}
                  >
                    {t}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">Summon shortcut</span>
                <span className="set-desc">
                  Currently <kbd>{status?.hotkey ?? "Alt+Space"}</kbd>. Click and press a new
                  combination; Backspace clears. OS-reserved chords (like ⌘Space) can't be captured.
                </span>
              </div>
              <div className="token-row">
                <input
                  className="token-input"
                  value={hotkeyDraft}
                  onChange={() => {}}
                  onKeyDown={captureHotkey}
                  placeholder="press keys…"
                  spellCheck={false}
                />
                {hotkeyDraft && (
                  <button
                    className="icon-btn"
                    onClick={() => {
                      setHotkeyDraft("");
                      setHotkeyMsg(null);
                    }}
                    title="Clear"
                    aria-label="Clear recorded shortcut"
                  >
                    ✕
                  </button>
                )}
                <button className="primary-btn" onClick={applyHotkey} disabled={!hotkeyDraft}>
                  Apply
                </button>
              </div>
              {hotkeyMsg && (
                <p className={hotkeyMsg === "saved" ? "set-empty" : "error-line"}>{hotkeyMsg}</p>
              )}
              {status?.hotkey !== "Alt+Space" && (
                <div className="set-links">
                  <button
                    className="link-btn"
                    onClick={async () => {
                      try {
                        await invoke("set_hotkey", { hotkey: "Alt+Space" });
                        setHotkeyDraft("");
                        setHotkeyMsg("saved");
                        refreshStatus();
                      } catch (e) {
                        setHotkeyMsg(String(e));
                      }
                    }}
                  >
                    Reset to Alt+Space
                  </button>
                </div>
              )}
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">Tabs</span>
                <span className="set-desc">
                  Drag the handle (or use the arrows) to reorder; ★ marks the
                  tab that opens on launch.
                </span>
              </div>
              <div className="tab-order">
                {sources.map((s, i) => (
                  <div
                    key={s.id}
                    className={`tab-row ${dragTab === s.id ? "dragging" : ""}`}
                    onPointerEnter={() => {
                      // live reorder: while a drag is held, entering another
                      // row moves the dragged tab into that slot
                      if (dragTab && dragTab !== s.id) commitDrag(dragTab, s.id);
                    }}
                  >
                    <span
                      className="drag-handle"
                      aria-hidden="true"
                      onPointerDown={(e) => {
                        e.preventDefault();
                        setDragTab(s.id);
                      }}
                    >
                      ⠿
                    </span>
                    <button
                      className={`star-btn ${defaultTab === s.id ? "on" : ""}`}
                      onClick={() => chooseDefaultTab(s.id)}
                      title={defaultTab === s.id ? "Opens on launch" : "Make this the launch tab"}
                      aria-label={`Make ${s.label} the default tab`}
                    >
                      {defaultTab === s.id ? "★" : "☆"}
                    </button>
                    <span className="tab-name">{s.label}</span>
                    <button
                      className="tab-move"
                      onClick={() => moveTab(s.id, -1)}
                      disabled={i === 0}
                      aria-label={`Move ${s.label} up`}
                    >
                      ↑
                    </button>
                    <button
                      className="tab-move"
                      onClick={() => moveTab(s.id, 1)}
                      disabled={i === sources.length - 1}
                      aria-label={`Move ${s.label} down`}
                    >
                      ↓
                    </button>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* PRIVACY */}
          <p className="set-eyebrow">Privacy</p>
          <div className="set-group">
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">
                  Clipboard history
                  {status?.clipboard_enabled && (
                    <span className="count-pill">{status.clip_count}</span>
                  )}
                </span>
                <span className="set-desc">
                  Recorded locally, searchable in the Clipboard tab. Password-manager
                  secrets are never stored.
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", enabled: false },
                  { label: "on", enabled: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${status?.clipboard_enabled === o.enabled ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        await invoke("set_clipboard_enabled", { enabled: o.enabled });
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {o.label}
                  </button>
                ))}
              </div>
            </div>

            {status?.clipboard_enabled && (
              <>
                <div className="set-row">
                  <div className="set-label">
                    <span className="set-name">Keep at most</span>
                  </div>
                  <div className="pill-row">
                    {[
                      { label: "500", entries: 500 },
                      { label: "2000", entries: 2000 },
                      { label: "unlimited", entries: 0 },
                    ].map((o) => (
                      <button
                        key={`n${o.entries}`}
                        className={`source ${status?.clip_max_entries === o.entries ? "active" : ""}`}
                        onClick={async () => {
                          try {
                            await invoke("set_clip_max_entries", { entries: o.entries });
                            await refreshStatus();
                          } catch (e) {
                            setLastError(String(e));
                          }
                        }}
                      >
                        {o.label}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div className="set-label">
                    <span className="set-name">Keep for</span>
                  </div>
                  <div className="pill-row">
                    {[
                      { label: "7 days", days: 7 },
                      { label: "30 days", days: 30 },
                      { label: "forever", days: 0 },
                    ].map((o) => (
                      <button
                        key={o.days}
                        className={`source ${status?.clip_retention_days === o.days ? "active" : ""}`}
                        onClick={async () => {
                          try {
                            await invoke("set_clip_retention", { days: o.days });
                            await refreshStatus();
                          } catch (e) {
                            setLastError(String(e));
                          }
                        }}
                      >
                        {o.label}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div className="set-label">
                    <span className="set-name">Clear history</span>
                    <span className="set-desc">Delete every recorded clip permanently.</span>
                  </div>
                  <button
                    className="danger-btn"
                    onClick={async () => {
                      try {
                        await invoke("clear_clips_now");
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    Clear
                  </button>
                </div>
              </>
            )}
          </div>

          {/* SYSTEM */}
          <p className="set-eyebrow">System</p>
          <div className="set-group">
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">Updates</span>
                <span className="set-desc">
                  {updPhase === "available" || updPhase === "downloading"
                    ? `Version ${updVersion} is available.`
                    : updPhase === "none"
                      ? "You are on the latest version."
                      : "Installed in place; your index and settings are kept."}
                </span>
              </div>
              {updPhase === "available" ? (
                <button className="primary-btn" onClick={doInstallUpdate}>
                  Update &amp; restart
                </button>
              ) : updPhase === "downloading" ? (
                <button className="primary-btn" disabled>
                  {updPct}%
                </button>
              ) : (
                <button
                  className="ghost-btn"
                  onClick={() => doCheckUpdate(false)}
                  disabled={updPhase === "checking"}
                >
                  {updPhase === "checking" ? "Checking…" : "Check now"}
                </button>
              )}
            </div>
            {updError && <p className="error-line">{updError}</p>}
          </div>

          {lastError && <p className="error-line">{lastError}</p>}
        </div>
      ) : (
        results.length > 0 && (
          <div className="results" ref={listRef}>
            {results.map((r, i) => (
              <div
                key={`${r.kind}-${r.kind === "app" ? r.target : r.id}`}
                data-idx={i}
                className={`row ${i >= selLo && i <= selHi ? "selected" : ""}`}
                onMouseMove={() => {
                  if (selAnchor == null) setSelected(i);
                }}
                onClick={() => openHit(r)}
              >
                {r.kind === "clip" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title clip-text" title={r.content}>
                        {r.content.split("\n")[0].slice(0, 200)}
                      </span>
                      {r.content.includes("\n") && (
                        <span className="row-sub">
                          {r.content.split("\n").filter((l) => l.trim()).length} lines
                        </span>
                      )}
                    </div>
                    <div className="row-meta">
                      {relTimeUnix(r.last_copied) && (
                        <span className="mono">{relTimeUnix(r.last_copied)}</span>
                      )}
                      {r.copy_count > 1 && <span>×{r.copy_count}</span>}
                    </div>
                  </>
                ) : r.kind === "app" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">{r.name}</span>
                      <span className="row-sub">Application</span>
                    </div>
                    <div className="row-meta">
                      <span className="app-badge">App</span>
                    </div>
                  </>
                ) : r.kind === "bookmark" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">{r.title}</span>
                      <span className="row-sub">
                        {r.folder && <span className="dim-prefix">{r.folder} · </span>}
                        {r.url}
                      </span>
                    </div>
                    <div className="row-meta">
                      {webScope === "all" && (
                        <span className="web-badge bookmark">Bookmark</span>
                      )}
                      {r.added_at != null && relTimeUnix(r.added_at) && (
                        <span
                          className="mono"
                          title={`added ${new Date(r.added_at * 1000).toISOString().slice(0, 10)}`}
                        >
                          {relTimeUnix(r.added_at)}
                        </span>
                      )}
                      <span>{r.browser}</span>
                    </div>
                  </>
                ) : r.kind === "history" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">{r.title || r.url}</span>
                      <span className="row-sub">{r.url}</span>
                    </div>
                    <div className="row-meta">
                      {webScope === "all" && (
                        <span className="web-badge history">History</span>
                      )}
                      {r.last_visit != null && relTimeUnix(r.last_visit) && (
                        <span className="mono">{relTimeUnix(r.last_visit)}</span>
                      )}
                      <span>{r.visit_count}×</span>
                    </div>
                  </>
                ) : r.kind === "repo" ? (
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
                      {relTime(r.pushed_at) && (
                        <span
                          className="mono"
                          title={`last push ${r.pushed_at?.slice(0, 10)}`}
                        >
                          {relTime(r.pushed_at)}
                        </span>
                      )}
                      {r.language && <span>{r.language}</span>}
                      <span className="stars">★ {formatStars(r.stars)}</span>
                    </div>
                  </>
                ) : (
                  <>
                    <div className="row-main">
                      <span className="row-title">{r.name}</span>
                      <span className="row-sub">
                        {r.snippet ? renderSnippet(r.snippet) : parentDir(r.path)}
                      </span>
                    </div>
                    <div className="row-meta">
                      {relTimeUnix(r.mtime) && (
                        <span
                          className="mono"
                          title={`modified ${new Date(r.mtime * 1000).toISOString().slice(0, 10)}`}
                        >
                          {relTimeUnix(r.mtime)}
                        </span>
                      )}
                      {r.ext && <span>{r.ext}</span>}
                      <span className="mono">{formatSize(r.size)}</span>
                      {imageQuery && (
                        <span className="sim">
                          {Math.max(0, Math.round(r.score * 100))}%
                        </span>
                      )}
                      {r.thumb && (
                        <img
                          className="thumb"
                          src={`data:image/jpeg;base64,${r.thumb}`}
                          alt=""
                        />
                      )}
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>
        )
      )}

      {!needsToken && !showSettings && results.length === 0 && query.trim() !== "" && (
        <div className="empty">
          {source === "github-stars"
            ? "No matches in your stars"
            : source === "web"
              ? "No matching bookmarks or history"
              : source === "clips"
                ? "No matching clips"
                : "No matches in indexed folders"}
        </div>
      )}

      <div className="footer">
        <span className="hints">
          <span>
            <kbd>↑↓</kbd> navigate
          </span>
          <span>
            <kbd>⏎</kbd> {source === "clips" ? "copy" : "open"}
          </span>
          <span>
            <kbd>tab</kbd> source
          </span>
          {source === "clips" && (
            <>
              <span>
                <kbd>⇧↑↓</kbd> select
              </span>
              <span>
                <kbd>ctrl⌦</kbd> delete
              </span>
            </>
          )}
          {(source === "local" || source === "web" || source === "github-stars") && (
            <span>
              <kbd>⇧tab</kbd> {source === "github-stars" ? "sort" : "scope"}
            </span>
          )}
          <span>
            <kbd>ctrl⏎</kbd> web
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
