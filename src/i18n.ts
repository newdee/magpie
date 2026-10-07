// Tiny built-in i18n: English strings are the keys, `t()` looks them up in
// the zh dictionary when the UI language is Chinese. No library — the app is
// a single window with ~130 strings, and English text doubles as the fallback
// for any key the dictionary misses.

export type LangPref = "auto" | "en" | "zh";
export type Lang = "en" | "zh";

const ZH: Record<string, string> = {
  // quick features: notes, recent opens, selection search, text verbs
  "Enter appends this line to your notes file": "Enter 把这一行追加到你的笔记文件",
  note: "笔记",
  "Recent opens on the empty box": "空搜索框显示最近打开",
  "With nothing typed, each tab lists what you opened from it most recently. Local also lists files in your folders that are new or changed this week.":
    "什么都没输入时，每个 tab 列出你最近从它打开过的条目。本地文件还会列出文件夹里这一周新增或改过的文件。",
  "Search selection shortcut": "划词搜索快捷键",
  "Removed.": "已移除。",
  "Skip git worktrees": "跳过 git worktree",
  "A linked worktree is a second copy of a checkout that is usually indexed already. Skipped when its main checkout is inside an indexed folder; a worktree that is the only copy is still indexed.":
    "linked worktree 是某个 checkout 的第二份拷贝，而那份通常已经在索引里。当它的主 checkout 位于已索引文件夹内时跳过；作为唯一副本的 worktree 照常索引。",
  "Reset to {k}": "恢复为 {k}",
  "Press it in any app to look up the selected text: magpie copies the selection and opens with it as the query.":
    "在任何应用里选中文字后按它：magpie 复制选区，并带着这段文字弹出来。",
  Remove: "移除",
  "Notes file": "笔记文件",
  "MCP server for AI assistants": "MCP 服务（给 AI 助手用）",
  "Lets Claude Code, Cursor and other MCP clients search this index and read indexed text. Loopback only, behind a token, read-only, off by default.":
    "让 Claude Code、Cursor 等 MCP 客户端搜索这份索引、读取已索引的文本。只监听本机回环地址，需令牌，只读，默认关闭。",
  "Copy Claude Code command": "复制 Claude Code 命令",
  "New token": "换新令牌",
  "Listening at": "监听于",
  "starting": "启动中",
  "copied": "已复制",
  "new token issued; add the server again in your clients": "已换新令牌；请在各客户端重新添加该服务",
  "Other clients take the same URL with the header from this command:": "其他客户端用同一 URL 加上这条命令里的请求头：",
  "note buy milk appends one timestamped line to this file.":
    "输入 note 买牛奶，会把一行带时间戳的记录追加到这个文件。",
  "full path, or empty for the default": "完整路径，留空用默认",
  Save: "保存",
  Open: "打开",
  "Ctrl+C copies whatever identifies a row — a path, a URL, a clip's text":
    "Ctrl+C 复制这一行的标识——路径、网址或剪贴条文本",
  "Recent opens on the empty box — switch it on in Settings → General":
    "空搜索框可以列出最近打开——设置 → 通用 里开启",
  "json alone pretty-prints your clipboard — upper, lower, slug, lines, count too":
    "单独输 json 就能格式化剪贴板里的 JSON——还有 upper、lower、slug、lines、count",
  "json min squeezes JSON onto one line, json sort orders its keys; bad JSON shows where it breaks":
    "json min 把 JSON 压成一行，json sort 按键名排序；JSON 有错会指出错在哪",
  "md5, sha1 or sha256 hash your clipboard, or the text you type after them":
    "md5、sha1、sha256 计算剪贴板或后面所跟文字的哈希",
  "jwt decodes the token on your clipboard and tells you when it expires — nothing leaves your machine":
    "jwt 解码剪贴板里的 token 并告诉你何时过期——全程不出本机",
  "snake, kebab, camel, pascal, title: snake MyMacCleaner gives my_mac_cleaner":
    "snake、kebab、camel、pascal、title：snake MyMacCleaner 得到 my_mac_cleaner",
  "qr turns your clipboard into a QR code; Enter copies it as an image":
    "qr 把剪贴板变成二维码，按 Enter 复制成图片",
  "Enter copies the image": "Enter 复制图片",
  "Date math lives in the box: today + 30d, until 2026-10-01, 2026-10-01 - today":
    "输入框会算日期：today + 30d、until 2026-10-01、2026-10-01 - today",
  "Narrow file searches: ext:pdf, >10mb, 7d, in:projects":
    "缩小文件搜索范围：ext:pdf、>10mb、7d、in:projects",
  "note buy milk — one timestamped line into your notes file":
    "note 买牛奶——一行带时间戳的记录进笔记文件",
  "Ctrl+Alt+Space looks up the text you have selected in any app — Option+Shift+Space on a Mac":
    "在任何应用里选中文字按 Ctrl+Alt+Space 直接搜——Mac 上是 Option+Shift+Space",
  // tabs
  "Local Files": "本地文件",
  "GitHub Stars": "GitHub Stars",
  Web: "Web",
  Clipboard: "剪贴板",
  // pills: sorts / scopes
  match: "匹配",
  recent: "最近",
  stars: "星数",
  all: "全部",
  text: "文本",
  images: "图片",
  bookmarks: "书签",
  history: "历史",
  // theme
  auto: "跟随系统",
  light: "浅色",
  dark: "深色",
  // misc option labels
  Unlimited: "无上限",
  "hf-mirror.com (China)": "hf-mirror.com（国内镜像）",
  // relative time
  today: "今天",
  "{n}d": "{n} 天",
  "{n}mo": "{n} 月",
  "{n}y": "{n} 年",
  // progress
  "listing stars… {n}": "拉取 star 列表… {n}",
  "readmes {a}/{b}": "README {a}/{b}",
  "indexing stars {a}/{b}": "索引 star {a}/{b}",
  "scanning files… {n}": "扫描文件… {n}",
  "indexing images {a}/{b}": "索引图片 {a}/{b}",
  "indexing files {a}/{b}": "索引文件 {a}/{b}",
  "indexing videos {a}/{b}": "索引视频 {a}/{b}",
  // footer status
  "error: {e}": "错误：{e}",
  "model download failed, keyword search only (set a mirror in settings)":
    "模型下载失败，仅关键词搜索可用（可在设置中切换镜像）",
  "preparing semantic model (first run downloads ~500 MB)":
    "正在准备语义模型（首次运行下载约 500 MB）",
  "preparing image model (first run downloads ~200 MB)":
    "正在准备图像模型（首次运行下载约 200 MB）",
  "{n} repos indexed": "已索引 {n} 个仓库",
  "{a} bookmarks · {b} history": "{a} 书签 · {b} 历史",
  "{n} clips recorded": "已记录 {n} 条剪贴",
  "clipboard history is off — enable it in settings": "剪贴板历史未开启 — 在设置中开启",
  "{n} files indexed": "已索引 {n} 个文件",
  // search input
  "Searching by image similarity": "按图像相似度搜索",
  "Search bookmarks and browser history": "搜索书签和浏览器历史",
  "Search your clipboard history": "搜索剪贴板历史",
  "Search {n} starred repos": "搜索 {n} 个已 star 仓库",
  "Search your stars": "搜索你的 stars",
  "Describe the image, or pick / drop / paste one": "描述图片，或选择 / 拖入 / 粘贴一张",
  "Search {n} local files, drop or paste an image": "搜索 {n} 个本地文件，可拖入或粘贴图片",
  "Search indexed folders": "搜索已索引的文件夹",
  "Search {s} (Shift+Tab cycles)": "搜索{s}（Shift+Tab 循环）",
  "Sort by {s}": "按{s}排序",
  "Search with an image file": "用图片文件搜索",
  "Re-fetch starred repos": "重新拉取 star 仓库",
  "Re-scan folders": "重新扫描文件夹",
  // collapse bars
  "Connect GitHub to sync your stars": "连接 GitHub 同步你的 stars",
  "No folders indexed yet, add some to search locally": "还没有索引文件夹，添加后即可本地搜索",
  // settings: frame
  Settings: "设置",
  "Back to search (Esc)": "返回搜索 (Esc)",
  Connection: "连接",
  "Appearance & behavior": "外观与行为",
  // settings pages
  General: "通用",
  About: "关于",
  "Shortcuts & tabs": "快捷键与标签页",
  "Quick actions": "快捷功能",
  "Models & integrations": "模型与集成",
  Folders: "文件夹",
  "Images, videos & PDFs": "图片、视频与 PDF",
  Apps: "应用",
  Browsers: "浏览器",
  "Browsers found": "已发现的浏览器",
  "Bookmarks and history are read from every Chromium- and Firefox-based browser on this computer, all profiles. Safari isn't read yet.":
    "读取本机所有 Chromium 内核和 Firefox 内核浏览器的书签与历史（全部 profile）。暂不读取 Safari。",
  "Sync now": "立即同步",
  "No browser data found yet.": "还没有找到浏览器数据。",
  Privacy: "隐私",
  System: "系统",
  // settings: github
  "Paste a new token to replace the current one.": "粘贴新 token 可替换当前的。",
  "A personal access token, no scopes needed — it only reads your public stars.":
    "一个 personal access token，无需任何 scope — 只读取你的公开 stars。",
  "not connected": "未连接",
  Checking: "验证中",
  Connect: "连接",
  "Create one on github.com": "去 github.com 创建",
  "Rebuild star index": "重建 star 索引",
  "Wipe the star index and sync everything from scratch": "清空 star 索引并从头重新同步",
  // settings: indexing
  "Indexed folders": "索引文件夹",
  "Scanned recursively; hidden and gitignored paths are skipped.":
    "递归扫描；跳过隐藏文件及 gitignore 路径。",
  "Add folder": "添加文件夹",
  "Add common places…": "添加常用位置…",
  "Downloaded from": "下载自",
  "Bookmarked in {where}": "收藏在 {where}",
  "Visited {n} times": "访问过 {n} 次",
  "earliest on record {d}": "最早记录 {d}",
  "latest {d}": "最近一次 {d}",
  "Files inside: {n}": "包含 {n} 个文件",
  "A year ago today you starred this": "一年前的今天 star 的",
  "A year ago today you bookmarked this": "一年前的今天收藏的",
  "{n} years ago today you starred this": "{n} 年前的今天 star 的",
  "{n} years ago today you bookmarked this": "{n} 年前的今天收藏的",
  "Starred {n} years ago": "{n} 年前 star 的",
  "Bookmarked {n} years ago": "{n} 年前收藏的",
  "Starred {n} months ago": "{n} 个月前 star 的",
  "Bookmarked {n} months ago": "{n} 个月前收藏的",
  "Don't show this again": "不再提醒这条",
  "Store app": "商店应用",
  "Date in the search box": "搜索框里显示日期",
  Pinned: "置顶",
  "Pinned folders": "置顶文件夹",
  "Up to {n} folders at the top of the empty box, in this order; Enter opens one. They need not be indexed.":
    "最多 {n} 个，按这个顺序显示在空搜索框的最上面，回车打开。不需要在索引文件夹里。",
  "Pin a folder…": "置顶文件夹…",
  "Move up": "上移",
  "Move down": "下移",
  "Unpin the folder": "取消置顶",
  "With nothing typed, the right of the search box shows today's date and weekday, with the Chinese calendar in Chinese.":
    "搜索框为空时，在框内右侧显示今天的日期和星期，中文界面还有农历。",
  "The row from long ago changes every day; Ctrl+K on it stops one from coming back":
    "空搜索框里那条旧物每天换一条；在它上面按 Ctrl+K 可以让它不再出现",
  "Ctrl+K on a result adds it to a workspace; type the name and Enter opens it all — gzxc or ws lists them":
    "在结果上按 Ctrl+K 可以加入工作现场；输入名字回车全部打开，gzxc 或 ws 列出所有工作现场",
  "Scripts in the scripts folder become commands (Settings → General); Raycast's script commands work as they are":
    "脚本文件夹里的脚本会变成命令（设置 → 通用）；Raycast 的脚本命令可以直接用",
  Workspace: "工作现场",
  "Opens {n}": "打开 {n} 项",
  "Open everything": "全部打开",
  "Take “{title}” out": "移出「{title}」",
  "Delete this workspace": "删除这个工作现场",
  "Add to workspace “{name}”": "加入工作现场「{name}」",
  "Add to a new workspace…": "加入新的工作现场…",
  "“{name}” now holds {n}": "「{name}」里现在有 {n} 项",
  "New workspace “{name}”": "新建工作现场「{name}」",
  "Type a name for the new workspace": "输入新工作现场的名字",
  "Enter puts “{title}” in it · Esc cancels": "按 Enter 把「{title}」放进去 · Esc 取消",
  "Some could not open: {list}": "有几项打不开：{list}",
  Script: "脚本",
  "Script command": "脚本命令",
  "Script commands": "脚本命令",
  "Scripts folder": "脚本文件夹",
  "A script in this folder becomes a command: put its name at the top in a comment, as # @raycast.title My Command. Raycast's script commands work as they are.":
    "放进这个文件夹的脚本会变成命令：在开头用注释写上名字，例如 # @raycast.title 我的命令。Raycast 的脚本命令可以直接用。",
  "{n} scripts found": "找到 {n} 个脚本",
  "Open the scripts folder": "打开脚本文件夹",
  "Use another folder…": "换一个文件夹…",
  "Back to the default folder": "用回默认文件夹",
  "Type {what} after the name, then Enter": "在名字后面输入 {what}，再按 Enter",
  "Nothing on this computer runs this script": "这台电脑上没有能运行它的程序",
  "Running {title}…": "正在运行 {title}…",
  "Stopped after a minute": "运行超过一分钟，已停止",
  "Exited with code {code}": "出错退出（代码 {code}）",
  Done: "完成",
  "(no output)": "（没有输出）",
  "Enter copies the output · Esc closes": "按 Enter 复制输出 · Esc 关闭",
  "From long ago on the empty box": "旧物重现",
  "With nothing typed, a bookmark and a starred repo you kept long ago come up first, different ones each day: Web shows the bookmark, Stars the repo, Local both. {key} on one can stop it from coming back.":
    "搜索框为空时，最上面显示很久以前收藏的书签和 star 的仓库，每天换：Web 页签显示书签，Stars 页签显示仓库，本地文件页签两个都有。在它上面按 {key} 可以让它不再出现。",
  "{exts} already open with {app} above, not here": "{exts} 已在上面设为用 {app} 打开，这一行对它们不生效",
  "{exts} open with {app} below, not here": "{exts} 会用下面那行的 {app} 打开，这一行对它们不生效",
  "password-protected, names only": "有密码，只能看到文件名",
  Edit: "编辑",
  "Edit with {app}": "用 {app} 编辑",
  "Editing apps": "编辑用的软件",
  "{key} on a file opens it in the app picked for its type. Other types use the system's own way to edit.":
    "在文件上按 {key}，用为它的类型选定的软件打开；其他类型用系统自带的编辑方式。",
  "Add a type": "添加类型",
  "File types": "文件类型",
  "Editing app": "编辑软件",
  "System default": "系统默认",
  "(not installed)": "（未安装）",
  "Where downloads, screenshots and files from chat apps usually land. Tick the ones to index.":
    "下载、截图和聊天软件收到的文件通常存在这些地方。勾选要索引的。",
  "Looking for common places…": "正在查找常用位置…",
  "No common places found on this computer.": "这台电脑上没找到常用位置。",
  Downloads: "下载",
  Desktop: "桌面",
  Screenshots: "截图",
  "Files received in WeChat": "微信收到的文件",
  "Files received in QQ": "QQ 收到的文件",
  "{n} files": "{n} 个文件",
  "{n}+ files": "{n}+ 个文件",
  Added: "已添加",
  "Already in an indexed folder": "已包含在索引的文件夹里",
  "Holds an indexed folder; remove that one first": "里面有已索引的文件夹，先移除那个",
  "Add {n}": "添加 {n} 个",
  Add: "添加",
  "No folders yet.": "还没有文件夹。",
  "The folder list failed to load — please report this with the error below.":
    "文件夹列表加载失败 — 请附下方错误反馈。",
  "Rebuild this folder's index from scratch": "从零重建该文件夹的索引",
  "Remove from index": "从索引中移除",
  "Max file size": "文件大小上限",
  "Video shot search": "视频镜头搜索",
  "Search videos by name, or describe a scene": "按文件名搜视频，或描述一个画面",
  "system install": "系统已装",
  downloaded: "已下载",
  videos: "视频",
  "Image text (OCR)": "图片文字（OCR）",
  "Reads the text inside indexed images and video frames (screenshots, scans, subtitles) so you can search it — video hits jump to the moment the text appears. Off by default; enabling downloads a small model (~15 MB).":
    "识别已索引图片和视频帧里的文字（截图、扫描件、字幕），让它们可以被搜到——视频命中直接跳到文字出现的时刻。默认关闭；开启后下载一个小模型（约 15 MB）。",
  "OCR model": "OCR 模型",
  "Scanned PDFs": "扫描版 PDF",
  "Also read pages of PDFs that have no text layer. Large scans take a while, so this is your call.":
    "同时识别没有文字层的 PDF 页面。大部头扫描件较耗时，开不开由你。",
  "File changes": "文件变化",
  "Changes inside indexed folders reach the index within seconds, without waiting for the next full walk.":
    "索引文件夹里的改动几秒内进索引，不用等下一次全量遍历。",
  "Watching": "正在监听",
  "folders": "个文件夹",
  "Full rescan": "全量重扫",
  "Every so often all folders are walked again, so anything the watcher missed still lands. Off leaves it to the watcher and to startup.":
    "每隔一段时间把所有文件夹重新遍历一遍，监听漏掉的也能补上。关闭则只靠监听和启动时那一次。",
  "min": "分钟",
  "Rescan now": "立即重扫",
  "rescanning": "重扫中",
  "Indexing threads": "索引线程数",
  "CPU threads each model (text, image, OCR) may use while indexing. Fewer keeps the machine responsive; all cores finishes a first index sooner. Applies right away.":
    "索引时每个模型（文本、图片、OCR）最多占用的 CPU 线程数。少一点前台更流畅，全部核心则首次索引更快。改完立即生效。",
  "all cores": "全部核心",
  "Decode limits": "解码限制",
  "Caps ffmpeg while indexing videos, so it never owns the machine. Hardware decode falls back to software if the driver fails.":
    "限制视频索引时 ffmpeg 的占用，后台跑不打扰前台。硬件解码失败自动回退软解。",
  "auto threads": "自动线程",
  "hw decode": "硬解",
  "Hardware decode (falls back to software on failure)": "硬件解码（失败自动回退软解）",
  "Videos in your folders are split into shots; each shot is searchable by image or description. Needs ffmpeg (auto-downloaded if missing).":
    "文件夹里的视频按镜头切分；每个镜头可用图片或文字描述搜索。需要 ffmpeg（缺失时自动下载）。",
  Video: "视频",
  "Larger files index by name only. Changing rebuilds.": "超限文件仅按文件名索引。修改后重建。",
  "Model download source": "模型下载源",
  "Pick the mirror if huggingface.co is unreachable from your network.":
    "网络连不上 huggingface.co 时选镜像。",
  "Semantic model": "语义模型",
  "Image model": "图像模型",
  ready: "就绪",
  "downloading (~500 MB, first run)…": "下载中（首次约 500 MB）…",
  "downloading (~200 MB, first run)…": "下载中（首次约 200 MB）…",
  // settings: appearance & behavior
  Theme: "主题",
  Language: "语言",
  "Palette and settings text; the tray menu follows.": "浮窗与设置文案；托盘菜单跟随。",
  "Pinyin app matching": "拼音匹配应用",
  "Launch tips": "启动小贴士",
  "A one-line tip below the empty search box, fresh on every summon.":
    "空搜索框下方的一行使用技巧，每次唤出随机一条。",
  // launch tips (extras.ts TIPS)
  "Ctrl+P pins a clip — it sorts first and never gets pruned":
    "Ctrl+P 钉住剪贴条——置顶显示且永不被清理",
  "The query box is a calculator: 3*(5+2)^2 — Enter copies the answer":
    "输入框就是计算器：3*(5+2)^2——Enter 复制结果",
  "Type : then a keyword to find emoji — :fire or :火":
    "输入 : 加关键词找表情——:fire 或 :火",
  "gh magpie searches GitHub straight from the box — prefixes are editable in settings":
    "gh magpie 直达 GitHub 搜索——前缀规则在设置里可编辑",
  "Shift+Enter pastes a clip straight into the app you came from":
    "Shift+Enter 把剪贴条直接粘贴回你刚才所在的应用",
  "→ previews the selected result — file text, images, a video's shots":
    "→ 预览选中结果——文件正文、图片大图、视频镜头条",
  "Ctrl+Shift+C puts the file itself on the clipboard — paste it as an attachment":
    "Ctrl+Shift+C 复制文件本体——粘贴出去就是附件",
  "Drop or paste an image to search your files by visual similarity":
    "拖入或粘贴一张图，按画面相似度搜你的文件",
  "ts 1700000000 turns a unix timestamp into local time":
    "ts 1700000000 把 unix 时间戳转成本地时间",
  "#ff6600 shows the color with rgb/hsl — Enter copies the hex":
    "#ff6600 显示色块和 rgb/hsl——Enter 复制 hex",
  "pwd 24 generates a cryptographically random password":
    "pwd 24 生成一枚密码学随机密码",
  "Enable OCR in settings — words inside screenshots and videos become searchable":
    "设置里开启 OCR——截图和视频里的文字都能搜",
  "Enter on a video hit starts playback right at the matched scene":
    "视频命中按 Enter，直接从匹配场景开始播放",
  "App names match by pinyin too — wx finds 微信, vsc finds VS Code":
    "应用名支持拼音——wx 找到微信，vsc 找到 VS Code",
  "Ctrl+Enter hands your query to the browser — URLs open directly":
    "Ctrl+Enter 把输入交给浏览器——网址直接打开",
  "Ctrl+1 to Ctrl+9 jump straight to a tab (⌘ on a Mac)": "Ctrl+1 到 Ctrl+9 直接切到对应 tab（Mac 上用 ⌘）",
  "Ctrl+K on a result lists everything you can do with it (⌘K on a Mac)":
    "在结果上按 Ctrl+K 列出能对它做的所有操作（Mac 上用 ⌘K）",
  "Ctrl+K on a PDF copies or saves it as Markdown, every page in order":
    "在 PDF 上按 Ctrl+K，可以把它复制或另存为 Markdown，所有页按顺序保留",
  "Copy as Markdown": "复制为 Markdown",
  "Save as Markdown…": "另存为 Markdown…",
  "Converting the PDF…": "正在转换 PDF…",
  "Copied {n} characters of Markdown": "已复制 {n} 个字符的 Markdown",
  "Saved {name}": "已保存 {name}",
  "Selection search needs Accessibility: allow magpie in System Settings → Privacy & Security → Accessibility":
    "划词搜索需要辅助功能权限：在 系统设置 → 隐私与安全性 → 辅助功能 里允许 magpie",
  "Open System Settings": "打开系统设置",
  "On Wayland, magpie bound {key} through {desktop}.": "Wayland 下，magpie 已通过 {desktop} 绑定了 {key}。",
  "niri takes no bindings at run time. Add these lines to ~/.config/niri/config.kdl:":
    "niri 不支持在运行时添加快捷键。把下面几行加到 ~/.config/niri/config.kdl：",
  "Binding the key through {desktop} failed ({error}). Bind a key to this command by hand:":
    "通过 {desktop} 绑定快捷键失败（{error}）。请手动把一个快捷键绑定到这条命令：",
  "On Wayland the desktop owns the keyboard: in its keyboard settings, add a custom shortcut that runs this command.":
    "Wayland 下快捷键由桌面管理：在系统的键盘设置里添加一个自定义快捷键，让它运行这条命令。",
  Copied: "已复制",
  Dismiss: "关闭",
  "kill chrome lists running Chrome processes; Enter twice ends one":
    "输入 kill chrome 列出运行中的 Chrome 进程；按两次 Enter 结束",
  "lock, sleep or dark mode run straight from the box": "锁屏、睡眠、深色模式，直接在搜索框里执行",
  "tokyo time, or 3pm pst to beijing: time zones work offline":
    "东京时间、3pm pst to beijing：时区换算离线可用",
  "Lock Screen": "锁屏",
  Sleep: "睡眠",
  Restart: "重启",
  "Shut Down": "关机",
  "Empty Trash": "清空废纸篓",
  "Empty Recycle Bin": "清空回收站",
  "Toggle Dark Mode": "切换深色模式",
  "System command": "系统命令",
  Command: "命令",
  Process: "进程",
  "Press Enter again to confirm": "再按一次 Enter 确认",
  "Press Enter again to end this process": "再按一次 Enter 结束这个进程",
  actions: "操作",
  "Show in folder": "在文件夹中显示",
  "Open with default app": "用默认应用打开",
  "Copy path": "复制路径",
  "Copy file": "复制文件",
  Play: "播放",
  "Run as administrator": "以管理员身份运行",
  "Open in browser": "在浏览器中打开",
  "Copy URL": "复制网址",
  "Copy clone command": "复制 clone 命令",
  "Copy as Markdown link": "复制为 Markdown 链接",
  Copy: "复制",
  "Paste into the previous app": "粘贴到之前的应用",
  Pin: "钉住",
  Unpin: "取消钉住",
  "Delete from history": "从历史中删除",
  Run: "执行",
  "End process": "结束进程",
  "Copy PID": "复制 PID",
  "Unit conversion lives in the box: 100 mb to gb, 32 f to c":
    "单位换算就在输入框：100 mb to gb、32 f to c",
  "Give apps aliases in settings: proxy = clash":
    "设置里给应用起别名：proxy = clash",
  "Shift+Tab cycles the local scope: all / text / images / videos":
    "Shift+Tab 循环本地范围：全部 / 文本 / 图片 / 视频",
  "Export your whole setup from Settings → About — the GitHub token stays out":
    "设置 → 关于 可一键导出全部配置——GitHub token 除外",
  "Latin queries match Chinese app names by full pinyin or initials (wx → 微信).":
    "拉丁输入按全拼或首字母匹配中文应用名（wx → 微信）。",
  "App aliases": "应用别名",
  "App folders": "应用文件夹",
  "Type a path such as D:\\Projects or ~/Downloads: Enter opens the folder":
    "输入路径，比如 D:\\Projects 或 ~/Downloads，回车直接打开文件夹",
  "wifi, 蓝牙, display or 卸载 jumps straight to that page of the system settings":
    "输入 wifi、蓝牙、display、卸载，直达系统设置里对应的那一页",
  "top lists what uses the most memory, cpu what uses the most CPU; Enter twice ends one":
    "top 列出最占内存的进程，cpu 列出最占 CPU 的，回车两次结束它",
  "vol 40 sets the volume, vol +10 turns it up, mute silences it":
    "vol 40 调音量，vol +10 调大一点，mute 静音",
  "农历 shows today in the Chinese calendar; 农历 中秋 or 农历 八月十五 tells you the date; 节气 gives the next solar term":
    "输入 农历 看今天的农历；农历 中秋、农历 八月十五 查是公历哪天；节气 看下一个节气",
  // typed paths, volume, top, settings pages
  "open folder": "打开文件夹",
  "show file in folder": "在文件夹中显示",
  volume: "音量",
  "set volume": "调整音量",
  mute: "静音",
  unmute: "取消静音",
  "already muted": "已经是静音",
  "not muted": "没有静音",
  "Enter opens it": "Enter 打开",
  "Enter sets it": "Enter 设置",
  "Enter does it": "Enter 执行",
  "No audio output device found": "没有找到音频输出设备",
  "Volume set to {n}%": "音量已调到 {n}%",
  "Sound muted": "已静音",
  "Sound on": "已取消静音",
  Path: "路径",
  Sound: "声音",
  Calendar: "农历",
  "Solar term reminders": "节气提醒",
  "On the first day of a solar term a notification brings two lines of verse for it; Settings → General turns it off":
    "交节当天会弹一条通知，配一联应节的古诗；不想要可以在 设置 → 通用 里关掉",
  "On the first day of each of the 24 solar terms, a notification after 9 am with two lines of classical verse, which also take turns with the tips that day. Type 节气 any time to see the next one.":
    "二十四节气交节当天，上午 9 点后弹一条通知，配一联古诗，当天的小贴士里也会轮流出现。随时输入 节气 可以看下一个节气。",
  "System settings": "系统设置",
  "{n} of them, {size} in all": "同名 {n} 个，共 {size}",
  "Wi-Fi Settings": "WLAN 设置",
  "Bluetooth Settings": "蓝牙设置",
  "Network Settings": "网络设置",
  "Display Settings": "显示设置",
  "Sound Settings": "声音设置",
  "Power & Battery": "电源和电池",
  Battery: "电池",
  "Default Apps": "默认应用",
  "Installed Apps": "已安装的应用（卸载）",
  "Keyboard Settings": "键盘设置",
  "Trackpad Settings": "触控板设置",
  "Mouse & Touchpad": "鼠标和触摸板",
  "Notification Settings": "通知设置",
  "Date & Time Settings": "日期和时间设置",
  "Language & Region": "语言和区域",
  "Windows Update": "Windows 更新",
  "Software Update": "软件更新",
  "Privacy & Security": "隐私和安全性",
  Printers: "打印机",
  Wallpaper: "壁纸",
  Storage: "存储空间",
  "Passwords and keys in the clipboard history show starred; Show on the row reveals one":
    "剪贴板历史里的密码和密钥打着星号，点行尾的「显示」才露出全文",
  "Apps on an external drive: add their folder in Settings → Local Files → App folders":
    "外接硬盘上的应用：在 设置 → 本地文件 → 应用文件夹 里把目录加进来",
  "Apps kept outside the usual places, such as on an external drive. Only the apps in a folder are listed, not the files inside them.":
    "装在别处的应用，比如外接硬盘上的，可以把所在文件夹加进来。只收应用本身，里面的零碎文件不收。",
  Show: "显示",
  Hide: "隐藏",
  "Show the full text": "显示完整内容",
  "Hide the full text": "重新隐藏",
  Cancel: "取消",
  "Change token": "更换 token",
  Disconnect: "断开连接",
  "Click again to disconnect": "再点一次断开",
  "Connected. Your stars sync from this account.": "已连接，星标从这个账号同步。",
  "The token and the star index are removed from this computer. Nothing changes on GitHub.":
    "会删掉本机保存的 token 和星标索引，GitHub 上的东西不受影响。",
  "One rule per line: alias = app name. The alias matches like a second name (pinyin included).":
    "一行一条：别名 = 应用名。别名当作第二名字参与匹配（含拼音）。",
  "Save aliases": "保存别名",
  "Web shortcuts": "网页快搜",
  "One rule per line: prefix = URL with {q}. Type the prefix, a space, and your query — Enter opens the search.":
    "一行一条：前缀 = 带 {q} 的 URL。输入前缀 + 空格 + 关键词，Enter 直达搜索。",
  "Save shortcuts": "保存快搜",
  "Search {s} for": "用 {s} 搜索",
  "Enter copies the result": "Enter 复制结果",
  calc: "计算",
  "No matching emoji": "无匹配表情",
  "Summon shortcut": "唤出快捷键",
  Currently: "当前",
  "Click and press a new combination; Backspace clears. OS-reserved chords (like ⌘Space) can't be captured.":
    "点击后按下新组合键；Backspace 清除。系统保留组合（如 ⌘Space）无法捕获。",
  "press keys…": "按下按键…",
  Apply: "应用",
  saved: "已保存",
  "use Ctrl/Alt/Win plus a key, or an F-key": "请用 Ctrl/Alt/Win 加一个键，或单独 F 键",
  Clear: "清除",
  "Reset to Alt+Space": "重置为 Alt+Space",
  Tabs: "标签页",
  "Tick which sources appear as tabs (at least one stays on). Drag the handle (or use the arrows) to reorder; ★ marks the tab that opens on launch.":
    "勾选要显示为标签页的源（至少保留一个）。拖动手柄（或用箭头）排序；★ 为启动时打开的标签页。",
  "At least one tab must stay visible": "至少保留一个可见标签页",
  "Show {s}": "显示 {s}",
  "Hide {s}": "隐藏 {s}",
  "Opens on launch": "启动时打开",
  "Make this the launch tab": "设为启动标签页",
  // settings: privacy
  "Clipboard history": "剪贴板历史",
  "Recorded locally, searchable in the Clipboard tab. Password-manager secrets are never stored.":
    "仅记录在本地，可在剪贴板标签页搜索。密码管理器的机密内容永不记录。",
  off: "关",
  on: "开",
  "Welcome to magpie": "欢迎使用 magpie",
  "One search box for your apps, files, bookmarks, starred repos and clipboard. Everything stays on this computer.":
    "一个搜索框，找应用、文件、书签、收藏的仓库和剪贴板。所有东西都只留在这台电脑上。",
  "Open it from anywhere with": "在任何地方按",
  ", type, and press Enter.": "唤出，输入，回车。",
  "What it finds": "能找到什么",
  "Apps, by name, initials or pinyin: vsc, wx": "应用：按名字、首字母或拼音，比如 vsc、wx",
  "Files in the folders you add in settings, text and images alike": "在设置里添加的文件夹中的文件，文字和图片都能搜",
  "Bookmarks and history from your browsers": "浏览器的书签和历史记录",
  "Your starred GitHub repos, once you connect": "连上 GitHub 后，你收藏的仓库",
  "What you copy, if you turn the clipboard history on": "打开剪贴板历史后，你复制过的内容",
  "switches between them. The box is also a calculator, a unit converter and more; the tips below it show how.":
    "在它们之间切换。搜索框还能当计算器、单位换算用，底下的小提示会告诉你怎么用。",
  "Search by meaning?": "要不要按意思搜索？",
  "With semantic search, “unpaid bill from the plumber” finds the plumbing invoice even when no word matches. It runs a model on this computer: about 500 MB to download once, and some memory while magpie runs.":
    "开了语义搜索，搜「没付的水管工账单」，就算一个字都对不上，也能找到那张水管维修发票。它在这台电脑上跑一个模型：第一次要下载约 500 MB，运行时会占一些内存。",
  "Without it, search matches words, and apps, tools and everything else work the same. You can change this any time in Settings → General.":
    "不开的话，搜索按字词匹配，启动应用、各种小工具都照常能用。之后随时可以在 设置 → 通用 里改。",
  "Turn it on (downloads ~500 MB)": "开启（下载约 500 MB）",
  "Not now": "先不开",
  Back: "上一步",
  Next: "下一步",
  "Semantic search": "语义搜索",
  "Before a game, pause semantic search from the tray to free its memory": "打游戏前，可以在托盘里暂停语义搜索，把内存让出来",
  "Finds things by meaning with a model that runs on this computer (~500 MB download, some memory while running). Off: search matches words; apps and tools work the same.":
    "用本机运行的模型按意思找东西（下载约 500 MB，运行时占一些内存）。关掉后按字词匹配，应用和小工具照常可用。",
  "Resume semantic search": "恢复语义搜索",
  "Pause it to free memory, e.g. for a game": "暂停以释放内存，比如要打游戏时",
  "paused, memory freed": "已暂停，内存已释放",
  "semantic search paused, keyword search only": "语义搜索已暂停，只按关键词搜索",
  "Keep at most": "最多保留",
  unlimited: "无限制",
  "Keep for": "保留时长",
  "7 days": "7 天",
  "30 days": "30 天",
  forever: "永久",
  "Clear history": "清空历史",
  "Delete every recorded clip permanently.": "永久删除所有已记录的剪贴内容。",
  // settings: system
  Updates: "更新",
  "Version {v} is available.": "新版本 {v} 可用。",
  "You are on the latest version.": "已是最新版本。",
  "Installed in place; your index and settings are kept.": "原地安装；索引和设置保持不变。",
  "Update & restart": "更新并重启",
  "Check now": "检查更新",
  "Checking…": "检查中…",
  Logs: "日志",
  "Local activity log (errors, model/ffmpeg status) — attach it to a bug report. Queries are never logged.":
    "本地运行日志（错误、模型/ffmpeg 状态）——报 issue 时附上。搜索内容永不记录。",
  "Open log folder": "打开日志文件夹",
  "Settings file": "设置文件",
  "Everything except the GitHub token — move your setup to another machine.":
    "除 GitHub token 外的全部配置——换机器一键搬家。",
  Export: "导出",
  Import: "导入",
  // empty states
  "No matches in your stars": "stars 中无匹配",
  "No matching bookmarks or history": "无匹配的书签或历史",
  "No matching clips": "无匹配的剪贴内容",
  "No matches in indexed folders": "索引文件夹中无匹配",
  // result rows
  Application: "应用程序",
  App: "应用",
  Bookmark: "书签",
  History: "历史",
  "Maybe related": "可能相关",
  "search in browser": "在浏览器中搜索",
  // opening elsewhere, the trash, ocr on the clipboard
  "After a screenshot, type ocr and the text on it is ready to copy": "截图之后输入 ocr，图上的字就能直接复制",
  "port 3000 shows what is listening on it; Enter twice ends it": "port 3000 看看谁占着这个端口，按两次回车就能结束它",
  "Ctrl+K on a file opens its folder in a terminal, or the file in your editor": "在文件上按 Ctrl+K，可以在终端里打开它的文件夹，或用编辑器打开它",
  ":sym lists symbols like → × ¥ ⌘; ip shows your local network address": ":sym 列出 → × ¥ ⌘ 这类符号；ip 显示本机局域网地址",
  "Ctrl+K on a file can also move it to the Recycle Bin (the Trash on a Mac); Enter twice confirms": "在文件上按 Ctrl+K 还能把它移到回收站（Mac 上是废纸篓），按两次回车确认",
  "dl lists the newest files in your Downloads folder": "dl 列出「下载」文件夹里最新的文件",
  "大写 1234.56 writes an amount in Chinese capitals; py 重庆 gives its pinyin": "大写 1234.56 写出金额大写；py 重庆 给出拼音",
  "timer 25m standup starts a countdown; a notification comes when it ends": "timer 25m 开会 开始倒计时，结束时弹出通知",
  "diff compares the last two texts you copied, line by line": "diff 逐行对比最近两次复制的文字",
  "qr with an image on the clipboard reads the QR code in it": "剪贴板里是图片时，qr 读出图中的二维码",
  "sys shows CPU, memory and free disk space; pick, dice and coin decide for you": "sys 看 CPU、内存和磁盘空间；pick、dice、coin 帮你拿主意",
  // small local tools: labels and messages of the top row
  clipboard: "剪贴板",
  "Chinese capitals": "金额大写",
  pinyin: "拼音",
  "half-width": "半角",
  "full-width": "全角",
  "unicode decoded": "Unicode 已解码",
  "unicode escaped": "Unicode 已转义",
  "html decoded": "HTML 已解码",
  "html escaped": "HTML 已转义",
  coin: "抛硬币",
  dice: "骰子",
  system: "系统状况",
  "QR code in the copied image": "复制的图片里的二维码",
  "Turn on clipboard history to compare the last two things you copied": "打开剪贴板历史后，才能对比最近两次复制的内容",
  "Copy two pieces of text first; diff compares the last two": "先复制两段文字，diff 会对比最近的两段",
  "The last two copies are the same": "最近两次复制的内容完全相同",
  "No QR code found in the copied image": "复制的图片里没有找到二维码",
  "Timer set: {clock}": "已开始倒计时 {clock}",
  "Stopped {n} timers": "已停止 {n} 个倒计时",
  "diff · {r} removed, {a} added": "对比 · 删去 {r} 行，新增 {a} 行",
  "No timer running. Try timer 25m": "没有正在运行的倒计时，试试 timer 25m",
  "{n} running · Enter stops them all": "{n} 个进行中 · Enter 全部停止",
  "Save image…": "图片另存为…",
  "Last two copies compared": "最近两次复制的对比",
  "… {n} more lines": "…… 还有 {n} 行",
  "Enter starts the timer; a notification comes when it ends": "Enter 开始倒计时，结束时会弹出系统通知",
  "Open in terminal": "在终端中打开",
  "Open in {app}": "用 {app} 打开",
  "Move to Trash": "移到废纸篓",
  "Move to Recycle Bin": "移到回收站",
  "Moved {name} to the Trash": "已将 {name} 移到废纸篓",
  "Moved {name} to the Recycle Bin": "已将 {name} 移到回收站",
  "Text in the copied image · {n} lines": "复制的图片里的文字 · {n} 行",
  "Turn on Image text (OCR) in settings to read text from images": "在设置里打开「图片文字（OCR）」后，才能读取图片里的文字",
  "The OCR model is still getting ready": "OCR 模型还在准备中",
  "No text found in the copied image": "复制的图片里没有找到文字",
  "No image on the clipboard": "剪贴板里没有图片",
  "The clipboard holds something marked confidential": "剪贴板里是标记为机密的内容",
  "Found by meaning; no word in common with what you typed": "按意思找到的，和你输入的词没有重合",
  "{n} lines": "{n} 行",
  archived: "已归档",
  "added {d}": "添加于 {d}",
  "modified {d}": "修改于 {d}",
  "last push {d}": "最后推送 {d}",
  // footer hints
  copy: "复制",
  open: "打开",
  source: "切换源",
  select: "多选",
  delete: "删除",
  scope: "范围",
  sort: "排序",
  web: "网页",
  settings: "设置",
  preview: "预览",
  paste: "粘贴",
  "close preview": "收起预览",
  "Clipboard entry": "剪贴内容",
  Image: "图片",
  Shots: "镜头",
  "No preview": "无可预览内容",
  "That app has moved or been removed; the app list was refreshed.":
    "这个应用已被移动或删除，应用列表已刷新。",
  "Launch at login": "开机启动",
  "Start magpie in the tray when you log in.": "登录系统后在托盘里启动 magpie。",
  "Hide on click-out": "失焦时隐藏",
  "The palette goes away when another window takes focus. Turn off to drag files in from other windows.":
    "点到别的窗口，面板就收起。要从其他窗口拖文件进来时请关闭。",
  "Jump to a tab": "数字键切换 tab",
  "A modifier plus the tab's number opens it directly, in the order the tabs are shown.":
    "修饰键加 tab 的序号直接切过去，序号按 tab 栏显示的顺序。",
  "not loaded; loads once an image, video or image clip is indexed":
    "未加载；索引到图片、视频或剪贴板图片后才加载",
};

export function resolveLang(pref: LangPref): Lang {
  if (pref === "en" || pref === "zh") return pref;
  try {
    return navigator.language?.toLowerCase().startsWith("zh") ? "zh" : "en";
  } catch {
    return "en";
  }
}

export function loadLangPref(): LangPref {
  try {
    const saved = localStorage.getItem("magpie.lang");
    if (saved === "en" || saved === "zh" || saved === "auto") return saved;
  } catch {
    /* default below */
  }
  return "auto";
}

let current: Lang = resolveLang(loadLangPref());

/// Must be called BEFORE the re-render that uses the new language (t() reads
/// module state synchronously during render).
export function setLang(l: Lang) {
  current = l;
}

export function t(s: string): string {
  return current === "zh" ? (ZH[s] ?? s) : s;
}

/// The interface language in use, for what is worded differently rather
/// than translated (the date line: the Chinese calendar only in Chinese).
export function currentLang(): Lang {
  return current;
}

export function tf(s: string, vars: Record<string, string | number>): string {
  let out = t(s);
  for (const [k, v] of Object.entries(vars)) out = out.split(`{${k}}`).join(String(v));
  return out;
}
