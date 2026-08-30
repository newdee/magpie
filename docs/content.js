// All page copy, in both languages. One source of truth so the two versions
// can never drift apart: the page swaps text nodes, it never reloads.
window.CONTENT = {
  en: {
    "nav.tour": "Tour",
    "hero.eyebrow": "Local-first · Open source",
    "stat.sources": "sources",
    "stat.local": "on-device",
    "stat.embed": "query embed",
    "stat.platforms": "platforms",
    "nav.download": "Download",
    "hero.title": "Everything you saved.<br />One keystroke.",
    "hero.lede":
      "Files, screenshots, videos, stars, bookmarks, clipboard. Describe what you half-remember and magpie finds it. All of it runs on your own machine.",
    "hero.download": "Download",
    "hero.source": "View source",
    "hero.meta": "Free · MIT · Windows / macOS / Linux",
    "g1.title": "What it searches",
    "g1.sub": "Six sources, one box. <kbd>Tab</kbd> moves between them.",
    "g2.title": "It reads what's inside",
    "g2.sub":
      "Filenames are the easy part. magpie also reads pixels, video frames, and the words printed on them.",
    "g3.title": "A toolbelt in the query box",
    "g3.sub": "Some answers don't need search results. <kbd>Enter</kbd> copies whatever comes up.",
    "g4.title": "Everyday feel",
    "g4.sub": "The small stuff that decides whether a launcher survives its first week.",
    "rest.title": "More it can do",
    "rest.sub": "Smaller pieces you'll run into once magpie is part of your day.",
    "g5.title": "Privacy you can verify yourself",
    "stars.title": "Star history",
    "stars.cta": "Add yours",
    "close.title": "Press a hotkey. Find the thing.",
    "close.docs": "Full documentation",
    "footer.built": "Built with Rust + Tauri.",
  },
  zh: {
    "nav.tour": "功能",
    "hero.eyebrow": "本地优先 · 开源",
    "stat.sources": "数据源",
    "stat.local": "本机运行",
    "stat.embed": "查询嵌入",
    "stat.platforms": "平台",
    "nav.download": "下载",
    "hero.title": "你存过的一切，<br />一个快捷键找回来。",
    "hero.lede":
      "文件、截图、视频、star、书签、剪贴板。凭你还记得的那点印象去搜，不用想起全名。全部跑在你自己的机器上。",
    "hero.download": "下载",
    "hero.source": "查看源码",
    "hero.meta": "免费 · MIT · Windows / macOS / Linux",
    "g1.title": "能搜什么",
    "g1.sub": "六个数据源共用一个输入框，<kbd>Tab</kbd> 键来回切。",
    "g2.title": "它看得懂内容",
    "g2.sub": "文件名只是最表层。画面、视频帧、印在上面的字，magpie 都读得到。",
    "g3.title": "输入框里的工具带",
    "g3.sub": "有些答案根本不用搜。<kbd>Enter</kbd> 直接复制。",
    "g4.title": "日常手感",
    "g4.sub": "决定一个启动器能不能活过第一周的那些小事。",
    "rest.title": "还有这些",
    "rest.sub": "用顺手之后会碰到的那些小功能。",
    "g5.title": "隐私这件事，你可以自己查",
    "stars.title": "Star 增长",
    "stars.cta": "来一颗",
    "close.title": "按下快捷键，找到那个东西。",
    "close.docs": "完整文档",
    "footer.built": "用 Rust + Tauri 构建。",
  },
};

// Feature cards: one screenshot each, grouped by section id.
window.FEATURES = {
  g1: [
    {
      img: "files.png",
      tag: "Local files",
      en: {
        t: "Local files, full text",
        d: "Add the folders you actually work in. magpie finds any file by name, and reads inside ~80 text formats plus PDF and Office docs. Matching lines come back highlighted.",
      },
      zh: {
        t: "本地文件全文检索",
        d: "把你天天在用的目录加进来。任何文件都能按名字找到，约 80 种文本格式外加 PDF、Office 文档还能搜内容，命中的那行会高亮。",
      },
    },
    {
      img: "apps.png",
      tag: "App launcher",
      en: {
        t: "An app launcher, too",
        d: "Type an app name and hit Enter. Prefixes work, so do substrings and acronyms like vsc. Chinese names match by pinyin, and you can teach it your own aliases.",
      },
      zh: {
        t: "同时是应用启动器",
        d: "输入应用名回车就启动。前缀、中间几个字、首字母缩写（比如 vsc）都认。中文名走拼音，还能自己加别名。",
      },
    },
    {
      img: "stars.png",
      tag: "GitHub stars",
      en: {
        t: "GitHub stars you forgot",
        d: "Your whole starred list, READMEs and all. magpie embeds them in chunks, so one sentence buried on page four is enough to find the repo again. Sort by relevance, date, or star count.",
      },
      zh: {
        t: "被你遗忘的 GitHub stars",
        d: "整个 star 列表连 README 一起收进来，分块嵌入之后，藏在文档第四页的一句话也够把项目翻出来。可按相关度、收藏时间、星数排序。",
      },
    },
    {
      img: "web.png",
      tag: "Bookmarks + history",
      en: {
        t: "Bookmarks and history",
        d: "magpie reads your browsers' own files: every Chromium browser plus Firefox, all profiles. Bookmarks and history come back in one list, with the pages you open most ranked higher.",
      },
      zh: {
        t: "书签与浏览历史",
        d: "直接读浏览器自己的文件：Chromium 系全家桶加 Firefox，所有 profile 都覆盖。书签和历史在一个列表里出，你常开的页面排前面。",
      },
    },
    {
      img: "clipboard.png",
      tag: "Clipboard",
      en: {
        t: "Clipboard history",
        d: "Off until you turn it on, and stored locally. Copy something back, paste it into the app you were just in, or pin the lines you use every day. Anything your password manager flags as secret never gets stored.",
      },
      zh: {
        t: "剪贴板历史",
        d: "默认不开，开了也只存本地。可以把某条复制回去、粘贴回你刚才那个应用，或者把天天用的钉住。密码管理器标成机密的内容一概不存。",
      },
    },
    {
      img: "clip-image.png",
      tag: "Copied images",
      en: {
        t: "Screenshots you copied count too",
        d: "Anything you copy as an image lands in the history with a thumbnail. Find it later by describing what was on screen, the same way you search your image files.",
      },
      zh: {
        t: "复制的截图也在里面",
        d: "以图片形式复制的东西会带缩略图进历史。之后描述一下当时屏幕上是什么就能找回来，和搜图片文件是同一套。",
      },
    },
    {
      img: "videos.png",
      tag: "Video shots",
      en: {
        t: "Inside your videos",
        d: "magpie cuts each video into shots and embeds them. Describe a scene or drop in a picture, and you land on the right moment with its timestamp. Enter starts playback there.",
      },
      zh: {
        t: "视频内部",
        d: "magpie 会把每个视频切成镜头再嵌入。描述一个画面或者丢一张图进去，就能落到对应那一段，带时间点，回车直接从那里开始播。",
      },
    },
  ],
  g2: [
    {
      img: "images.png",
      tag: "SigLIP 2",
      en: {
        t: "Images by what's in them",
        d: "SigLIP 2 runs on your machine. Describe a picture in any language, or drop one in and get back visually similar files with a similarity score.",
      },
      zh: {
        t: "按画面内容搜图",
        d: "SigLIP 2 跑在你自己机器上。用任何语言描述一张图，或者把图拖进来找视觉上相近的文件，结果带相似度。",
      },
    },
    {
      img: "ocr.png",
      tag: "OCR",
      en: {
        t: "The text in your screenshots",
        d: "Turn on OCR (PP-OCRv4 or v6, Chinese and English) and magpie reads the text in your indexed images. That error code you screenshotted three weeks ago is one search away.",
      },
      zh: {
        t: "截图里的文字",
        d: "在设置里打开 OCR（PP-OCRv4 或 v6，中英文都行），magpie 就会读已索引图片里的字。三周前截图里那个报错码，搜一下就回来了。",
      },
    },
    {
      img: "ocr-video.png",
      tag: "OCR / video",
      en: {
        t: "…and in your videos",
        d: "Same engine, every shot's frame. Subtitles, slides, code on someone's screen share: all searchable. Open the hit and playback starts where the text appears.",
      },
      zh: {
        t: "……以及视频里的文字",
        d: "同一个引擎，逐个镜头帧地读。字幕、幻灯片、别人屏幕共享里的代码，全都能搜到，点开就从文字出现的那一刻开始播。",
      },
    },
    {
      img: "crosslang.png",
      tag: "Cross-language",
      en: {
        t: "Ask in one language, match another",
        d: "Search in Chinese and English READMEs come back, or the other way round. The embedding model covers 100+ languages, so you don't have to remember which language you read something in.",
      },
      zh: {
        t: "中文提问，英文命中",
        d: "用中文搜，英文 README 照样出得来，反过来也一样。嵌入模型覆盖 100 多种语言，你不用记当初看的是哪种语言写的。",
      },
    },
    {
      img: "preview.png",
      tag: "Preview pane",
      en: {
        t: "Confirm before you open",
        d: "<kbd>→</kbd> opens a preview next to the list. File text with your terms highlighted, images at full size, a video's shot strip, the top of a repo's README.",
      },
      zh: {
        t: "打开之前先看清",
        d: "<kbd>→</kbd> 在列表旁边开预览：文件正文带高亮、图片看大图、视频看镜头条、仓库看 README 开头。",
      },
    },
  ],
  g3: [
    {
      img: "calc.png",
      tag: "Calculator",
      en: {
        t: "Calculator and unit conversion",
        d: "3*(5+2)^2, 0xff + 1, 100 mb to gb, 32 f to c. The answer shows up above your results, and Enter copies it.",
      },
      zh: {
        t: "计算器与单位换算",
        d: "3*(5+2)^2、0xff + 1、100 mb to gb、32 f to c。答案出现在结果上方，Enter 复制。",
      },
    },
    {
      img: "color.png",
      tag: "Generators",
      en: {
        t: "Colors, UUIDs, passwords",
        d: "#ff6600 gives you a swatch plus rgb and hsl. The same box does uuid, ts 1700000000, pwd 24, and b64 or url encoding both ways.",
      },
      zh: {
        t: "颜色、UUID、密码",
        d: "#ff6600 给你色块加 rgb、hsl。同一个框还能出 uuid、ts 1700000000、pwd 24，以及 b64 和 url 的正反编解码。",
      },
    },
    {
      img: "bang.png",
      tag: "Web shortcuts",
      en: {
        t: "Bang-style web shortcuts",
        d: "gh magpie goes to GitHub search, g … to Google, bd … to Baidu. Edit the prefixes in settings, or add your own.",
      },
      zh: {
        t: "网页快搜前缀",
        d: "gh magpie 去 GitHub 搜索，g … 走 Google，bd … 走百度。前缀规则在设置里改，也可以自己加。",
      },
    },
    {
      img: "emoji.png",
      tag: "Emoji",
      en: {
        t: "Emoji lookup",
        d: "Type : and a keyword, like :fire or :火. Click one or hit Enter to copy it.",
      },
      zh: {
        t: "表情查找",
        d: "输入 : 加关键词，比如 :fire 或者 :火。点一下或者回车就复制。",
      },
    },
  ],
  g4: [
    {
      img: "tips.png",
      tag: "Discoverability",
      en: {
        t: "It teaches itself",
        d: "The empty box shows a one-line tip: a shortcut, or something you probably haven't found yet. It swaps for a new one every few seconds.",
      },
      zh: {
        t: "自己教你用",
        d: "空着的输入框下面有一行提示：某个快捷键，或者你多半还没发现的功能。过几秒换一条。",
      },
    },
  ],
  // Chapter 05: everything that doesn't need a screenshot to explain.
  rest: {
    en: [
      ["Hybrid retrieval", "SQLite FTS5 keyword search and local embeddings, merged with reciprocal rank fusion. Long files are chunked, so a sentence on page 100 still surfaces."],
      ["It learns your habits", "Results you actually open drift upward over time. The stats are local and decay, and they never beat a better match."],
      ["Files come with next steps", "<kbd>Ctrl+C</kbd> copies a hit's path. <kbd>Ctrl+Shift+C</kbd> puts the file itself on the clipboard, ready to paste as an attachment."],
      ["Updates that find you", "magpie checks at launch and once a day. A pending release shows a red dot on the tray icon; installing is still your call."],
      ["English and 简体中文", "The whole interface, tray menu included. Follows your OS by default, switchable in settings."],
      ["Scanned PDFs", "A separate switch turns OCR loose on PDF pages that have no text layer. Off by default, since big scans take a while."],
      ["Straight to the browser", "<kbd>Ctrl+Enter</kbd> hands your text to the default browser. URLs open, everything else becomes a web search."],
      ["Multi-monitor", "The palette shows up on whichever display your cursor is on."],
      ["Settings travel", "Export the whole setup to one JSON and import it on another machine. Your GitHub token stays behind."],
      ["Models even behind a firewall", "Switch to hf-mirror.com in one click. If Hugging Face and the mirror are both unreachable, magpie falls back to its own release assets."],
    ],
    zh: [
      ["混合检索", "SQLite FTS5 关键词加本地向量，用 RRF 融合排名。长文件分块存，第 100 页的一句话照样能被搜到。"],
      ["越用越懂你", "真正被打开的结果会慢慢往上走。统计只在本地、随时间衰减，也压不过更匹配的结果。"],
      ["文件命中带后续动作", "<kbd>Ctrl+C</kbd> 复制路径，<kbd>Ctrl+Shift+C</kbd> 把文件本体放上剪贴板，粘出去就是附件。"],
      ["更新会自己找上门", "启动时查一次，之后每天查一次。有新版托盘图标亮红点，装不装还是你说了算。"],
      ["English 和简体中文", "整个界面连托盘菜单都有。默认跟随系统，设置里能切。"],
      ["扫描版 PDF", "单独一个开关，专门对付没有文字层的 PDF 页面。默认关着，大部头扫描件比较耗时。"],
      ["直接去浏览器", "<kbd>Ctrl+Enter</kbd> 把输入交给默认浏览器。是网址就打开，不是就变成网页搜索。"],
      ["多显示器", "浮窗出现在你鼠标所在的那块屏幕上。"],
      ["配置带着走", "整套设置导出成一个 JSON，换台机器导进去。GitHub token 不会跟着走。"],
      ["模型在墙内也能下", "一键切到 hf-mirror.com。要是 Hugging Face 和镜像都连不上，magpie 会回落到自己的 Release 资产。"],
    ],
  },
  g5: {
    en: [
      "<b>No full-disk scanning.</b> magpie only reads folders you added yourself. It respects .gitignore, skips hidden files, and won't follow a symlink out of the folder you chose.",
      "<b>Nothing leaves your machine.</b> Your index is one SQLite file in your user profile. Models run locally through ONNX and work offline once they've downloaded.",
      "<b>Queries never get logged.</b> The activity log keeps errors and model status so you can file a bug report worth reading. What you typed isn't in there.",
      "<b>Secrets stay out.</b> Clips your password manager marks confidential never get stored, and exporting your settings leaves the GitHub token behind.",
      "<b>Signed and notarized.</b> macOS builds carry a Developer ID signature and Apple's notarization. Updates install in place and check the signature first.",
    ],
    zh: [
      "<b>不做全盘扫描。</b>只读你自己添加的目录，尊重 .gitignore，跳过隐藏文件，也不会顺着符号链接跑出去。",
      "<b>数据不出本机。</b>索引就是用户目录下的一个 SQLite 文件。模型通过 ONNX 本地跑，下载完就能离线用。",
      "<b>搜索内容不进日志。</b>日志里只有错误和模型状态，够你提一个有用的 issue。你输入过什么，里面没有。",
      "<b>机密不留痕。</b>密码管理器标成机密的剪贴内容一概不存，导出配置也不会带上 GitHub token。",
      "<b>已签名并公证。</b>macOS 版带 Developer ID 签名和苹果公证，更新原地安装，装之前先验签名。",
    ],
  },
};
