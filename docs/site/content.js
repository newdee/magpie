// All page copy, in both languages. One source of truth so the two versions
// can never drift apart — the page swaps text nodes, it never reloads.
window.CONTENT = {
  en: {
    "nav.repo": "Repo",
    "nav.download": "Download",
    "hero.title": "Everything you saved. One keystroke.",
    "hero.lede":
      "Magpies hoard shiny things and famously forget where they put them. So do we. magpie is a tiny desktop launcher that brings them all back — press a hotkey, type what you vaguely remember, hit Enter. Everything runs on your machine.",
    "hero.download": "Download",
    "hero.source": "Source",
    "hero.meta": "Free · MIT · Windows / macOS / Linux",
    "g1.title": "What it searches",
    "g1.sub": "Six sources behind one box — <kbd>Tab</kbd> cycles them.",
    "g2.title": "It reads what's inside",
    "g2.sub": "Not just filenames — pixels, frames, and the words printed in them.",
    "g3.title": "A toolbelt in the query box",
    "g3.sub": "Answers that need no search results at all. <kbd>Enter</kbd> copies.",
    "g4.title": "Everyday feel",
    "g4.sub": "The small things that decide whether a launcher stays installed.",
    "g5.title": "Private by architecture, not by promise",
    "close.title": "Press a hotkey. Find the thing.",
    "close.docs": "Full documentation",
    "footer.built": "Built with Rust + Tauri.",
  },
  zh: {
    "nav.repo": "代码仓库",
    "nav.download": "下载",
    "hero.title": "你存过的一切，一个快捷键之外。",
    "hero.lede":
      "喜鹊爱收集闪亮的东西，又出了名地忘记藏在哪。人也一样。magpie 是一个小巧的桌面启动器，把它们统统找回来——按下快捷键，输入模糊的印象，回车。所有计算都在你自己的机器上。",
    "hero.download": "下载",
    "hero.source": "源码",
    "hero.meta": "免费 · MIT · Windows / macOS / Linux",
    "g1.title": "能搜什么",
    "g1.sub": "六个数据源，一个输入框——<kbd>Tab</kbd> 循环切换。",
    "g2.title": "它看得懂内容",
    "g2.sub": "不只是文件名——还有画面、视频帧，以及印在它们上面的字。",
    "g3.title": "输入框里的工具带",
    "g3.sub": "根本不需要搜索结果的答案。<kbd>Enter</kbd> 复制。",
    "g4.title": "日常手感",
    "g4.sub": "决定一个启动器会不会被卸载的那些小事。",
    "g5.title": "隐私靠架构保证，不靠承诺",
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
      en: {
        t: "Local files, full text",
        d: "Add the folders you actually work in. Every file is findable by name; ~80 text formats, PDF, and Office documents are searched by content, with the matching line highlighted.",
      },
      zh: {
        t: "本地文件全文检索",
        d: "把你真正在用的文件夹加进来。任何文件都能按名字找到；约 80 种文本格式、PDF 和 Office 文档按内容检索，命中行高亮显示。",
      },
    },
    {
      img: "apps.png",
      en: {
        t: "An app launcher, too",
        d: "Type a name — prefix, substring, or acronym like vsc — and Enter launches it. Chinese app names match by pinyin, and you can add your own aliases.",
      },
      zh: {
        t: "同时是应用启动器",
        d: "输入应用名——前缀、子串或首字母缩写如 vsc——回车启动。中文应用名支持拼音匹配，还能自定义别名。",
      },
    },
    {
      img: "stars.png",
      en: {
        t: "GitHub stars you forgot",
        d: "Your whole starred list, READMEs included and chunk-embedded, so a repo surfaces from a sentence buried deep in its docs. Sort by relevance, date, or stars.",
      },
      zh: {
        t: "被你遗忘的 GitHub stars",
        d: "整个 star 列表，README 全文分块嵌入——凭文档深处的一句话就能把项目翻出来。可按相关度、收藏时间、星数排序。",
      },
    },
    {
      img: "web.png",
      en: {
        t: "Bookmarks and history",
        d: "Read straight from your browsers' local files — every Chromium-based browser plus Firefox, all profiles. Searched together, ranked with visit counts.",
      },
      zh: {
        t: "书签与浏览历史",
        d: "直接读浏览器本地文件——所有 Chromium 内核浏览器加 Firefox，覆盖全部 profile。合并检索，按访问次数加权排序。",
      },
    },
    {
      img: "clipboard.png",
      en: {
        t: "Clipboard history",
        d: "Opt-in and local. Copy anything back, paste it straight into the app you came from, or pin what you use daily. Password-manager secrets are never recorded.",
      },
      zh: {
        t: "剪贴板历史",
        d: "默认关闭，纯本地。复制回任意一条、直接粘贴回上一个应用，或钉住每天都用的内容。密码管理器标记的机密永不记录。",
      },
    },
    {
      img: "videos.png",
      en: {
        t: "Inside your videos",
        d: "Videos are split into shots and embedded, so a description or a dropped image lands on the exact scene — with its time range, and playback starts right there.",
      },
      zh: {
        t: "视频内部",
        d: "视频按镜头切分并嵌入——一句描述或一张图直接定位到具体场景，带精确时间范围，播放也从那一刻开始。",
      },
    },
  ],
  g2: [
    {
      img: "images.png",
      en: {
        t: "Images by what's in them",
        d: "SigLIP 2 runs locally: describe a picture in any language, or drop one in to find visually similar files — ranked with a similarity percentage.",
      },
      zh: {
        t: "按画面内容搜图",
        d: "SigLIP 2 本地运行：用任何语言描述一张图，或者拖一张进来找视觉相似的文件——带相似度百分比排序。",
      },
    },
    {
      img: "ocr.png",
      en: {
        t: "The text in your screenshots",
        d: "Opt-in OCR (PP-OCRv4 or v6, Chinese + English) reads indexed images, so an error code you screenshotted three weeks ago is one search away.",
      },
      zh: {
        t: "截图里的文字",
        d: "可选 OCR（PP-OCRv4 或 v6，中英文）识别已索引图片——三周前截图里的那个错误码，一次搜索就能回来。",
      },
    },
    {
      img: "ocr-video.png",
      en: {
        t: "…and in your videos",
        d: "The same engine reads every shot's frame, so subtitles, slides and on-screen code become searchable — and the hit jumps playback to the moment the text appears.",
      },
      zh: {
        t: "……以及视频里的文字",
        d: "同一个引擎识别每个镜头帧——字幕、幻灯片、屏幕上的代码全都可搜，命中直接跳到文字出现的那一刻。",
      },
    },
    {
      img: "preview.png",
      en: {
        t: "Confirm before you open",
        d: "<kbd>→</kbd> opens a preview beside the list: file text with your terms highlighted, full-size images, a video's shot strip, a repo's README head.",
      },
      zh: {
        t: "打开之前先看清",
        d: "<kbd>→</kbd> 在列表旁展开预览：文件正文带命中高亮、图片大图、视频镜头条带、仓库 README 开头。",
      },
    },
  ],
  g3: [
    {
      img: "calc.png",
      en: {
        t: "Calculator and unit conversion",
        d: "3*(5+2)^2, 0xff + 1, 100 mb to gb, 32 f to c. The answer rides above the results and Enter copies it.",
      },
      zh: {
        t: "计算器与单位换算",
        d: "3*(5+2)^2、0xff + 1、100 mb to gb、32 f to c。答案显示在结果上方，Enter 复制。",
      },
    },
    {
      img: "color.png",
      en: {
        t: "Colors, UUIDs, passwords",
        d: "#ff6600 shows a live swatch with rgb/hsl. uuid, ts 1700000000, pwd 24, b64 and url encoding — all in the same line.",
      },
      zh: {
        t: "颜色、UUID、密码",
        d: "#ff6600 显示实时色块和 rgb/hsl。uuid、ts 1700000000、pwd 24、b64 与 url 编解码——都在同一行里。",
      },
    },
    {
      img: "bang.png",
      en: {
        t: "Bang-style web shortcuts",
        d: "gh magpie searches GitHub, g … Google, bd … Baidu. The prefix rules are yours to edit in settings.",
      },
      zh: {
        t: "网页快搜前缀",
        d: "gh magpie 直达 GitHub，g … Google，bd … 百度。前缀规则在设置里随你编辑。",
      },
    },
    {
      img: "emoji.png",
      en: {
        t: "Emoji lookup",
        d: "Type : and a keyword — :fire or :火 — then click or press Enter to copy.",
      },
      zh: {
        t: "表情查找",
        d: "输入 : 加关键词——:fire 或 :火——点击或回车复制。",
      },
    },
  ],
  g4: [
    {
      img: "tips.png",
      en: {
        t: "It teaches itself",
        d: "The empty palette carries a one-line tip — a shortcut or a feature you may not have found yet — rotating every few seconds.",
      },
      zh: {
        t: "自己教你用",
        d: "空搜索框下方一行小贴士——一个快捷键，或你可能还没发现的功能——每隔几秒换一条。",
      },
    },
  ],
  g5: {
    en: [
      "<b>No full-disk scanning, ever.</b> Only folders you explicitly add are read — .gitignore respected, hidden files skipped, symlinks never followed out.",
      "<b>Nothing leaves your machine.</b> The index is one SQLite file in your profile; models run locally through ONNX and work offline after the first download.",
      "<b>Queries are never logged.</b> The local activity log records errors and model status so you can file a useful bug report — never what you searched for.",
      "<b>Secrets stay secret.</b> Clips a password manager marks confidential are never recorded, and the settings export leaves your GitHub token behind.",
      "<b>Signed and notarized.</b> macOS builds carry a Developer ID signature and Apple notarization; updates are signature-verified in place.",
    ],
    zh: [
      "<b>永不全盘扫描。</b>只读取你显式添加的文件夹——尊重 .gitignore、跳过隐藏文件、绝不追踪符号链接越界。",
      "<b>数据不出本机。</b>索引是用户目录里的一个 SQLite 文件；模型通过 ONNX 本地运行，首次下载后完全离线。",
      "<b>搜索内容永不入日志。</b>本地运行日志只记录错误和模型状态，方便你报 issue——绝不记录你搜了什么。",
      "<b>机密仍是机密。</b>密码管理器标记为机密的剪贴内容永不记录，配置导出也不会带上你的 GitHub token。",
      "<b>已签名并公证。</b>macOS 构建带 Developer ID 签名与 Apple 公证；更新原地安装且校验签名。",
    ],
  },
};
