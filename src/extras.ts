// Query-box extras: bang-style web shortcuts and emoji lookup.
// Pure functions over localStorage-backed config — no backend involvement.
import emojilib from "emojilib";

export const BANGS_KEY = "magpie.bangs";
export const DEFAULT_BANGS = `g = https://www.google.com/search?q={q}
b = https://www.bing.com/search?q={q}
gh = https://github.com/search?q={q}&type=repositories
bd = https://www.baidu.com/s?wd={q}`;

/** One rule per line: `prefix = url-with-{q}`. Bad lines are skipped. */
export function parseBangs(text: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const line of text.split("\n")) {
    const eq = line.indexOf("=");
    if (eq < 1) continue;
    const prefix = line.slice(0, eq).trim().toLowerCase();
    const url = line.slice(eq + 1).trim();
    if (prefix && !prefix.includes(" ") && url.includes("{q}") && /^https?:\/\//i.test(url)) {
      out.set(prefix, url);
    }
  }
  return out;
}

export function loadBangs(): Map<string, string> {
  try {
    return parseBangs(localStorage.getItem(BANGS_KEY) ?? DEFAULT_BANGS);
  } catch {
    return parseBangs(DEFAULT_BANGS);
  }
}

export interface BangMatch {
  prefix: string;
  url: string;
  host: string;
  rest: string;
}

/** `g rust tokio` -> the g rule with rest "rust tokio". Needs a space and a
 * non-empty query so plain prefixes still search normally. */
export function matchBang(query: string, bangs: Map<string, string>): BangMatch | null {
  const sp = query.indexOf(" ");
  if (sp < 1) return null;
  const prefix = query.slice(0, sp).toLowerCase();
  const rest = query.slice(sp + 1).trim();
  const tpl = bangs.get(prefix);
  if (!tpl || !rest) return null;
  const url = tpl.replace("{q}", encodeURIComponent(rest));
  let host = "";
  try {
    host = new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
  return { prefix, url, host, rest };
}

// ---------- launch tips ----------

export const TIPS_KEY = "magpie.tips";

/** One-liner discoverability tips, shown in the empty state. English text
 * doubles as the i18n key (translated in i18n.ts like everything else). */
export const TIPS: string[] = [
  "Ctrl+P pins a clip — it sorts first and never gets pruned",
  "The query box is a calculator: 3*(5+2)^2 — Enter copies the answer",
  "Type : then a keyword to find emoji — :fire or :火",
  "gh magpie searches GitHub straight from the box — prefixes are editable in settings",
  "Shift+Enter pastes a clip straight into the app you came from",
  "→ previews the selected result — file text, images, a video's shots",
  "Ctrl+Shift+C puts the file itself on the clipboard — paste it as an attachment",
  "Drop or paste an image to search your files by visual similarity",
  "ts 1700000000 turns a unix timestamp into local time",
  "#ff6600 shows the color with rgb/hsl — Enter copies the hex",
  "pwd 24 generates a cryptographically random password",
  "Enable OCR in settings — words inside screenshots and videos become searchable",
  "Enter on a video hit starts playback right at the matched scene",
  "App names match by pinyin too — wx finds 微信, vsc finds VS Code",
  "Ctrl+Enter hands your query to the browser — URLs open directly",
  "Unit conversion lives in the box: 100 mb to gb, 32 f to c",
  "Give apps aliases in settings: proxy = clash",
  "Shift+Tab cycles the local scope: all / text / images / videos",
  "Export your whole setup from Settings → System — the GitHub token stays out",
];

export function tipsEnabled(): boolean {
  try {
    return localStorage.getItem(TIPS_KEY) !== "0";
  } catch {
    return true;
  }
}

export function randomTip(): string {
  return TIPS[Math.floor(Math.random() * TIPS.length)];
}

/** A different tip than `current` — the rotation must never repeat itself
 * in place, which reads as a stuck UI. */
export function nextTip(current: string): string {
  if (TIPS.length < 2) return TIPS[0] ?? current;
  let pick = current;
  while (pick === current) pick = randomTip();
  return pick;
}

// ---------- emoji ----------

export interface EmojiHit {
  emoji: string;
  name: string;
}

// A hand-picked Chinese keyword layer over emojilib's English keywords, for
// the emoji people actually reach for.
const ZH: Record<string, string> = {
  "😂": "笑哭 大笑", "❤️": "爱心 红心", "👍": "赞 好", "🔥": "火 热",
  "😭": "哭", "🎉": "庆祝 撒花", "😄": "笑 开心", "🙏": "拜托 感谢 祈祷",
  "😅": "尬笑 汗", "🤣": "笑翻", "💪": "加油 肌肉", "👏": "鼓掌",
  "🌹": "玫瑰 花", "😊": "微笑", "🎂": "蛋糕 生日", "☕": "咖啡",
  "😎": "酷 墨镜", "🤔": "思考 疑惑", "👌": "OK 没问题", "💯": "满分",
  "😴": "困 睡觉", "🍺": "啤酒 干杯", "🌙": "月亮 晚安", "☀️": "太阳",
  "🐶": "狗", "🐱": "猫", "🚀": "火箭 起飞", "⭐": "星星",
  "💰": "钱", "🎁": "礼物", "😡": "生气 愤怒", "🥰": "喜欢 爱",
  "🤝": "握手 合作", "✅": "对勾 完成", "❌": "叉 错误", "⚠️": "警告 注意",
  "🙈": "捂脸", "🍉": "西瓜 吃瓜", "🧧": "红包", "🐂": "牛",
};

const ALL: [string, string[]][] = Object.entries(emojilib as Record<string, string[]>);

/** Search by English keywords (emojilib) or the Chinese layer. Empty query
 * returns a popular starter set. */
export function searchEmoji(q: string, limit = 40): EmojiHit[] {
  const query = q.trim().toLowerCase();
  if (!query) {
    return Object.keys(ZH).slice(0, limit).map((e) => ({ emoji: e, name: ZH[e] }));
  }
  // primary-name match first, then any exact keyword, then substrings
  const primary: EmojiHit[] = [];
  const exact: EmojiHit[] = [];
  const partial: EmojiHit[] = [];
  for (const [emoji, keywords] of ALL) {
    const zh = ZH[emoji];
    const zhWords = zh ? zh.split(" ") : [];
    const hit = { emoji, name: zh ?? keywords[0].replace(/_/g, " ") };
    if (keywords[0] === query || zhWords[0] === query) {
      primary.push(hit);
    } else if (keywords.includes(query) || zhWords.includes(query)) {
      exact.push(hit);
    } else if (keywords.some((k) => k.includes(query)) || zhWords.some((w) => w.includes(query))) {
      partial.push(hit);
    }
    if (primary.length >= limit) break;
  }
  return primary.concat(exact, partial).slice(0, limit);
}
