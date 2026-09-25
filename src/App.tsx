import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { check as checkUpdate, type Update } from "@tauri-apps/plugin-updater";
import {
  BANGS_KEY,
  DEFAULT_BANGS,
  TIPS_KEY,
  loadBangs,
  matchBang,
  nextTip,
  randomTip,
  searchEmoji,
  tipsEnabled,
  type BangMatch,
  type EmojiHit,
  matchNote,
  type NoteMatch,
  recentsEnabled,
  setRecentsEnabled,
} from "./extras";
import { loadLangPref, resolveLang, setLang, t, tf, type LangPref } from "./i18n";
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
  clip_kind: "text" | "image";
  thumb: string | null;
  width: number | null;
  height: number | null;
  pinned: boolean;
  score: number;
}

interface VideoHit {
  kind: "video";
  id: number;
  shot_id: number;
  path: string;
  name: string;
  start_ms: number;
  end_ms: number;
  ts_ms: number;
  thumb: string | null;
  duration_ms: number;
  score: number;
}

/// The calculator / transform row above the results.
interface CalcHit {
  value: string;
  alt: string | null;
  swatch?: string | null;
  /// the value explains why the input did not work; Enter copies nothing
  error?: boolean;
  /// a PNG (base64) to show and copy instead of text, e.g. a QR code
  image?: string | null;
}

/// A system command (lock, sleep, restart, …) offered next to apps.
interface CommandHit {
  kind: "command";
  id: string;
  destructive: boolean;
  score: number;
}

/// A running process, listed by `kill <name>`.
interface ProcessHit {
  kind: "process";
  pid: number;
  name: string;
  memory: number;
  exe: string | null;
}

type Hit =
  | RepoHit
  | FileHit
  | BookmarkHit
  | HistoryHit
  | ClipHit
  | AppHit
  | VideoHit
  | CommandHit
  | ProcessHit;

/// A stable identity per row, for React keys and for the "press Enter
/// again" confirmation.
function hitKey(r: Hit): string {
  switch (r.kind) {
    case "app":
      return `app-${r.target}`;
    case "command":
      return `command-${r.id}`;
    case "process":
      return `process-${r.pid}`;
    default:
      return `${r.kind}-${r.id}`;
  }
}

/// `kill chrome` / `结束 chrome` → "chrome"; anything else → null.
function matchKill(q: string): string | null {
  const m = /^(?:kill|结束)\s+(.+)$/i.exec(q.trim());
  return m ? m[1].trim() : null;
}

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
  app_aliases: string;
  video_count: number;
  video_shot_count: number;
  video_indexing_enabled: boolean;
  /// prune linked git worktrees whose main checkout is indexed (default on)
  skip_worktrees: boolean;
  video_indexing: boolean;
  video_note: string;
  ffmpeg_status: string;
  video_decode_threads: number;
  video_hwaccel: boolean;
  index_threads: number;
  cpu_cores: number;
  mcp_enabled: boolean;
  mcp_status: string;
  mcp_url: string;
  mcp_command: string;
  watch_enabled: boolean;
  watch_status: string;
  watched_folders: number;
  rescan_minutes: number;
  embedded_count: number;
  last_sync: string | null;
  username: string | null;
  has_token: boolean;
  model: string;
  image_model: string;
  ocr_enabled: boolean;
  ocr_model: string;
  ocr_status: string;
  ocr_pdf: boolean;
  syncing: boolean;
  local_indexing: boolean;
  max_file_mb: number;
  hotkey: string;
  /// the effective selection-search chord; empty when the user removed it
  hotkey_selection: string;
  hotkey_selection_default: string;
  note_path: string;
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
  stage: "scan" | "embed" | "embed-images" | "videos";
  done: number;
  total?: number;
}

const PAGE = 8;
const WINDOW_WIDTH = 720;
// the palette keeps its own width and the preview pane is added beside it, so
// the tab strip and the query row never reflow when the pane opens
const PREVIEW_PANE_WIDTH = 372;
const WINDOW_WIDTH_PREVIEW = WINDOW_WIDTH + PREVIEW_PANE_WIDTH;

/// localStorage keys included in a settings export/import.
const LOCAL_KEYS = [
  "magpie.bangs",
  "magpie.tips",
  "magpie.recents",
  "magpie.taborder",
  "magpie.tabhidden",
  "magpie.defaulttab",
  "magpie.theme",
  "magpie.lang",
  "magpie.pinyin",
  "magpie.scope",
  "magpie.webscope",
  "magpie.sort",
  "magpie.hideonblur",
  "magpie.tabkeys",
] as const;

/// macOS gets ⌘ in the key hints. The handlers themselves accept Ctrl or Cmd
/// on every platform, so this only changes what the footer says.
const IS_MAC = typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);
const IS_WIN = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
const MOD = IS_MAC ? "⌘" : "ctrl";

/// Display names of the system commands, by the backend's ids.
const COMMAND_LABELS: Record<string, string> = {
  lock: "Lock Screen",
  sleep: "Sleep",
  restart: "Restart",
  shutdown: "Shut Down",
  empty_trash: IS_WIN ? "Empty Recycle Bin" : "Empty Trash",
  dark_mode: "Toggle Dark Mode",
};
const COMMAND_GLYPHS: Record<string, string> = {
  lock: "🔒",
  sleep: "🌙",
  restart: "🔄",
  shutdown: "⏻",
  empty_trash: "🗑️",
  dark_mode: "🌓",
};

/// One entry of a row's action menu (Ctrl/Cmd+K). `risky` actions go
/// through the same "press Enter again" confirmation as their rows.
interface RowAction {
  key: string;
  label: string;
  risky?: boolean;
  run: () => Promise<unknown> | void;
}

/// Hide the palette when it loses focus. Stored "1"/"0"; nothing stored
/// means the platform default: on for macOS, where a Spotlight-style panel
/// is expected to go away on click-out, off elsewhere, where dragging files
/// in from Explorer needs the window to survive losing focus.
function loadHideOnBlur(): boolean {
  try {
    const v = localStorage.getItem("magpie.hideonblur");
    if (v === "1" || v === "0") return v === "1";
  } catch {
    /* fall through to the default */
  }
  return IS_MAC;
}

/// Which modifier plus 1–9 jumps straight to a tab.
type TabKeys = "mod" | "alt" | "off";
const TAB_KEYS: { id: TabKeys; label: string }[] = [
  { id: "mod", label: IS_MAC ? "⌘ 1–9" : "Ctrl 1–9" },
  { id: "alt", label: IS_MAC ? "⌥ 1–9" : "Alt 1–9" },
  { id: "off", label: "off" },
];

function loadTabKeys(): TabKeys {
  try {
    const v = localStorage.getItem("magpie.tabkeys");
    if (v === "mod" || v === "alt" || v === "off") return v;
  } catch {
    /* fall through to the default */
  }
  return "mod";
}

/// The 0-based tab a keydown asks for, or null. Reads the physical key
/// (`code`), not the character: Option+1 types "¡" on macOS.
function tabFromKey(
  e: Pick<KeyboardEvent, "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">,
  keys: TabKeys,
): number | null {
  const m = /^Digit([1-9])$/.exec(e.code);
  if (!m || e.shiftKey || keys === "off") return null;
  const mod = e.ctrlKey || e.metaKey;
  const ok = keys === "mod" ? mod && !e.altKey : e.altKey && !mod;
  return ok ? Number(m[1]) - 1 : null;
}

/// Native dialogs take focus from the palette. While one is up, a
/// hide-on-blur must not fire, or the palette vanishes under its own dialog.
let holdOpenCount = 0;
async function holdOpen<T>(f: () => Promise<T>): Promise<T> {
  holdOpenCount++;
  try {
    return await f();
  } finally {
    // focus comes back a moment after the dialog closes
    setTimeout(() => {
      holdOpenCount--;
    }, 300);
  }
}

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

type LocalScope = "all" | "text" | "images" | "videos";
const SCOPES: { id: LocalScope; label: string }[] = [
  { id: "all", label: "all" },
  { id: "text", label: "text" },
  { id: "images", label: "images" },
  { id: "videos", label: "videos" },
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
  if (days < 1) return t("today");
  if (days < 30) return tf("{n}d", { n: Math.floor(days) });
  if (days < 365) return tf("{n}mo", { n: Math.floor(days / 30) });
  return tf("{n}y", { n: Math.floor(days / 365) });
}

function parentDir(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : path;
}

/// mm:ss (or h:mm:ss) for video shot ranges
function fmtTime(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = (s % 60).toString().padStart(2, "0");
  return h > 0 ? `${h}:${m.toString().padStart(2, "0")}:${sec}` : `${m}:${sec}`;
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
      return tf("listing stars… {n}", { n: p.total });
    case "readmes":
      return p.total === 0 ? null : tf("readmes {a}/{b}", { a: p.done ?? 0, b: p.total });
    case "embedding":
      return p.total === 0 ? null : tf("indexing stars {a}/{b}", { a: p.done ?? 0, b: p.total });
  }
}

function localProgressLabel(p: LocalProgress): string | null {
  if (p.stage === "scan") return tf("scanning files… {n}", { n: p.done });
  if ((p.total ?? 0) === 0) return null;
  if (p.stage === "videos") return tf("indexing videos {a}/{b}", { a: p.done, b: p.total ?? 0 });
  return p.stage === "embed-images"
    ? tf("indexing images {a}/{b}", { a: p.done, b: p.total ?? 0 })
    : tf("indexing files {a}/{b}", { a: p.done, b: p.total ?? 0 });
}

export default function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Hit[]>([]);
  const [selected, setSelected] = useState(0);
  // shift+arrows extend a range from this anchor (clips only); null = single
  const [selAnchor, setSelAnchor] = useState<number | null>(null);
  // user-customizable tab order, visibility, and which tab opens on launch.
  // Hidden ids are stored (not visible ones) so future sources default to shown.
  const [sourceOrder, setSourceOrder] = useState<string[]>(loadTabOrder);
  const [hiddenTabs, setHiddenTabs] = useState<string[]>(() => {
    try {
      const raw = localStorage.getItem("magpie.tabhidden");
      if (raw) return JSON.parse(raw) as string[];
    } catch {
      /* default: nothing hidden */
    }
    return [];
  });
  const sources = useMemo(() => {
    const visible = orderedSources(sourceOrder).filter((s) => !hiddenTabs.includes(s.id));
    // never let the tab strip go empty, whatever storage says
    return visible.length > 0 ? visible : orderedSources(sourceOrder);
  }, [sourceOrder, hiddenTabs]);
  const sourcesRef = useRef(sources);
  sourcesRef.current = sources;
  const [defaultTab, setDefaultTab] = useState<string>(
    () => localStorage.getItem("magpie.defaulttab") || "local",
  );
  const [sourceIdx, setSourceIdx] = useState(() => {
    let hidden: string[] = [];
    try {
      hidden = JSON.parse(localStorage.getItem("magpie.tabhidden") ?? "[]") as string[];
    } catch {
      /* nothing hidden */
    }
    let visible = loadTabOrder().filter((id) => !hidden.includes(id));
    if (visible.length === 0) visible = loadTabOrder();
    const want = localStorage.getItem("magpie.defaulttab") || "local";
    const idx = visible.indexOf(want);
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
  const updPhaseRef = useRef(updPhase);
  updPhaseRef.current = updPhase;
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  // set only when list_folders itself fails; an empty list is not a failure
  const [foldersFailed, setFoldersFailed] = useState(false);
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
  const [langPref, setLangPref] = useState<LangPref>(loadLangPref);
  const [pinyinOn, setPinyinOn] = useState(
    () => localStorage.getItem("magpie.pinyin") !== "0",
  );
  const pinyinRef = useRef(pinyinOn);
  pinyinRef.current = pinyinOn;
  const [hideOnBlur, setHideOnBlur] = useState(loadHideOnBlur);
  const hideOnBlurRef = useRef(hideOnBlur);
  hideOnBlurRef.current = hideOnBlur;
  const [tabKeys, setTabKeys] = useState(loadTabKeys);
  const tabKeysRef = useRef(tabKeys);
  tabKeysRef.current = tabKeys;
  // launch at login: the OS registration is the truth, so it's read back
  // from the backend rather than remembered here (null = not known yet)
  const [autostart, setAutostart] = useState<boolean | null>(null);
  // bumped when the backend re-reads app icons; keys the icon components so
  // rows already on screen fetch the fresh ones
  const [iconEpoch, setIconEpoch] = useState(0);
  // a short-lived line in the footer: an action's progress or outcome
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(t);
  }, [notice]);
  // the row (hitKey) whose risky action is waiting for a second Enter
  const [armed, setArmed] = useState<string | null>(null);
  const armedRef = useRef(armed);
  armedRef.current = armed;
  // the action menu (Ctrl/Cmd+K) of the selected row, and its cursor
  const [actionsOpen, setActionsOpen] = useState(false);
  const [actionSel, setActionSel] = useState(0);
  const actionsOpenRef = useRef(actionsOpen);
  actionsOpenRef.current = actionsOpen;

  // language: update the module-level dictionary BEFORE the re-render, then
  // tell the backend so the tray menu follows
  const chooseLang = useCallback((p: LangPref) => {
    setLang(resolveLang(p));
    localStorage.setItem("magpie.lang", p);
    setLangPref(p);
    invoke("set_ui_lang", { lang: resolveLang(p) }).catch(() => {});
  }, []);

  const togglePinyin = useCallback((on: boolean) => {
    localStorage.setItem("magpie.pinyin", on ? "1" : "0");
    setPinyinOn(on);
  }, []);

  // app alias rules ("proxy = clash", one per line); saved to the backend,
  // which re-attaches aliases to the in-memory app list
  const [aliasDraft, setAliasDraft] = useState<string | null>(null);
  const [aliasMsg, setAliasMsg] = useState<string | null>(null);

  // preview pane: → opens (cursor at end of input), ← closes; the selected
  // hit's content renders beside the list. Backend data only for kinds whose
  // content is not already in the hit (file text/image, video shots, repo).
  const [previewOpen, setPreviewOpen] = useState(false);
  const [preview, setPreview] = useState<Record<string, unknown> | null>(null);

  // sync the tray language once at startup ("auto" resolves per OS locale)
  useEffect(() => {
    invoke("set_ui_lang", { lang: resolveLang(loadLangPref()) }).catch(() => {});
  }, []);
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

  // query-box extras: inline calculator, bang web shortcuts, emoji lookup.
  // topRowActive = Enter targets the calc/bang row until the user arrows
  // down into the normal result list (reset on every query change).
  const [calcHit, setCalcHit] = useState<CalcHit | null>(null);
  const [bangHit, setBangHit] = useState<BangMatch | null>(null);
  // `note …`: Enter appends the text to the notes file instead of searching
  const [noteHit, setNoteHit] = useState<NoteMatch | null>(null);
  const [recentsOn, setRecentsOn] = useState(recentsEnabled);
  // settings drafts for the selection-search chord and the notes file
  const [selDraft, setSelDraft] = useState("");
  const [selMsg, setSelMsg] = useState<string | null>(null);
  const [notePathDraft, setNotePathDraft] = useState("");
  const [noteMsg, setNoteMsg] = useState<string | null>(null);
  const [mcpMsg, setMcpMsg] = useState<string | null>(null);
  const [emojiHits, setEmojiHits] = useState<EmojiHit[] | null>(null);
  const [topRowActive, setTopRowActive] = useState(true);
  const [bangsDraft, setBangsDraft] = useState<string | null>(null);
  // a fresh discoverability tip on every summon; the row lives only in the
  // empty state, so typing anything replaces it with results
  const [tip, setTip] = useState(randomTip);
  const [showTips, setShowTips] = useState(tipsEnabled);
  // "in" while a tip is settling, "out" during the hand-off to the next one
  const [tipPhase, setTipPhase] = useState<"in" | "out">("in");
  const inputRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  // monotonic search ticket; see runSearch
  const searchSeqRef = useRef(0);
  const listRef = useRef<HTMLDivElement>(null);
  const queryRef = useRef(query);
  queryRef.current = query;
  const sourceRef = useRef(sourceIdx);
  sourceRef.current = sourceIdx;
  const needsTokenRef = useRef(false);
  const imageQueryRef = useRef<ImageQuery | null>(null);
  imageQueryRef.current = imageQuery;
  const showSettingsRef = useRef(showSettings);
  showSettingsRef.current = showSettings;

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
      setFoldersFailed(false);
    } catch (e) {
      // surfaced in settings; a silent failure here looks like "no folders"
      setFoldersFailed(true);
      setLastError(`folder list failed: ${String(e)}`);
    }
  }, []);

  const saveAliases = useCallback(
    async (text: string) => {
      try {
        await invoke("set_app_aliases", { text });
        setAliasMsg("saved");
        setAliasDraft(null);
        await refreshStatus();
      } catch (e) {
        setAliasMsg(String(e));
      }
    },
    [refreshStatus],
  );

  // opening settings always shows fresh state
  useEffect(() => {
    if (showSettings) {
      refreshStatus();
      refreshFolders();
      invoke<boolean>("get_autostart")
        .then((on) => setAutostart(!!on))
        .catch(() => setAutostart(null));
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
    // one ticket per search: only the latest request may publish results. The
    // backend cancels the superseded query outright (see take_search_conn);
    // this guard covers the tail where an older response is already in flight.
    const seq = ++searchSeqRef.current;
    // empty input shows nothing: the palette stays a bare search box.
    // Clipboard is the exception — its whole point is "what did I just copy",
    // so an empty query lists the most recent clips.
    const srcId = (sourcesRef.current[srcIdx] ?? sourcesRef.current[0]).id;
    // `kill <name>` lists running processes, from any tab
    const killQ = matchKill(q);
    if (killQ) {
      try {
        const ps = await invoke<Omit<ProcessHit, "kind">[]>("list_processes", { query: killQ });
        if (seq === searchSeqRef.current && sourceRef.current === srcIdx) {
          setResults(ps.map((p) => ({ ...p, kind: "process" as const })));
          setSelected(0);
          setSelAnchor(null);
        }
      } catch {
        /* listing failed: keep what is on screen */
      }
      return;
    }
    if (q.trim() === "" && srcId !== "clips") {
      // opt-in: the empty box lists what you opened most recently from this
      // tab, so "back to that file from a minute ago" is two keystrokes
      if (recentsEnabled() && (srcId === "local" || srcId === "github-stars" || srcId === "web")) {
        try {
          const recents = await invoke<Hit[]>("recent_hits", { source: srcId });
          if (seq === searchSeqRef.current && sourceRef.current === srcIdx) {
            setResults(Array.isArray(recents) ? recents : []);
            setSelected(0);
            setSelAnchor(null);
          }
        } catch {
          /* keep the bare box */
        }
        return;
      }
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
        // local: matching apps surface as top hits, then files (+ videos in
        // the images scope — the backend tags each hit's kind)
        const [apps, fs, cmds] = await Promise.all([
          invoke<Omit<AppHit, "kind">[]>("search_apps", {
            query: q,
            pinyin: pinyinRef.current,
          }),
          invoke<Hit[]>("search_local", {
            query: q,
            scope: localScopeRef.current,
          }),
          invoke<Omit<CommandHit, "kind">[]>("search_commands", { query: q }).catch(() => []),
        ]);
        // system commands and apps share one scale; the better match leads
        const top: Hit[] = [
          ...cmds.map((c) => ({ ...c, kind: "command" as const })),
          ...apps.map((a) => ({ ...a, kind: "app" as const })),
        ].sort((a, b) => b.score - a.score);
        hits = [...top, ...fs];
      }
      if (seq === searchSeqRef.current && sourceRef.current === srcIdx) {
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

  // tips rotate only while the empty state is on screen: typing, opening
  // settings, or dropping an image stops the timer instead of burning one
  // in the background
  const tipsIdle = showTips && !showSettings && query.trim() === "" && !imageQuery;
  useEffect(() => {
    if (!tipsIdle) {
      // leaving the empty state mid-hand-off would strand the phase on
      // "out", and the next tip would render already fading away
      setTipPhase("in");
      return;
    }
    const OUT_MS = 140;
    const iv = setInterval(() => {
      setTipPhase("out");
      setTimeout(() => {
        setTip((t) => nextTip(t));
        setTipPhase("in");
      }, OUT_MS);
    }, 8000);
    return () => clearInterval(iv);
  }, [tipsIdle]);

  // extras react to the raw query synchronously (they're cheap and local)
  useEffect(() => {
    setTopRowActive(true);
    const q = query.trim();
    if (q.startsWith(":")) {
      setEmojiHits(searchEmoji(q.slice(1)));
      setCalcHit(null);
      setBangHit(null);
      setNoteHit(null);
      return;
    }
    setEmojiHits(null);
    setNoteHit(matchNote(q));
    setBangHit(matchBang(q, loadBangs()));
    if (q.length >= 2) {
      invoke<CalcHit | null>("calc_query", { query: q })
        .then((r) => setCalcHit(r ?? null))
        .catch(() => setCalcHit(null));
    } else {
      setCalcHit(null);
    }
  }, [query]);

  // live search, debounced; an active image query searches by similarity instead
  useEffect(() => {
    if (imageQuery) {
      invoke<Hit[]>("search_by_image", {
        path: imageQuery.path ?? null,
        bytesB64: imageQuery.bytesB64 ?? null,
      })
        .then((hits) => {
          if (imageQueryRef.current !== imageQuery) return; // stale
          setResults(hits);
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

  // Settings toggle on the WINDOW, not the panel: with settings open the
  // input is hidden and focus can sit on <body>, so a panel-level handler
  // would open settings but never close them. Alt+, matches the app's own
  // Alt family; Ctrl/Cmd+, keeps the platform convention working too. The
  // character OR the physical key matches: Option+, types "≤" on macOS (the
  // key is still Comma), and on AZERTY the comma sits on another key (the
  // character is still ",").
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key === "," || e.code === "Comma") && (e.altKey || e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        setShowSettings((s) => !s);
      } else if (e.key === "Escape" && document.activeElement === document.body) {
        // focus fell to <body> (a click on a row, a pill, the empty strip
        // under the list): the panel handler can't hear this Esc, so run
        // its whole ladder here, down to hiding the palette
        e.preventDefault();
        if (actionsOpenRef.current) {
          setActionsOpen(false);
        } else if (imageQueryRef.current) {
          setImageQuery(null);
        } else if (showSettingsRef.current) {
          setShowSettings(false);
        } else {
          void getCurrentWindow().hide();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // hide on click-out (opt-in, on by default on macOS). A short grace keeps
  // a focus bounce while the window is being shown from hiding it again.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const un = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      clearTimeout(timer);
      if (focused || !hideOnBlurRef.current || holdOpenCount > 0) return;
      timer = setTimeout(() => {
        if (hideOnBlurRef.current && holdOpenCount === 0) void getCurrentWindow().hide();
      }, 120);
    });
    return () => {
      clearTimeout(timer);
      void un.then((f) => f());
    };
  }, []);

  // whenever settings close (shortcut, Esc, ✕), typing must work immediately
  useEffect(() => {
    if (!showSettings) inputRef.current?.focus();
  }, [showSettings]);

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
        setTip(randomTip());
        setTipPhase("in");
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
      // the backend re-read some app icons (new or updated apps): forget the
      // palette's copies and let the rows on screen fetch them again
      listen("app-icons-changed", () => {
        appIconCache.clear();
        setIconEpoch((n) => n + 1);
      }),
      // the selection-search chord: the backend copied the selected text and
      // is about to summon the palette; make that text the query
      listen<string>("search-selection", (e) => {
        setShowSettings(false);
        setImageQuery(null);
        setSourceIdx(0);
        setQuery(e.payload);
      }),
      // no hide-on-blur: the palette stays until the user dismisses it
      // explicitly (Esc, Alt+Space, tray) — dragging files in needs the
      // window to survive losing focus
    ];
    return () => {
      subs.forEach((p) => p.then((un) => un()));
    };
  }, [refreshStatus, refreshFolders, runSearch]);

  // The pane renders only when there is a row to describe, so the reserved
  // column and the wider window follow that same condition. Keying them off
  // the open/closed flag alone leaves a blank strip when a query is cleared
  // while the preview is open.
  const previewShown =
    previewOpen && !showSettings && (results.length > 0 || calcHit != null || bangHit != null);

  // window size follows content; the preview pane widens the palette
  useLayoutEffect(() => {
    const el = panelRef.current;
    if (!el) return;
    const h = Math.min(Math.max(el.offsetHeight, 96), 620);
    const w = previewShown ? WINDOW_WIDTH_PREVIEW : WINDOW_WIDTH;
    // through the backend, not setSize: the backend skips no-op resizes and
    // clamps the window back onto the screen when the reserved room (see
    // show_window) is not there, e.g. after a drag against the screen edge
    invoke("resize_palette", { width: w, height: h }).catch(() => {});
  });

  // fetch preview content for the selected hit (index-local, so cheap); the
  // hit itself already carries everything for clips/web/apps
  useEffect(() => {
    if (!previewOpen) return;
    const hit = results[selected];
    if (!hit) {
      setPreview(null);
      return;
    }
    const needsFetch =
      hit.kind === "file" ||
      hit.kind === "video" ||
      hit.kind === "repo" ||
      (hit.kind === "clip" && hit.clip_kind === "image");
    // clear immediately so a slower fetch can never leave the previous hit's
    // content rendered under the new selection
    setPreview(null);
    if (!needsFetch) {
      return;
    }
    let stale = false;
    const t = setTimeout(() => {
      invoke<Record<string, unknown>>("get_preview", {
        kind: hit.kind,
        id: hit.id,
        query: queryRef.current,
      })
        .then((p) => {
          if (!stale) setPreview(p);
        })
        .catch(() => {
          // "none", not null: null means still loading, and a failed
          // fetch should fall back to the file details, not spin forever
          if (!stale) setPreview({ kind: "none" });
        });
    }, 100);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [previewOpen, selected, results]);

  // keep selection visible
  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-idx="${selected}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selected, results]);

  // Every action that "uses" the query (open, copy, paste, note) ends here:
  // the palette goes away and the query with it, so the next summon starts
  // from an empty box instead of the last thing that was opened. Hide first
  // so the empty state never flashes on screen. Escape deliberately keeps
  // the query: dismissing is not finishing.
  const finishAction = useCallback(async () => {
    await getCurrentWindow().hide();
    setQuery("");
    setImageQuery(null);
  }, []);

  /// Enter (or a click) on the calculator / transform row: copy the image
  /// (a QR code) or the value (a color's hex); an error copies nothing.
  const takeCalc = useCallback(
    async (c: CalcHit) => {
      if (c.error) return;
      try {
        if (c.image) {
          await invoke("copy_png", { pngB64: c.image });
        } else {
          await invoke("copy_clip", { text: c.swatch ?? c.value });
        }
        await finishAction();
      } catch (e) {
        setLastError(String(e));
      }
    },
    [finishAction],
  );

  // a pending confirmation lapses when the list or the selection moves on,
  // or after a few seconds
  useEffect(() => {
    setArmed(null);
  }, [results, selected]);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(null), 4000);
    return () => clearTimeout(t);
  }, [armed]);

  /// Run a system command or end a process. Ending a process, and the
  /// commands that lose something (restart, shut down, empty the trash),
  /// take a second Enter: the first one only arms the row.
  const runRisky = useCallback(
    async (hit: CommandHit | ProcessHit): Promise<boolean> => {
      const key = hitKey(hit);
      const needsConfirm = hit.kind === "process" || hit.destructive;
      if (needsConfirm && armedRef.current !== key) {
        setArmed(key);
        return false;
      }
      setArmed(null);
      try {
        if (hit.kind === "command") {
          await invoke("run_system_command", { id: hit.id, confirmed: true });
          await finishAction();
        } else {
          await invoke("end_process", { pid: hit.pid, name: hit.name });
          setLastError(null);
          // the list without it; the palette stays for the next one
          void runSearch(queryRef.current, sourceRef.current);
        }
      } catch (e) {
        setLastError(String(e));
      }
      return true;
    },
    [finishAction, runSearch],
  );

  const openHit = useCallback(async (hit: Hit | undefined) => {
    if (!hit) return;
    if (hit.kind === "command" || hit.kind === "process") {
      await runRisky(hit);
      return;
    }
    // frecency: remember what actually gets opened (stable identity per kind)
    const frecencyKey =
      hit.kind === "app"
        ? hit.target
        : hit.kind === "file" || hit.kind === "video"
          ? hit.path
          : hit.kind === "bookmark" || hit.kind === "history"
            ? hit.url
            : hit.kind === "repo"
              ? String(hit.id)
              : null;
    if (frecencyKey) {
      invoke("record_hit_use", { kind: hit.kind, key: frecencyKey }).catch(() => {});
    }
    try {
      if (hit.kind === "repo") {
        await invoke("open_repo", { url: hit.html_url });
      } else if (hit.kind === "bookmark" || hit.kind === "history") {
        await invoke("open_repo", { url: hit.url });
      } else if (hit.kind === "app") {
        await invoke("launch_app", { target: hit.target });
      } else if (hit.kind === "clip") {
        if (hit.clip_kind === "image") {
          await invoke("copy_image_clip", { clipId: hit.id });
        } else {
          await invoke("copy_clip", { text: hit.content });
        }
      } else if (hit.kind === "video") {
        // default player with a seek interface starts at the matched shot
        await invoke("play_video", { path: hit.path, tsMs: hit.start_ms });
      } else {
        await invoke("open_file", { path: hit.path });
      }
      await finishAction();
    } catch (e) {
      const msg = String(e);
      if (hit.kind === "app" && msg.includes("moved or been removed")) {
        // the backend is rescanning apps; search again once it has, so the
        // stale row goes away
        setLastError(t("That app has moved or been removed; the app list was refreshed."));
        setTimeout(() => void runSearch(queryRef.current, sourceRef.current), 1200);
      } else {
        setLastError(msg);
      }
    }
  }, [finishAction, runSearch, runRisky]);

  /// What the action menu (Ctrl/Cmd+K) offers for a row. The first entry is
  /// what Enter does on the row itself.
  const rowActions = useCallback(
    (hit: Hit): RowAction[] => {
      const act = (p: Promise<unknown>) => p.catch((e) => setLastError(String(e)));
      const copy = (text: string) => act(invoke("copy_clip", { text }));
      const refresh = () => void runSearch(queryRef.current, sourceRef.current);
      switch (hit.kind) {
        case "file":
          return [
            { key: "reveal", label: t("Show in folder"), run: () => openHit(hit) },
            {
              key: "open",
              label: t("Open with default app"),
              run: () => act(invoke("open_path_default", { path: hit.path }).then(finishAction)),
            },
            ...(/\.pdf$/i.test(hit.path)
              ? [
                  {
                    key: "pdf-md-copy",
                    label: t("Copy as Markdown"),
                    run: async () => {
                      setNotice(t("Converting the PDF…"));
                      try {
                        const md = await invoke<string>("pdf_markdown", { path: hit.path });
                        await invoke("copy_clip", { text: md });
                        setNotice(tf("Copied {n} characters of Markdown", { n: md.length }));
                      } catch (e) {
                        setNotice(null);
                        setLastError(String(e));
                      }
                    },
                  },
                  {
                    key: "pdf-md-save",
                    label: t("Save as Markdown…"),
                    run: async () => {
                      const dest = await holdOpen(() =>
                        saveDialog({
                          defaultPath: hit.path.replace(/\.pdf$/i, ".md"),
                          filters: [{ name: "Markdown", extensions: ["md"] }],
                        }),
                      );
                      if (!dest) return;
                      setNotice(t("Converting the PDF…"));
                      try {
                        await invoke("save_pdf_markdown", { path: hit.path, dest });
                        setNotice(tf("Saved {name}", { name: dest.split(/[\\/]/).pop() ?? dest }));
                      } catch (e) {
                        setNotice(null);
                        setLastError(String(e));
                      }
                    },
                  },
                ]
              : []),
            { key: "copy-path", label: t("Copy path"), run: () => copy(hit.path) },
            { key: "copy-file", label: t("Copy file"), run: () => act(invoke("copy_file_clip", { path: hit.path })) },
          ];
        case "video":
          return [
            { key: "play", label: t("Play"), run: () => openHit(hit) },
            {
              key: "reveal",
              label: t("Show in folder"),
              run: () => act(invoke("open_file", { path: hit.path }).then(finishAction)),
            },
            { key: "copy-path", label: t("Copy path"), run: () => copy(hit.path) },
          ];
        case "app":
          return [
            { key: "open", label: t("Open"), run: () => openHit(hit) },
            {
              key: "reveal",
              label: t("Show in folder"),
              run: () => act(invoke("reveal_app", { target: hit.target }).then(finishAction)),
            },
            ...(IS_WIN
              ? [
                  {
                    key: "admin",
                    label: t("Run as administrator"),
                    run: () => act(invoke("run_app_as_admin", { target: hit.target }).then(finishAction)),
                  },
                ]
              : []),
            { key: "copy-path", label: t("Copy path"), run: () => copy(hit.target) },
          ];
        case "repo":
          return [
            { key: "open", label: t("Open in browser"), run: () => openHit(hit) },
            { key: "copy-url", label: t("Copy URL"), run: () => copy(hit.html_url) },
            { key: "copy-clone", label: t("Copy clone command"), run: () => copy(`git clone ${hit.html_url}.git`) },
          ];
        case "bookmark":
        case "history":
          return [
            { key: "open", label: t("Open in browser"), run: () => openHit(hit) },
            { key: "copy-url", label: t("Copy URL"), run: () => copy(hit.url) },
            {
              key: "copy-md",
              label: t("Copy as Markdown link"),
              run: () => copy(`[${(hit.title || hit.url).replace(/[[\]]/g, "")}](${hit.url})`),
            },
          ];
        case "clip":
          return [
            { key: "copy", label: t("Copy"), run: () => openHit(hit) },
            ...(hit.clip_kind === "text"
              ? [
                  {
                    key: "paste",
                    label: t("Paste into the previous app"),
                    run: () =>
                      act(
                        invoke("paste_clip", { text: hit.content }).then(() => {
                          setQuery("");
                          setImageQuery(null);
                        }),
                      ),
                  },
                ]
              : []),
            {
              key: "pin",
              label: hit.pinned ? t("Unpin") : t("Pin"),
              run: () => act(invoke("toggle_pin_clip", { clipId: hit.id }).then(refresh)),
            },
            {
              key: "delete",
              label: t("Delete from history"),
              run: () => act(invoke("delete_clip", { clipId: hit.id }).then(refresh)),
            },
          ];
        case "command":
          return [{ key: "run", label: t("Run"), risky: hit.destructive, run: () => runRisky(hit) }];
        case "process":
          return [
            { key: "end", label: t("End process"), risky: true, run: () => runRisky(hit) },
            { key: "copy-pid", label: t("Copy PID"), run: () => copy(String(hit.pid)) },
            ...(hit.exe ? [{ key: "copy-path", label: t("Copy path"), run: () => copy(hit.exe ?? "") }] : []),
          ];
      }
    },
    [openHit, runRisky, finishAction, runSearch],
  );

  const menuActions = useMemo(
    () => (actionsOpen && results[selected] ? rowActions(results[selected]) : []),
    [actionsOpen, results, selected, rowActions],
  );

  // the menu belongs to one row: it closes when the list or selection moves
  useEffect(() => {
    setActionsOpen(false);
  }, [results, selected]);

  /// Run the menu's highlighted action. A risky one arms on the first
  /// Enter (the menu stays, showing the confirmation) and runs on the second.
  const runMenuAction = useCallback(
    async (i: number) => {
      const a = menuActions[i];
      if (!a) return;
      const hit = results[selected];
      if (a.risky && hit && armedRef.current !== hitKey(hit)) {
        await a.run(); // arms
        return;
      }
      setActionsOpen(false);
      await a.run();
      inputRef.current?.focus();
    },
    [menuActions, results, selected],
  );

  // `note …` → one line into the notes file, then the palette goes away
  const saveNote = useCallback(async () => {
    const n = noteHit;
    if (!n) return;
    try {
      await invoke("append_note", { text: n.text });
      await finishAction();
    } catch (e) {
      setLastError(String(e));
    }
  }, [noteHit, finishAction]);

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
      await finishAction();
    } catch (e) {
      setLastError(String(e));
    }
  }, [finishAction]);

  const switchSource = useCallback((idx: number) => {
    setSourceIdx(idx);
    setSelected(0);
    setSelAnchor(null);
    setShowSettings(false);
    inputRef.current?.focus();
  }, []);

  // Ctrl/Cmd (or Alt) + 1–9 jumps straight to a tab, in the order the strip
  // shows. On the window so it works whether focus is in the query box, on
  // <body>, or in settings; other text fields (the shortcut recorders, the
  // alias box) keep the keys to themselves.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement;
      const free = el === document.body || el === inputRef.current || el == null;
      if (!free) return;
      const i = tabFromKey(e, tabKeysRef.current);
      if (i == null || i >= sourcesRef.current.length) return;
      e.preventDefault();
      switchSource(i);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [switchSource]);

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

  // show/hide a source tab; the last visible one can't be hidden
  const toggleTabVisible = useCallback(
    (id: string) => {
      setHiddenTabs((prev) => {
        const hiding = !prev.includes(id);
        const allIds = orderedSources(loadTabOrder()).map((s) => s.id);
        const visibleNow = allIds.filter((x) => !prev.includes(x));
        if (hiding && visibleNow.length <= 1) return prev; // keep at least one
        const next = hiding ? [...prev, id] : prev.filter((x) => x !== id);
        localStorage.setItem("magpie.tabhidden", JSON.stringify(next));
        // if the active tab just vanished, land on the first visible one
        const nextVisible = allIds.filter((x) => !next.includes(x));
        const activeId = (sourcesRef.current[sourceIdx] ?? sourcesRef.current[0]).id;
        const idx = nextVisible.indexOf(activeId);
        setSourceIdx(idx >= 0 ? idx : 0);
        return next;
      });
    },
    [sourceIdx],
  );

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
      // Ctrl/Cmd+K: the selected row's action menu (physical key, so any
      // layout works)
      if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.code === "KeyK" || e.key.toLowerCase() === "k")) {
        e.preventDefault();
        if (actionsOpen) {
          setActionsOpen(false);
        } else if (!showSettings && results[selected] && !(topRowActive && (calcHit || bangHit))) {
          setActionSel(0);
          setActionsOpen(true);
        }
        return;
      }
      if (actionsOpen) {
        // the menu has the keyboard until it closes
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const n = menuActions.length;
          setActionSel((s) => (e.key === "ArrowDown" ? (s + 1) % n : (s - 1 + n) % n));
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault();
          void runMenuAction(actionSel);
          return;
        }
        if (e.key === "Escape") {
          e.preventDefault();
          setActionsOpen(false);
          return;
        }
        // any other key (typing) closes the menu and does what it does
        setActionsOpen(false);
      }
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
          if (topRowActive && (calcHit || bangHit) && max >= 0) {
            setTopRowActive(false); // step off the calc/bang row into the list
          } else {
            move((s) => Math.min(s + 1, max));
          }
          break;
        case "ArrowUp":
          e.preventDefault();
          if (!topRowActive && (calcHit || bangHit) && selected === 0) {
            setTopRowActive(true); // step back up onto the calc/bang row
          } else {
            move((s) => Math.max(s - 1, 0));
          }
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
        case "ArrowRight": {
          // open the preview only when the caret has nowhere left to go —
          // otherwise → keeps moving the cursor like any text field
          const inp = inputRef.current;
          const atEnd = !inp || (inp.selectionStart === inp.value.length && inp.selectionEnd === inp.value.length);
          if (!previewOpen && atEnd && max >= 0 && !showSettings) {
            e.preventDefault();
            setPreviewOpen(true);
          }
          break;
        }
        case "ArrowLeft":
          if (previewOpen) {
            e.preventDefault();
            setPreviewOpen(false);
          }
          break;
        case "Enter":
          e.preventDefault();
          if (emojiHits && emojiHits.length > 0) {
            // emoji mode: Enter copies the first match (click copies any)
            void invoke("copy_clip", { text: emojiHits[0].emoji }).then(finishAction);
          } else if (topRowActive && noteHit && !e.ctrlKey && !e.metaKey) {
            void saveNote();
          } else if (topRowActive && bangHit && !e.ctrlKey && !e.metaKey) {
            void invoke("open_repo", { url: bangHit.url }).then(finishAction);
          } else if (topRowActive && calcHit && !e.ctrlKey && !e.metaKey) {
            // color results copy the hex, not the whole display string
            void takeCalc(calcHit);
          } else if (e.ctrlKey || e.metaKey) {
            openWeb();
          } else if (e.shiftKey && source === "clips" && max >= 0) {
            // Shift+Enter pastes straight into the app the palette covered
            const range = selAnchor != null && selHi > selLo ? results.slice(selLo, selHi + 1) : [results[selected]];
            const text = range
              .filter((r) => r.kind === "clip" && r.clip_kind !== "image")
              .map((r) => (r as ClipHit).content)
              .join("\n");
            if (text) {
              void invoke("paste_clip", { text }).then(() => {
                setQuery("");
                setImageQuery(null);
              });
            }
          } else if (source === "clips" && selAnchor != null && selHi > selLo) {
            // multi-select: copy every selected clip, list order, one per line
            const joined = results
              .slice(selLo, selHi + 1)
              .filter((r) => r.kind === "clip" && r.clip_kind !== "image")
              .map((r) => (r as ClipHit).content)
              .join("\n");
            void invoke("copy_clip", { text: joined }).then(finishAction);
          } else {
            openHit(results[selected]);
          }
          break;
        case "p":
        case "P": {
          // Ctrl+P pins/unpins the selected clip (survives pruning, sorts first)
          if (!(e.ctrlKey || e.metaKey) || showSettings) break;
          const r = results[selected];
          if (r && r.kind === "clip") {
            e.preventDefault();
            void invoke("toggle_pin_clip", { clipId: r.id })
              .then(() => runSearch(queryRef.current, sourceRef.current))
              .catch((err) => setLastError(String(err)));
          }
          break;
        }
        case "c":
        case "C": {
          // Ctrl+C copies what identifies the row: a file's path (Ctrl+Shift+C
          // the file itself), a repo's or page's URL, a clip's text, an app's
          // launch target. Text selected in the input keeps native copy.
          if (!(e.ctrlKey || e.metaKey) || showSettings) break;
          const r = results[selected];
          const inp = inputRef.current;
          const hasSelection = inp && inp.selectionStart !== inp.selectionEnd;
          if (!r || hasSelection) break;
          if ((r.kind === "file" || r.kind === "video") && e.shiftKey) {
            e.preventDefault();
            void invoke("copy_file_clip", { path: r.path }).catch((err) => setLastError(String(err)));
            break;
          }
          if (r.kind === "clip" && r.clip_kind === "image") {
            // an image clip's identity is the image itself
            e.preventDefault();
            void invoke("copy_image_clip", { clipId: r.id }).catch((err) => setLastError(String(err)));
            break;
          }
          const text =
            r.kind === "file" || r.kind === "video"
              ? r.path
              : r.kind === "repo"
                ? r.html_url
                : r.kind === "bookmark" || r.kind === "history"
                  ? r.url
                  : r.kind === "clip"
                    ? r.clip_kind === "text"
                      ? r.content
                      : null
                    : r.kind === "app"
                      ? r.target
                      : null;
          if (text != null) {
            e.preventDefault();
            void invoke("copy_clip", { text });
          }
          break;
        }
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
        // note: the Settings toggle (Alt+, / Ctrl+,) lives on a window-level
        // listener — in settings mode the input is hidden and focus can land
        // on <body>, where this panel handler never hears the key
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
    [results, selected, selAnchor, selLo, selHi, sourceIdx, sources, imageQuery, showSettings, source, localScope, webScope, repoSort, previewOpen, openHit, openWeb, switchSource, setScope, setWebScope, deleteSelectedClips, calcHit, bangHit, noteHit, saveNote, emojiHits, topRowActive, runSearch, finishAction, actionsOpen, actionSel, menuActions, runMenuAction],
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
    const file = await holdOpen(() =>
      openDialog({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"] }],
      }),
    );
    if (typeof file !== "string") return;
    const thumb = await invoke<string | null>("preview_thumb", { path: file }).catch(() => null);
    acceptImageQuery({
      label: file.split(/[\\/]/).pop() ?? "image",
      path: file,
      thumbSrc: thumb ? `data:image/jpeg;base64,${thumb}` : undefined,
    });
  }, [acceptImageQuery]);

  const addFolder = useCallback(async () => {
    const dir = await holdOpen(() => openDialog({ directory: true, multiple: false }));
    if (typeof dir !== "string") return;
    try {
      setFolders(await invoke<FolderInfo[]>("add_folder", { path: dir }));
      setLastError(null);
    } catch (e) {
      setLastError(String(e));
    }
  }, []);

  const captureHotkey = useCallback((e: React.KeyboardEvent) => {
    const r = chordFromEvent(e);
    if (r.ignore) return;
    if (r.clear) {
      setHotkeyDraft("");
      setHotkeyMsg(null);
      return;
    }
    if (r.error) {
      setHotkeyMsg(r.error);
      return;
    }
    setHotkeyDraft(r.chord ?? "");
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

  // the same recorder, for the selection-search chord
  const captureSelectionHotkey = useCallback((e: React.KeyboardEvent) => {
    const r = chordFromEvent(e);
    if (r.ignore) return;
    if (r.clear) {
      setSelDraft("");
      setSelMsg(null);
      return;
    }
    if (r.error) {
      setSelMsg(r.error);
      return;
    }
    setSelDraft(r.chord ?? "");
    setSelMsg(null);
  }, []);

  const applySelectionHotkey = useCallback(
    async (chord: string) => {
      try {
        await invoke("set_selection_hotkey", { hotkey: chord });
        setSelMsg("saved");
        setSelDraft("");
        refreshStatus();
      } catch (e) {
        setSelMsg(String(e));
      }
    },
    [refreshStatus],
  );

  // the backend composes the command (URL + bearer header) so the settings
  // row and the docs never drift from what the server actually expects
  const copyMcpCommand = useCallback(async () => {
    if (!status?.mcp_command) return;
    try {
      await invoke("copy_clip", { text: status.mcp_command });
      setMcpMsg("copied");
    } catch (e) {
      setMcpMsg(String(e));
    }
  }, [status?.mcp_command]);
  const rotateMcpToken = useCallback(async () => {
    try {
      setMcpMsg(null);
      await invoke("rotate_mcp_token");
      await refreshStatus();
      setTimeout(() => void refreshStatus(), 600);
      setMcpMsg("new token issued; add the server again in your clients");
    } catch (e) {
      setMcpMsg(String(e));
    }
  }, [refreshStatus]);
  const applyNotePath = useCallback(async () => {
    try {
      await invoke("set_note_path", { path: notePathDraft });
      setNoteMsg("saved");
      setNotePathDraft("");
      refreshStatus();
    } catch (e) {
      setNoteMsg(String(e));
    }
  }, [notePathDraft, refreshStatus]);

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
      // keep the counts in the status line in step with the list
      await refreshStatus();
    } catch (e) {
      setLastError(String(e));
    }
  }, [refreshStatus]);

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
    // periodic re-checks must not yank the UI out of an install in progress
    if (silent && updPhaseRef.current === "downloading") return;
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
      // not plugin-process's relaunch: the backend has to release the
      // single-instance lock first, or the replacement process quits on startup
      await invoke("restart_for_update");
    } catch (e) {
      setUpdPhase("error");
      setUpdError(String(e));
    }
  }, []);

  // quiet startup check, delayed so it never competes with model init —
  // then every 24h while resident (the app is a tray dweller; a launch-only
  // check would miss releases for however long the machine stays up)
  useEffect(() => {
    const t = setTimeout(() => doCheckUpdate(true), 15_000);
    const iv = setInterval(() => doCheckUpdate(true), 24 * 60 * 60 * 1000);
    return () => {
      clearTimeout(t);
      clearInterval(iv);
    };
  }, [doCheckUpdate]);

  // mirror the pending update onto the tray: red-dot icon + menu entry
  useEffect(() => {
    if (updPhase === "available") {
      invoke("set_update_badge", { version: updVersion }).catch(() => {});
    }
  }, [updPhase, updVersion]);

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

  const footerStatus = notice
    ? notice
    : lastError
    ? tf("error: {e}", { e: lastError })
    : modelFailed
      ? t("model download failed, keyword search only (set a mirror in settings)")
      : modelWarming
        ? t("preparing semantic model (first run downloads ~500 MB)")
        : source === "local" && status?.image_model === "loading"
          ? t("preparing image model (first run downloads ~200 MB)")
          : status
            ? source === "github-stars"
              ? tf("{n} repos indexed", { n: status.repo_count })
              : source === "web"
                ? tf("{a} bookmarks · {b} history", {
                    a: status.bookmark_count,
                    b: status.history_count,
                  })
                : source === "clips"
                  ? status.clipboard_enabled
                    ? tf("{n} clips recorded", { n: status.clip_count })
                    : t("clipboard history is off — enable it in settings")
                  : tf("{n} files indexed", { n: status.file_count })
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
      className={`panel ${showSettings ? "settings-mode" : ""} ${
        previewShown ? "preview-open" : ""
      }`}
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
            title={
              tabKeys === "off" || i >= 9
                ? undefined
                : `${tabKeys === "alt" ? (IS_MAC ? "⌥" : "Alt+") : IS_MAC ? "⌘" : "Ctrl+"}${i + 1}`
            }
          >
            {t(s.label)}
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
                title={tf("Search {s} (Shift+Tab cycles)", { s: t(s.label) })}
              >
                {t(s.label)}
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
                title={tf("Search {s} (Shift+Tab cycles)", { s: t(s.label) })}
              >
                {t(s.label)}
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
                title={tf("Sort by {s}", { s: t(s.label) })}
              >
                {t(s.label)}
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
            <button onClick={() => setImageQuery(null)} aria-label={t("Clear")}>
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
              ? t("Searching by image similarity")
              : source === "web"
              ? t("Search bookmarks and browser history")
              : source === "clips"
              ? t("Search your clipboard history")
              : source === "github-stars"
                ? status && status.repo_count > 0
                  ? tf("Search {n} starred repos", { n: status.repo_count })
                  : t("Search your stars")
                : localScope === "videos"
                  ? t("Search videos by name, or describe a scene")
                  : localScope === "images"
                  ? t("Describe the image, or pick / drop / paste one")
                  : status && status.file_count > 0
                    ? tf("Search {n} local files, drop or paste an image", {
                        n: status.file_count,
                      })
                    : t("Search indexed folders")
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
            title={t("Search with an image file")}
            aria-label={t("Search with an image file")}
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
          title={source === "github-stars" ? t("Re-fetch starred repos") : t("Re-scan folders")}
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
          <span>{t("Connect GitHub to sync your stars")}</span>
          <svg className="chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}

      {source === "local" && !showSettings && folders.length === 0 && (
        <button className="collapse-bar" onClick={() => setShowSettings(true)}>
          <span>{t("No folders indexed yet, add some to search locally")}</span>
          <svg className="chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}

      {showSettings ? (
        <div className="card settings-page">
          <div className="card-head" data-tauri-drag-region>
            <p className="card-title settings-title">
              {t("Settings")}
              {status?.version && <span className="ver">v{status.version}</span>}
            </p>
            <button
              className="icon-btn"
              onClick={() => setShowSettings(false)}
              title={t("Back to search (Esc)")}
              aria-label="Close settings"
            >
              ✕
            </button>
          </div>

          {/* CONNECTION */}
          <p className="set-eyebrow">{t("Connection")}</p>
          <div className="set-group">
            <div className="set-row stack">
              <div className="set-head">
                <div className="set-label">
                  <span className="set-name">GitHub</span>
                  <span className="set-desc">
                    {status?.has_token
                      ? t("Paste a new token to replace the current one.")
                      : t(
                          "A personal access token, no scopes needed — it only reads your public stars.",
                        )}
                  </span>
                </div>
                {status?.has_token && status.username ? (
                  <span className="conn-badge ok">
                    <span className="conn-dot" aria-hidden="true" /> {status.username}
                  </span>
                ) : (
                  <span className="conn-badge">{t("not connected")}</span>
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
                  {tokenBusy ? t("Checking") : t("Connect")}
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
                  {t("Create one on github.com")}
                </button>
                {status?.has_token && (
                  <button
                    className="link-btn"
                    onClick={rebuildStars}
                    title={t("Wipe the star index and sync everything from scratch")}
                  >
                    {t("Rebuild star index")}
                  </button>
                )}
              </div>
            </div>
          </div>

          {/* INDEXING */}
          <p className="set-eyebrow">{t("Indexing")}</p>
          <div className="set-group">
            <div className="set-row stack">
              <div className="set-head">
                <div className="set-label">
                  <span className="set-name">
                    {t("Indexed folders")}
                    {status != null && status.folder_count > 0 && (
                      <span className="count-pill">{status.folder_count}</span>
                    )}
                  </span>
                  <span className="set-desc">
                    {t("Scanned recursively; hidden and gitignored paths are skipped.")}
                  </span>
                </div>
                <button className="primary-btn" onClick={addFolder}>
                  {t("Add folder")}
                </button>
              </div>
              {folders.length === 0 &&
                // "failed to load" only when the load actually failed. It used
                // to be inferred from status.folder_count, which lags behind
                // the list after the last folder is removed and showed this
                // as an error every time.
                (foldersFailed ? (
                  <p className="error-line">
                    {t("The folder list failed to load — please report this with the error below.")}
                  </p>
                ) : (
                  <p className="set-empty">{t("No folders yet.")}</p>
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
                        title={t("Rebuild this folder's index from scratch")}
                        aria-label={`Rebuild index for ${f.path}`}
                      >
                        ↻
                      </button>
                      <button
                        className="folder-remove"
                        onClick={() => removeFolder(f.id)}
                        title={t("Remove from index")}
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
                <span className="set-name">{t("Max file size")}</span>
                <span className="set-desc">
                  {t("Larger files index by name only. Changing rebuilds.")}
                </span>
              </div>
              <div className="pill-row">
                {FILE_CAPS.map((c) => (
                  <button
                    key={c.mb}
                    className={`source ${status?.max_file_mb === c.mb ? "active" : ""}`}
                    onClick={() => applyFileCap(c.mb)}
                  >
                    {t(c.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Skip git worktrees")}</span>
                <span className="set-desc">
                  {t(
                    "A linked worktree is a second copy of a checkout that is usually indexed already. Skipped when its main checkout is inside an indexed folder; a worktree that is the only copy is still indexed.",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${(status?.skip_worktrees ?? true) === o.on ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        await invoke("set_skip_worktrees", { enabled: o.on });
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Indexing threads")}</span>
                <span className="set-desc">
                  {t(
                    "CPU threads each model (text, image, OCR) may use while indexing. Fewer keeps the machine responsive; all cores finishes a first index sooner. Applies right away.",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[1, 2, 4, 8]
                  .filter((n) => n <= (status?.cpu_cores ?? 8))
                  .map((n) => ({ label: String(n), threads: n }))
                  .concat([{ label: "all", threads: 0 }])
                  .map((o) => (
                    <button
                      key={o.label}
                      className={`source ${(status?.index_threads ?? 4) === o.threads ? "active" : ""}`}
                      onClick={async () => {
                        try {
                          await invoke("set_index_threads", { threads: o.threads });
                          await refreshStatus();
                        } catch (e) {
                          setLastError(String(e));
                        }
                      }}
                    >
                      {o.threads === 0 ? `${t("all cores")} (${status?.cpu_cores ?? "?"})` : o.label}
                    </button>
                  ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("File changes")}</span>
                <span className="set-desc">
                  {t(
                    "Changes inside indexed folders reach the index within seconds, without waiting for the next full walk.",
                  )}
                  {status?.watch_enabled && status.watch_status === "watching" && (
                    <>
                      {" "}
                      {t("Watching")} {status.watched_folders} {t("folders")}
                    </>
                  )}
                  {status?.watch_enabled && status.watch_status.startsWith("failed") && (
                    <>
                      {" "}
                      <span className="error-line">{status.watch_status}</span>
                    </>
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${(status?.watch_enabled ?? true) === o.on ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        await invoke("set_watch", { enabled: o.on });
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Full rescan")}</span>
                <span className="set-desc">
                  {t(
                    "Every so often all folders are walked again, so anything the watcher missed still lands. Off leaves it to the watcher and to startup.",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[5, 15, 30, 60]
                  .map((n) => ({ label: `${n} ${t("min")}`, minutes: n }))
                  .concat([{ label: t("off"), minutes: 0 }])
                  .map((o) => (
                    <button
                      key={o.minutes}
                      className={`source ${(status?.rescan_minutes ?? 30) === o.minutes ? "active" : ""}`}
                      onClick={async () => {
                        try {
                          await invoke("set_rescan_minutes", { minutes: o.minutes });
                          await refreshStatus();
                        } catch (e) {
                          setLastError(String(e));
                        }
                      }}
                    >
                      {o.label}
                    </button>
                  ))}
                <button
                  className="ghost-btn"
                  disabled={status?.local_indexing ?? false}
                  onClick={async () => {
                    try {
                      await invoke("index_local");
                      await refreshStatus();
                    } catch (e) {
                      setLastError(String(e));
                    }
                  }}
                >
                  {status?.local_indexing ? t("rescanning") : t("Rescan now")}
                </button>
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">
                  {t("Video shot search")}
                  {status != null && status.video_shot_count > 0 && (
                    <span className="count-pill">{status.video_shot_count}</span>
                  )}
                </span>
                <span className="set-desc">
                  {status?.video_note
                    ? status.video_note
                    : t(
                        "Videos in your folders are split into shots; each shot is searchable by image or description. Needs ffmpeg (auto-downloaded if missing).",
                      )}
                  {status?.ffmpeg_status ? (
                    <>
                      {" · ffmpeg: "}
                      {status.ffmpeg_status === "system"
                        ? t("system install")
                        : status.ffmpeg_status === "bundled"
                          ? t("downloaded")
                          : status.ffmpeg_status}
                    </>
                  ) : null}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${status?.video_indexing_enabled === o.on ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        await invoke("set_video_indexing", { enabled: o.on });
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Image text (OCR)")}</span>
                <span className="set-desc">
                  {t(
                    "Reads the text inside indexed images and video frames (screenshots, scans, subtitles) so you can search it — video hits jump to the moment the text appears. Off by default; enabling downloads a small model (~15 MB).",
                  )}
                  {status?.ocr_enabled && status.ocr_status ? (
                    <>
                      {" · "}
                      {status.ocr_status === "ready" ? t("ready") : status.ocr_status}
                    </>
                  ) : null}
                </span>
              </div>
              <div className="pill-row">
                <select
                  className="set-select"
                  value={status?.ocr_model ?? "pp-ocr-v4"}
                  onChange={async (e) => {
                    try {
                      // ids/labels mirror core::ocr::OCR_MODELS
                      await invoke("set_ocr", {
                        enabled: status?.ocr_enabled ?? false,
                        model: e.target.value,
                      });
                      await refreshStatus();
                    } catch (err) {
                      setLastError(String(err));
                    }
                  }}
                  aria-label={t("OCR model")}
                >
                  <option value="pp-ocr-v4">PP-OCRv4 (15 MB)</option>
                  <option value="pp-ocr-v6-small">PP-OCRv6 small (30 MB)</option>
                </select>
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${(status?.ocr_enabled ?? false) === o.on ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        await invoke("set_ocr", {
                          enabled: o.on,
                          model: status?.ocr_model ?? "pp-ocr-v4",
                        });
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            {status?.ocr_enabled && (
              <div className="set-row">
                <div className="set-label">
                  <span className="set-name">{t("Scanned PDFs")}</span>
                  <span className="set-desc">
                    {t(
                      "Also read pages of PDFs that have no text layer. Large scans take a while, so this is your call.",
                    )}
                  </span>
                </div>
                <div className="pill-row">
                  {[
                    { label: "off", on: false },
                    { label: "on", on: true },
                  ].map((o) => (
                    <button
                      key={o.label}
                      className={`source ${(status?.ocr_pdf ?? false) === o.on ? "active" : ""}`}
                      onClick={async () => {
                        try {
                          await invoke("set_ocr_pdf", { enabled: o.on });
                          await refreshStatus();
                        } catch (e) {
                          setLastError(String(e));
                        }
                      }}
                    >
                      {t(o.label)}
                    </button>
                  ))}
                </div>
              </div>
            )}

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Decode limits")}</span>
                <span className="set-desc">
                  {t(
                    "Caps ffmpeg while indexing videos, so it never owns the machine. Hardware decode falls back to software if the driver fails.",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "1", threads: 1 },
                  { label: "2", threads: 2 },
                  { label: "4", threads: 4 },
                  { label: "auto", threads: 0 },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${status?.video_decode_threads === o.threads ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        await invoke("set_video_decode", {
                          threads: o.threads,
                          hwaccel: status?.video_hwaccel ?? false,
                        });
                        await refreshStatus();
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {o.threads === 0 ? t("auto threads") : o.label}
                  </button>
                ))}
                <button
                  className={`source ${status?.video_hwaccel ? "active" : ""}`}
                  onClick={async () => {
                    try {
                      await invoke("set_video_decode", {
                        threads: status?.video_decode_threads ?? 2,
                        hwaccel: !(status?.video_hwaccel ?? false),
                      });
                      await refreshStatus();
                    } catch (e) {
                      setLastError(String(e));
                    }
                  }}
                  title={t("Hardware decode (falls back to software on failure)")}
                >
                  {t("hw decode")}
                </button>
              </div>
            </div>

            <div className="set-row stack">
              <div className="set-head">
                <div className="set-label">
                  <span className="set-name">{t("Model download source")}</span>
                  <span className="set-desc">
                    {t("Pick the mirror if huggingface.co is unreachable from your network.")}
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
                      {t(e.label)}
                    </button>
                  ))}
                </div>
              </div>
              <div className="model-status">
                <span>
                  <span className={`status-dot ${status?.model === "ready" ? "ok" : ""}`} />
                  {t("Semantic model")} —{" "}
                  {status?.model === "ready"
                    ? t("ready")
                    : status?.model === "loading"
                      ? t("downloading (~500 MB, first run)…")
                      : (status?.model ?? "…")}
                </span>
                <span>
                  <span className={`status-dot ${status?.image_model === "ready" ? "ok" : ""}`} />
                  {t("Image model")} —{" "}
                  {status?.image_model === "ready"
                    ? t("ready")
                    : status?.image_model === "loading"
                      ? t("downloading (~200 MB, first run)…")
                      : status?.image_model === "idle"
                        ? t("not loaded; loads once an image, video or image clip is indexed")
                        : (status?.image_model ?? "…")}
                </span>
              </div>
            </div>
          </div>

          {/* APPEARANCE & BEHAVIOR */}
          <p className="set-eyebrow">{t("Appearance & behavior")}</p>
          <div className="set-group">
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Theme")}</span>
              </div>
              <div className="pill-row">
                {THEMES.map((th) => (
                  <button
                    key={th}
                    className={`source ${theme === th ? "active" : ""}`}
                    onClick={() => setTheme(th)}
                  >
                    {t(th)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Language")}</span>
                <span className="set-desc">
                  {t("Palette and settings text; the tray menu follows.")}
                </span>
              </div>
              <div className="pill-row">
                {(
                  [
                    { id: "auto", label: t("auto") },
                    { id: "en", label: "English" },
                    { id: "zh", label: "中文" },
                  ] as { id: LangPref; label: string }[]
                ).map((o) => (
                  <button
                    key={o.id}
                    className={`source ${langPref === o.id ? "active" : ""}`}
                    onClick={() => chooseLang(o.id)}
                  >
                    {o.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Launch at login")}</span>
                <span className="set-desc">
                  {t("Start magpie in the tray when you log in.")}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${autostart === o.on ? "active" : ""}`}
                    disabled={autostart == null}
                    onClick={async () => {
                      try {
                        await invoke("set_autostart", { on: o.on });
                        setAutostart(await invoke<boolean>("get_autostart"));
                      } catch (e) {
                        setLastError(String(e));
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Hide on click-out")}</span>
                <span className="set-desc">
                  {t(
                    "The palette goes away when another window takes focus. Turn off to drag files in from other windows.",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${hideOnBlur === o.on ? "active" : ""}`}
                    onClick={() => {
                      setHideOnBlur(o.on);
                      try {
                        localStorage.setItem("magpie.hideonblur", o.on ? "1" : "0");
                      } catch {
                        /* preference just won't persist */
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Jump to a tab")}</span>
                <span className="set-desc">
                  {t("A modifier plus the tab's number opens it directly, in the order the tabs are shown.")}
                </span>
              </div>
              <div className="pill-row">
                {TAB_KEYS.map((o) => (
                  <button
                    key={o.id}
                    className={`source ${tabKeys === o.id ? "active" : ""}`}
                    onClick={() => {
                      setTabKeys(o.id);
                      try {
                        localStorage.setItem("magpie.tabkeys", o.id);
                      } catch {
                        /* preference just won't persist */
                      }
                    }}
                  >
                    {o.id === "off" ? t("off") : o.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Pinyin app matching")}</span>
                <span className="set-desc">
                  {t(
                    "Latin queries match Chinese app names by full pinyin or initials (wx → 微信).",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${pinyinOn === o.on ? "active" : ""}`}
                    onClick={() => togglePinyin(o.on)}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Recent opens on the empty box")}</span>
                <span className="set-desc">
                  {t("With nothing typed, each tab lists what you opened from it most recently.")}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${recentsOn === o.on ? "active" : ""}`}
                    onClick={() => {
                      setRecentsOn(o.on);
                      setRecentsEnabled(o.on);
                      // the list behind the settings page was built under the
                      // old setting; rebuild it now, or it lingers until the
                      // next keystroke or tab switch
                      void runSearch(queryRef.current, sourceRef.current);
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Launch tips")}</span>
                <span className="set-desc">
                  {t("A one-line tip below the empty search box, fresh on every summon.")}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${showTips === o.on ? "active" : ""}`}
                    onClick={() => {
                      setShowTips(o.on);
                      try {
                        localStorage.setItem(TIPS_KEY, o.on ? "1" : "0");
                      } catch {
                        /* preference just won't persist */
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("App aliases")}</span>
                <span className="set-desc">
                  {t(
                    "One rule per line: alias = app name. The alias matches like a second name (pinyin included).",
                  )}
                </span>
              </div>
              <textarea
                className="alias-input"
                value={aliasDraft ?? status?.app_aliases ?? ""}
                onChange={(e) => {
                  setAliasDraft(e.target.value);
                  setAliasMsg(null);
                }}
                onKeyDown={(e) => e.stopPropagation()}
                placeholder={"proxy = clash\nbrowser = chrome"}
                spellCheck={false}
                rows={3}
              />
              <div className="set-links">
                <button
                  className="link-btn"
                  onClick={() => saveAliases(aliasDraft ?? status?.app_aliases ?? "")}
                  disabled={aliasDraft == null}
                >
                  {t("Save aliases")}
                </button>
                {aliasMsg && (
                  <span className={aliasMsg === "saved" ? "set-empty" : "error-line"}>
                    {t(aliasMsg)}
                  </span>
                )}
              </div>
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("Web shortcuts")}</span>
                <span className="set-desc">
                  {t(
                    "One rule per line: prefix = URL with {q}. Type the prefix, a space, and your query — Enter opens the search.",
                  )}
                </span>
              </div>
              <textarea
                className="alias-input"
                value={bangsDraft ?? (localStorage.getItem(BANGS_KEY) ?? DEFAULT_BANGS)}
                onChange={(e) => setBangsDraft(e.target.value)}
                onKeyDown={(e) => e.stopPropagation()}
                spellCheck={false}
                rows={4}
              />
              <div className="set-links">
                <button
                  className="link-btn"
                  onClick={() => {
                    try {
                      localStorage.setItem(BANGS_KEY, bangsDraft ?? DEFAULT_BANGS);
                    } catch {
                      /* storage unavailable: rules just don't persist */
                    }
                    setBangsDraft(null);
                  }}
                  disabled={bangsDraft == null}
                >
                  {t("Save shortcuts")}
                </button>
              </div>
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("Summon shortcut")}</span>
                <span className="set-desc">
                  {t("Currently")} <kbd>{status?.hotkey ?? "Alt+Space"}</kbd>.{" "}
                  {t(
                    "Click and press a new combination; Backspace clears. OS-reserved chords (like ⌘Space) can't be captured.",
                  )}
                </span>
              </div>
              <div className="token-row">
                <input
                  className="token-input"
                  value={hotkeyDraft}
                  onChange={() => {}}
                  onKeyDown={captureHotkey}
                  placeholder={t("press keys…")}
                  spellCheck={false}
                />
                {hotkeyDraft && (
                  <button
                    className="icon-btn"
                    onClick={() => {
                      setHotkeyDraft("");
                      setHotkeyMsg(null);
                    }}
                    title={t("Clear")}
                    aria-label="Clear recorded shortcut"
                  >
                    ✕
                  </button>
                )}
                <button className="primary-btn" onClick={applyHotkey} disabled={!hotkeyDraft}>
                  {t("Apply")}
                </button>
              </div>
              {hotkeyMsg && (
                // hotkeyMsg holds internal sentinels ("saved") or raw errors;
                // translate known sentinels at render time only
                <p className={hotkeyMsg === "saved" ? "set-empty" : "error-line"}>
                  {t(hotkeyMsg)}
                </p>
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
                    {t("Reset to Alt+Space")}
                  </button>
                </div>
              )}
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("Search selection shortcut")}</span>
                <span className="set-desc">
                  {status?.hotkey_selection ? (
                    <>
                      {t("Currently")} <kbd>{status.hotkey_selection}</kbd>.{" "}
                    </>
                  ) : (
                    <>{t("Removed.")} </>
                  )}
                  {t(
                    "Press it in any app to look up the selected text: magpie copies the selection and opens with it as the query.",
                  )}
                </span>
              </div>
              <div className="token-row">
                <input
                  className="token-input"
                  value={selDraft}
                  onChange={() => {}}
                  onKeyDown={captureSelectionHotkey}
                  placeholder={t("press keys…")}
                  spellCheck={false}
                />
                <button
                  className="primary-btn"
                  onClick={() => void applySelectionHotkey(selDraft)}
                  disabled={!selDraft}
                >
                  {t("Apply")}
                </button>
                {status?.hotkey_selection && (
                  <button className="ghost-btn" onClick={() => void applySelectionHotkey("")}>
                    {t("Remove")}
                  </button>
                )}
              </div>
              {selMsg && (
                <p className={selMsg === "saved" ? "set-empty" : "error-line"}>{t(selMsg)}</p>
              )}
              {status && status.hotkey_selection !== status.hotkey_selection_default && (
                <div className="set-links">
                  <button
                    className="link-btn"
                    onClick={() => void applySelectionHotkey(status.hotkey_selection_default)}
                  >
                    {tf("Reset to {k}", { k: status.hotkey_selection_default })}
                  </button>
                </div>
              )}
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("Notes file")}</span>
                <span className="set-desc">
                  {t("note buy milk appends one timestamped line to this file.")}{" "}
                  {t("Currently")} <code>{status?.note_path ?? "notes.md"}</code>
                </span>
              </div>
              <div className="token-row">
                <input
                  className="token-input"
                  value={notePathDraft}
                  onChange={(e) => setNotePathDraft(e.target.value)}
                  onKeyDown={(e) => e.stopPropagation()}
                  placeholder={t("full path, or empty for the default")}
                  spellCheck={false}
                />
                <button className="primary-btn" onClick={() => void applyNotePath()}>
                  {t("Save")}
                </button>
                <button
                  className="ghost-btn"
                  onClick={() => invoke("open_note_file").catch((e) => setNoteMsg(String(e)))}
                >
                  {t("Open")}
                </button>
              </div>
              {noteMsg && (
                <p className={noteMsg === "saved" ? "set-empty" : "error-line"}>{t(noteMsg)}</p>
              )}
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("MCP server for AI assistants")}</span>
                <span className="set-desc">
                  {t(
                    "Lets Claude Code, Cursor and other MCP clients search this index and read indexed text. Loopback only, behind a token, read-only, off by default.",
                  )}
                </span>
              </div>
              <div className="pill-row">
                {[
                  { label: "off", on: false },
                  { label: "on", on: true },
                ].map((o) => (
                  <button
                    key={o.label}
                    className={`source ${(status?.mcp_enabled ?? false) === o.on ? "active" : ""}`}
                    onClick={async () => {
                      try {
                        setMcpMsg(null);
                        await invoke("set_mcp", { enabled: o.on });
                        // the listener binds in the background; ask twice
                        await refreshStatus();
                        setTimeout(() => void refreshStatus(), 600);
                      } catch (e) {
                        setMcpMsg(String(e));
                      }
                    }}
                  >
                    {t(o.label)}
                  </button>
                ))}
                {status?.mcp_enabled && (
                  <>
                    <button
                      className="ghost-btn"
                      disabled={!status.mcp_command}
                      onClick={() => void copyMcpCommand()}
                    >
                      {t("Copy Claude Code command")}
                    </button>
                    <button className="ghost-btn" onClick={() => void rotateMcpToken()}>
                      {t("New token")}
                    </button>
                  </>
                )}
              </div>
              {status?.mcp_enabled && (
                <p className={status.mcp_status.startsWith("failed") ? "error-line" : "set-empty"}>
                  {status.mcp_status.startsWith("failed") ? (
                    t(status.mcp_status)
                  ) : status.mcp_url ? (
                    <>
                      {t("Listening at")} <code>{status.mcp_url}</code>
                    </>
                  ) : (
                    t("starting")
                  )}
                </p>
              )}
              {status?.mcp_enabled && status.mcp_command && (
                <p className="set-desc">
                  {t("Other clients take the same URL with the header from this command:")}
                  <br />
                  <code className="mono-wrap">{status.mcp_command}</code>
                </p>
              )}
              {mcpMsg && (
                <p className={mcpMsg === "copied" ? "set-empty" : "error-line"}>{t(mcpMsg)}</p>
              )}
            </div>

            <div className="set-row stack">
              <div className="set-label">
                <span className="set-name">{t("Tabs")}</span>
                <span className="set-desc">
                  {t(
                    "Tick which sources appear as tabs (at least one stays on). Drag the handle (or use the arrows) to reorder; ★ marks the tab that opens on launch.",
                  )}
                </span>
              </div>
              <div className="tab-order">
                {orderedSources(sourceOrder).map((s, i, all) => {
                  const hidden = hiddenTabs.includes(s.id);
                  const lastVisible = !hidden && all.filter((x) => !hiddenTabs.includes(x.id)).length <= 1;
                  return (
                    <div
                      key={s.id}
                      className={`tab-row ${dragTab === s.id ? "dragging" : ""} ${hidden ? "hidden-tab" : ""}`}
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
                      <input
                        type="checkbox"
                        className="tab-check"
                        checked={!hidden}
                        disabled={lastVisible}
                        onChange={() => toggleTabVisible(s.id)}
                        title={
                          lastVisible
                            ? t("At least one tab must stay visible")
                            : hidden
                              ? tf("Show {s}", { s: t(s.label) })
                              : tf("Hide {s}", { s: t(s.label) })
                        }
                        aria-label={`Show ${s.label} as a tab`}
                      />
                      <button
                        className={`star-btn ${defaultTab === s.id ? "on" : ""}`}
                        onClick={() => chooseDefaultTab(s.id)}
                        disabled={hidden}
                        title={
                          defaultTab === s.id
                            ? t("Opens on launch")
                            : t("Make this the launch tab")
                        }
                        aria-label={`Make ${s.label} the default tab`}
                      >
                        {defaultTab === s.id ? "★" : "☆"}
                      </button>
                      <span className="tab-name">{t(s.label)}</span>
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
                        disabled={i === all.length - 1}
                        aria-label={`Move ${s.label} down`}
                      >
                        ↓
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>

          {/* PRIVACY */}
          <p className="set-eyebrow">{t("Privacy")}</p>
          <div className="set-group">
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">
                  {t("Clipboard history")}
                  {status?.clipboard_enabled && (
                    <span className="count-pill">{status.clip_count}</span>
                  )}
                </span>
                <span className="set-desc">
                  {t(
                    "Recorded locally, searchable in the Clipboard tab. Password-manager secrets are never stored.",
                  )}
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
                    {t(o.label)}
                  </button>
                ))}
              </div>
            </div>

            {status?.clipboard_enabled && (
              <>
                <div className="set-row">
                  <div className="set-label">
                    <span className="set-name">{t("Keep at most")}</span>
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
                        {t(o.label)}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div className="set-label">
                    <span className="set-name">{t("Keep for")}</span>
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
                        {t(o.label)}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="set-row">
                  <div className="set-label">
                    <span className="set-name">{t("Clear history")}</span>
                    <span className="set-desc">
                      {t("Delete every recorded clip permanently.")}
                    </span>
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
                    {t("Clear")}
                  </button>
                </div>
              </>
            )}
          </div>

          {/* SYSTEM */}
          <p className="set-eyebrow">{t("System")}</p>
          <div className="set-group">
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Updates")}</span>
                <span className="set-desc">
                  {updPhase === "available" || updPhase === "downloading"
                    ? tf("Version {v} is available.", { v: updVersion ?? "" })
                    : updPhase === "none"
                      ? t("You are on the latest version.")
                      : t("Installed in place; your index and settings are kept.")}
                </span>
              </div>
              {updPhase === "available" ? (
                <button className="primary-btn" onClick={doInstallUpdate}>
                  {t("Update & restart")}
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
                  {updPhase === "checking" ? t("Checking…") : t("Check now")}
                </button>
              )}
            </div>
            {updError && <p className="error-line">{updError}</p>}

            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Logs")}</span>
                <span className="set-desc">
                  {t("Local activity log (errors, model/ffmpeg status) — attach it to a bug report. Queries are never logged.")}
                </span>
              </div>
              <div className="pill-row">
                <button
                  className="ghost-btn"
                  onClick={() => invoke("open_log_dir").catch((e) => setLastError(String(e)))}
                >
                  {t("Open log folder")}
                </button>
              </div>
            </div>
            <div className="set-row">
              <div className="set-label">
                <span className="set-name">{t("Settings file")}</span>
                <span className="set-desc">
                  {t("Everything except the GitHub token — move your setup to another machine.")}
                </span>
              </div>
              <div className="pill-row">
                <button
                  className="ghost-btn"
                  onClick={async () => {
                    try {
                      const path = await holdOpen(() =>
                        saveDialog({
                          defaultPath: "magpie-settings.json",
                          filters: [{ name: "JSON", extensions: ["json"] }],
                        }),
                      );
                      if (!path) return;
                      const frontend: Record<string, string> = {};
                      for (const k of LOCAL_KEYS) {
                        const v = localStorage.getItem(k);
                        if (v != null) frontend[k] = v;
                      }
                      await invoke("export_settings", { path, frontend });
                      setLastError(null);
                    } catch (e) {
                      setLastError(String(e));
                    }
                  }}
                >
                  {t("Export")}
                </button>
                <button
                  className="ghost-btn"
                  onClick={async () => {
                    try {
                      const path = await holdOpen(() =>
                        openDialog({
                          multiple: false,
                          filters: [{ name: "JSON", extensions: ["json"] }],
                        }),
                      );
                      if (typeof path !== "string") return;
                      const frontend = await invoke<Record<string, string>>("import_settings", {
                        path,
                      });
                      for (const k of LOCAL_KEYS) {
                        if (typeof frontend[k] === "string") localStorage.setItem(k, frontend[k]);
                      }
                      window.location.reload(); // re-read every store in one clean pass
                    } catch (e) {
                      setLastError(String(e));
                    }
                  }}
                >
                  {t("Import")}
                </button>
              </div>
            </div>
          </div>

          {lastError && <p className="error-line">{lastError}</p>}
        </div>
      ) : emojiHits ? (
        <div className="emoji-grid">
          {emojiHits.length === 0 && <span className="emoji-empty">{t("No matching emoji")}</span>}
          {emojiHits.map((h, i) => (
            <button
              key={h.emoji}
              className={`emoji-cell ${i === 0 ? "first" : ""}`}
              title={h.name}
              onClick={() =>
                void invoke("copy_clip", { text: h.emoji }).then(finishAction)
              }
            >
              {h.emoji}
            </button>
          ))}
        </div>
      ) : (
        (results.length > 0 || calcHit != null || bangHit != null || noteHit != null) && (
          <div
            className="body-row"
            // the menu floats over the list; a short list grows to hold it
            style={actionsOpen ? { minHeight: menuActions.length * 34 + 28 } : undefined}
          >
          {actionsOpen && menuActions.length > 0 && (
            <div className="action-menu" role="menu">
              {menuActions.map((a, i) => (
                <div
                  key={a.key}
                  role="menuitem"
                  className={`action-item ${i === actionSel ? "on" : ""} ${a.risky ? "risky" : ""}`}
                  onMouseMove={() => setActionSel(i)}
                  onMouseDown={(ev) => ev.preventDefault() /* keep focus in the box */}
                  onClick={() => void runMenuAction(i)}
                >
                  <span>
                    {a.risky && results[selected] && armed === hitKey(results[selected])
                      ? t("Press Enter again to confirm")
                      : a.label}
                  </span>
                  {i === 0 && <kbd>⏎</kbd>}
                </div>
              ))}
            </div>
          )}
          <div className="results" ref={listRef}>
            {bangHit && (
              <div
                className={`row extra-row ${topRowActive ? "selected" : ""}`}
                onClick={() =>
                  void invoke("open_repo", { url: bangHit.url }).then(finishAction)
                }
              >
                <div className="row-main">
                  <span className="row-title">
                    {tf("Search {s} for", { s: bangHit.host })} “{bangHit.rest}”
                  </span>
                  <span className="row-sub">{bangHit.url}</span>
                </div>
                <span className="badge">{t("web")}</span>
              </div>
            )}
            {noteHit && (
              <div
                className={`row extra-row ${topRowActive ? "selected" : ""}`}
                onClick={() => void saveNote()}
              >
                <div className="row-main">
                  <span className="row-title">📝 {noteHit.text}</span>
                  <span className="row-sub">{t("Enter appends this line to your notes file")}</span>
                </div>
                <span className="badge">{t("note")}</span>
              </div>
            )}
            {!bangHit && !noteHit && calcHit && (
              <div
                className={`row extra-row ${topRowActive ? "selected" : ""}`}
                onClick={() =>
                  void takeCalc(calcHit)
                }
              >
                <div className="row-lead">
                {calcHit.image && (
                  <img className="calc-image" src={`data:image/png;base64,${calcHit.image}`} alt="" />
                )}
                <div className="row-main">
                  <span className={`row-title calc-value ${calcHit.error ? "calc-error" : ""}`}>
                    {calcHit.swatch && (
                      <i className="color-swatch" style={{ background: calcHit.swatch }} />
                    )}
                    {calcHit.swatch || calcHit.error || calcHit.image
                      ? calcHit.value
                      : // a multi-line result (pretty JSON) previews as one line
                        `= ${oneLine(calcHit.value)}`}
                  </span>
                  <span className="row-sub">
                    {calcHit.alt ? `${calcHit.alt}${calcHit.error ? "" : " · "}` : ""}
                    {calcHit.error
                      ? ""
                      : calcHit.image
                        ? t("Enter copies the image")
                        : t("Enter copies the result")}
                  </span>
                </div>
                </div>
                <span className="badge">{t("calc")}</span>
              </div>
            )}
            {results.map((r, i) => (
              <div
                key={hitKey(r)}
                data-idx={i}
                className={`row ${i >= selLo && i <= selHi && !(topRowActive && (calcHit || bangHit)) ? "selected" : ""} ${armed === hitKey(r) ? "armed" : ""}`}
                onMouseMove={() => {
                  if (selAnchor == null) setSelected(i);
                }}
                onClick={() => openHit(r)}
              >
                {r.kind === "command" ? (
                  <>
                    <div className="row-lead">
                      <span className="app-icon cmd-glyph">{COMMAND_GLYPHS[r.id] ?? "⚙️"}</span>
                      <div className="row-main">
                        <span className="row-title">{t(COMMAND_LABELS[r.id] ?? r.id)}</span>
                        <span className="row-sub">
                          {armed === hitKey(r) ? t("Press Enter again to confirm") : t("System command")}
                        </span>
                      </div>
                    </div>
                    <div className="row-meta">
                      <span className="app-badge">{t("Command")}</span>
                    </div>
                  </>
                ) : r.kind === "process" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">{r.name}</span>
                      <span className="row-sub">
                        {armed === hitKey(r)
                          ? t("Press Enter again to end this process")
                          : `PID ${r.pid} · ${formatSize(r.memory)}${r.exe ? ` · ${r.exe}` : ""}`}
                      </span>
                    </div>
                    <div className="row-meta">
                      <span className="web-badge">{t("Process")}</span>
                    </div>
                  </>
                ) : r.kind === "clip" && r.clip_kind === "image" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">{t("Image")}</span>
                      <span className="row-sub">
                        {r.width && r.height ? `${r.width} × ${r.height}` : ""}
                      </span>
                    </div>
                    <div className="row-meta">
                      {relTimeUnix(r.last_copied) && (
                        <span className="mono">{relTimeUnix(r.last_copied)}</span>
                      )}
                      {r.copy_count > 1 && <span>×{r.copy_count}</span>}
                      {r.thumb && (
                        <img className="thumb" src={`data:image/jpeg;base64,${r.thumb}`} alt="" />
                      )}
                    </div>
                  </>
                ) : r.kind === "clip" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title clip-text" title={r.content}>
                        {r.pinned && <span className="pin-mark">📌 </span>}
                        {r.content.split("\n")[0].slice(0, 200)}
                      </span>
                      {r.content.includes("\n") && (
                        <span className="row-sub">
                          {tf("{n} lines", {
                            n: r.content.split("\n").filter((l) => l.trim()).length,
                          })}
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
                    <div className="row-lead">
                      <AppIcon key={iconEpoch} target={r.target} />
                      <div className="row-main">
                        <span className="row-title">{r.name}</span>
                        <span className="row-sub">{t("Application")}</span>
                      </div>
                    </div>
                    <div className="row-meta">
                      <span className="app-badge">{t("App")}</span>
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
                        <span className="web-badge bookmark">{t("Bookmark")}</span>
                      )}
                      {r.added_at != null && relTimeUnix(r.added_at) && (
                        <span
                          className="mono"
                          title={tf("added {d}", {
                            d: new Date(r.added_at * 1000).toISOString().slice(0, 10),
                          })}
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
                        <span className="web-badge history">{t("History")}</span>
                      )}
                      {r.last_visit != null && relTimeUnix(r.last_visit) && (
                        <span className="mono">{relTimeUnix(r.last_visit)}</span>
                      )}
                      <span>{r.visit_count}×</span>
                    </div>
                  </>
                ) : r.kind === "video" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">{r.name}</span>
                      <span className="row-sub">
                        {/* a shot hit names its time range; a filename hit is whole-file */}
                        {r.end_ms > r.start_ms && (
                          <span className="dim-prefix">
                            {fmtTime(r.start_ms)} – {fmtTime(r.end_ms)} ·{" "}
                          </span>
                        )}
                        {parentDir(r.path)}
                      </span>
                    </div>
                    <div className="row-meta">
                      <span className="app-badge">{t("Video")}</span>
                      {r.duration_ms > 0 && <span className="mono">{fmtTime(r.duration_ms)}</span>}
                      {imageQuery && (
                        <span className="sim">{Math.max(0, Math.round(r.score * 100))}%</span>
                      )}
                      {r.thumb && (
                        <img className="thumb" src={`data:image/jpeg;base64,${r.thumb}`} alt="" />
                      )}
                    </div>
                  </>
                ) : r.kind === "repo" ? (
                  <>
                    <div className="row-main">
                      <span className="row-title">
                        <span className="dim-prefix">{r.full_name.split("/")[0]}/</span>
                        {r.full_name.split("/")[1]}
                        {r.archived && <span className="badge">{t("archived")}</span>}
                      </span>
                      {r.description && <span className="row-sub">{r.description}</span>}
                    </div>
                    <div className="row-meta">
                      {relTime(r.pushed_at) && (
                        <span
                          className="mono"
                          title={tf("last push {d}", { d: r.pushed_at?.slice(0, 10) ?? "" })}
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
                          title={tf("modified {d}", {
                            d: new Date(r.mtime * 1000).toISOString().slice(0, 10),
                          })}
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
          {previewOpen && (
            <PreviewPane hit={results[selected]} data={preview} query={query} />
          )}
          </div>
        )
      )}

      {!needsToken && !showSettings && results.length === 0 && query.trim() !== "" && !noteHit && (
        <div className="empty">
          {source === "github-stars"
            ? t("No matches in your stars")
            : source === "web"
              ? t("No matching bookmarks or history")
              : source === "clips"
                ? t("No matching clips")
                : t("No matches in indexed folders")}
        </div>
      )}

      {tipsIdle && (
        <div className="tip-row">
          {/* keyed so React swaps the node and the enter animation replays */}
          <span key={tip} className={`tip-text ${tipPhase === "out" ? "leaving" : ""}`}>
            <span className="tip-bulb">💡</span> {t(tip)}
          </span>
        </div>
      )}

      {/* footer */}
      <div className="footer">
        <span className="hints">
          {/* ↑↓ needs no hint; the action menu does, and the footer has
              room for one key only (see the note on the 1–9 chord below) */}
          <span>
            <kbd>{MOD}K</kbd> {t("actions")}
          </span>
          <span>
            <kbd>⏎</kbd> {source === "clips" ? t("copy") : t("open")}
          </span>
          {/* the 1–9 jump chord is not listed here: the footer is full, and
              an extra key pushed the index status off its right end. Each
              tab's tooltip names it, and so does a launch tip. */}
          <span>
            <kbd>tab</kbd> {t("source")}
          </span>
          {source === "clips" && (
            <>
              <span>
                <kbd>⇧⏎</kbd> {t("paste")}
              </span>
              <span>
                <kbd>⇧↑↓</kbd> {t("select")}
              </span>
              <span>
                <kbd>{MOD}⌦</kbd> {t("delete")}
              </span>
            </>
          )}
          {(source === "local" || source === "web" || source === "github-stars") && (
            <span>
              <kbd>⇧tab</kbd> {source === "github-stars" ? t("sort") : t("scope")}
            </span>
          )}
          {results.length > 0 && !showSettings && (
            <span>
              <kbd>{previewOpen ? "←" : "→"}</kbd> {previewOpen ? t("close preview") : t("preview")}
            </span>
          )}
          <span>
            <kbd>{IS_MAC ? "⌘," : "alt,"}</kbd> {t("settings")}
            {(updPhase === "available" || updPhase === "downloading") && (
              <i className="upd-dot" title={tf("Version {v} is available.", { v: updVersion ?? "" })} />
            )}
          </span>
          <span>
            <kbd>{MOD}⏎</kbd> {t("web")}
          </span>
          {/* no "esc hide": every launcher's Esc closes it, and the room
              went to the action menu's hint without squeezing the status */}
        </span>
        <span className="status">{footerStatus}</span>
      </div>
    </div>
  );
}

/// Turn a keydown inside a shortcut recorder into a chord string.
/// Modifier-only presses are ignored; a bare Backspace/Delete/Escape clears.
/// Shift-only or bare chords (Shift+A, Tab…) would hijack normal typing
/// system-wide, so anything that is not an F-key needs Ctrl/Alt/Super.
function chordFromEvent(e: React.KeyboardEvent): {
  chord?: string;
  clear?: boolean;
  ignore?: boolean;
  error?: string;
} {
  e.preventDefault();
  e.stopPropagation();
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return { ignore: true };
  if (!e.ctrlKey && !e.altKey && !e.metaKey && ["Backspace", "Delete", "Escape"].includes(e.key)) {
    return { clear: true };
  }
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  let key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (e.key === " ") key = "Space";
  const strongModifier = e.ctrlKey || e.altKey || e.metaKey;
  if (!strongModifier && !/^F([1-9]|1[0-9]|2[0-4])$/.test(key)) {
    return { error: "use Ctrl/Alt/Win plus a key, or an F-key" };
  }
  parts.push(key);
  return { chord: parts.join("+") };
}

/// One-line rendering of a possibly multi-line value (pretty JSON, sorted
/// lines): the first line, clipped, plus a line count. Enter still copies
/// the whole thing.
/// A result as one line for the top row: whitespace runs collapsed (so
/// pretty JSON reads `{ "a": 1, … }` instead of a lone `{`), cut at 160
/// chars, and a line count when it had several.
function oneLine(value: string): string {
  const lines = value.split("\n").length;
  const flat = value.replace(/\s+/g, " ").trim();
  const head = flat.length > 160 ? `${flat.slice(0, 160)}…` : flat;
  return lines > 1 ? `${head}  (${lines} lines)` : head;
}

/// Highlight every occurrence of the query's words in a text run.
function highlightQuery(text: string, query: string): React.ReactNode[] {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w.length >= 2);
  if (words.length === 0) return [text];
  const out: React.ReactNode[] = [];
  const lower = text.toLowerCase();
  let pos = 0;
  let key = 0;
  while (pos < text.length) {
    let best = -1;
    let bestLen = 0;
    for (const w of words) {
      const i = lower.indexOf(w, pos);
      if (i >= 0 && (best === -1 || i < best)) {
        best = i;
        bestLen = w.length;
      }
    }
    if (best === -1) {
      out.push(text.slice(pos));
      break;
    }
    if (best > pos) out.push(text.slice(pos, best));
    out.push(<mark key={key++}>{text.slice(best, best + bestLen)}</mark>);
    pos = best + bestLen;
  }
  return out;
}

/// Right-hand preview of the selected hit. Web/clip/app hits carry all their
/// data already; file/video/repo previews arrive via get_preview.
/// App icons by launch target, for the whole session: the backend caches
/// too, this just saves the round trip on every re-render and keystroke.
const appIconCache = new Map<string, string | null>();

/// The OS icon for an app row. Until it arrives (or when there is none) an
/// empty box of the same size holds the title's place, so rows never shift.
function AppIcon({ target }: { target: string }) {
  const [src, setSrc] = useState<string | null | undefined>(() => appIconCache.get(target));
  useEffect(() => {
    if (appIconCache.has(target)) {
      setSrc(appIconCache.get(target));
      return;
    }
    let live = true;
    invoke<string | null>("app_icon", { target })
      .then((url) => {
        appIconCache.set(target, url ?? null);
        if (live) setSrc(url ?? null);
      })
      .catch(() => {
        if (live) setSrc(null);
      });
    return () => {
      live = false;
    };
  }, [target]);
  return src ? <img className="app-icon" src={src} alt="" /> : <span className="app-icon" />;
}

function PreviewPane({
  hit,
  data,
  query,
}: {
  hit: Hit | undefined;
  data: Record<string, unknown> | null;
  query: string;
}) {
  if (!hit) return <div className="preview-pane" />;
  return (
    <div className="preview-pane">
      {hit.kind === "clip" && hit.clip_kind === "image" ? (
        <>
          {data?.kind === "image" && typeof data.image === "string" ? (
            <img className="pv-image" src={`data:image/jpeg;base64,${data.image}`} alt="" />
          ) : hit.thumb ? (
            <img className="pv-image" src={`data:image/jpeg;base64,${hit.thumb}`} alt="" />
          ) : null}
          <p className="pv-meta">
            {hit.width} × {hit.height} · {relTimeUnix(hit.last_copied)} · ×{hit.copy_count}
          </p>
        </>
      ) : hit.kind === "clip" ? (
        <>
          <p className="pv-title">{t("Clipboard entry")}</p>
          <pre className="pv-text">{hit.content}</pre>
          <p className="pv-meta">
            {relTimeUnix(hit.last_copied)} · ×{hit.copy_count}
          </p>
        </>
      ) : hit.kind === "bookmark" || hit.kind === "history" ? (
        <>
          <p className="pv-title">{hit.title || hit.url}</p>
          <p className="pv-link">{hit.url}</p>
          <p className="pv-meta">
            {hit.kind === "bookmark"
              ? `${t("Bookmark")} · ${hit.folder || "—"} · ${hit.browser}`
              : `${t("History")} · ${hit.visit_count}× · ${hit.browser}`}
          </p>
        </>
      ) : hit.kind === "app" ? (
        <>
          <p className="pv-title">{hit.name}</p>
          <p className="pv-meta mono-wrap">{hit.target}</p>
        </>
      ) : hit.kind === "repo" && data?.kind === "repo" ? (
        <>
          {typeof data.description === "string" && data.description && (
            <p className="pv-desc">{data.description}</p>
          )}
          {typeof data.topics === "string" && data.topics !== "[]" && (
            <p className="pv-chips">
              {(JSON.parse(data.topics as string) as string[]).slice(0, 8).map((tp) => (
                <span key={tp} className="pv-chip">
                  {tp}
                </span>
              ))}
            </p>
          )}
          {typeof data.readme === "string" && data.readme && (
            <pre className="pv-text">
              {data.readme}
              {data.readme_clipped ? " …" : ""}
            </pre>
          )}
        </>
      ) : hit.kind === "file" && data?.kind === "image" && typeof data.image === "string" ? (
        <img className="pv-image" src={`data:image/jpeg;base64,${data.image}`} alt="" />
      ) : hit.kind === "file" && data?.kind === "text" && typeof data.text === "string" ? (
        <pre className="pv-text">
          {data.clipped_head ? "… " : ""}
          {highlightQuery(data.text, query)}
          {data.clipped_tail ? " …" : ""}
        </pre>
      ) : hit.kind === "video" && data?.kind === "image" && typeof data.image === "string" ? (
        // a video the index has not cut into shots yet: one frame of it
        <img className="pv-image" src={`data:image/jpeg;base64,${data.image}`} alt="" />
      ) : (hit.kind === "video" || hit.kind === "file") &&
        data?.kind === "shots" &&
        Array.isArray(data.shots) &&
        data.shots.length > 0 ? (
        <>
          <p className="pv-title">{t("Shots")}</p>
          <div className="pv-shots">
            {(data.shots as { start_ms: number; end_ms: number; ts_ms: number; thumb: string | null }[]).map(
              (s) => (
                <div
                  key={s.ts_ms}
                  className={`pv-shot ${hit.kind === "video" && hit.ts_ms === s.ts_ms ? "on" : ""}`}
                >
                  {s.thumb ? (
                    <img src={`data:image/jpeg;base64,${s.thumb}`} alt="" />
                  ) : (
                    <span className="pv-shot-empty" />
                  )}
                  <span className="pv-shot-t">{fmtTime(s.start_ms)}</span>
                </div>
              ),
            )}
          </div>
        </>
      ) : data == null ? null /* still loading */ : hit.kind === "file" || hit.kind === "video" ? (
        // nothing to render inline (a binary, an empty file, a video with
        // no ffmpeg at hand): the facts about the file instead of a blank
        <>
          <p className="pv-title">{hit.name}</p>
          <p className="pv-meta mono-wrap">{parentDir(hit.path)}</p>
          <p className="pv-meta">
            {hit.kind === "file"
              ? [formatSize(hit.size), relTimeUnix(hit.mtime)].filter(Boolean).join(" · ")
              : hit.duration_ms > 0
                ? fmtTime(hit.duration_ms)
                : t("Video")}
          </p>
        </>
      ) : (
        <p className="pv-meta">{t("No preview")}</p>
      )}
    </div>
  );
}
