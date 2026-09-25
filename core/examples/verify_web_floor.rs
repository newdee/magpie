//! Calibration for the web search's semantic floor: with the real e5 model,
//! how similar are bookmarks a query should find by meaning alone, and how
//! similar are ones it should not find at all? Prints both distributions.
//! Run: cargo run -p magpie-core --release --example verify_web_floor -- <models dir>

use magpie_core::embed::Embedder;

/// (title, folder, url) as bookmarks are embedded (`title\nfolder\nurl`).
const DOCS: &[(&str, &str, &str)] = &[
    ("哔哩哔哩 (゜-゜)つロ 干杯~-bilibili", "娱乐", "https://www.bilibili.com/"),
    ("YouTube", "Other bookmarks", "https://www.youtube.com/"),
    ("GitHub", "Dev", "https://github.com/"),
    ("docs.rs", "Dev/Rust", "https://docs.rs/"),
    ("The Rust Programming Language", "Dev/Rust", "https://doc.rust-lang.org/book/"),
    ("Announcing native support for Stripe's JavaScript SDK in Cloudflare Workers", "其他书签", "https://blog.cloudflare.com/announcing-stripe-support-in-workers/"),
    ("V2EX › Planet › rich-1e", "其他书签", "https://www.v2ex.com/planet/rich-1e.sol/162E7BBB-750F-4FBA-96E6"),
    ("TechCrunch | 科技新闻", "其他书签", "https://techcrunch.com/"),
    ("淘宝网 - 淘！我喜欢", "购物", "https://www.taobao.com/"),
    ("Amazon.com. Spend less. Smile more.", "Shopping", "https://www.amazon.com/"),
    ("知乎 - 有问题，就会有答案", "社区", "https://www.zhihu.com/"),
    ("中国天气网", "生活", "https://www.weather.com.cn/"),
    ("Gmail", "Other bookmarks", "https://mail.google.com/"),
    ("Google 翻译", "工具", "https://translate.google.com/"),
    ("Hacker News", "News", "https://news.ycombinator.com/"),
    ("Stack Overflow - Where Developers Learn, Share, & Build Careers", "Dev", "https://stackoverflow.com/"),
    ("Figma: The Collaborative Interface Design Tool", "Design", "https://www.figma.com/"),
    ("Netflix", "Entertainment", "https://www.netflix.com/"),
    ("网易云音乐", "娱乐", "https://music.163.com/"),
    ("12306 中国铁路", "出行", "https://www.12306.cn/"),
];

/// (query, titles it should find by meaning, with no shared keyword).
const WANTED: &[(&str, &[&str])] = &[
    ("视频", &["哔哩哔哩", "YouTube", "Netflix"]),
    ("video", &["哔哩哔哩", "YouTube", "Netflix"]),
    ("代码托管", &["GitHub"]),
    ("购物", &["淘宝网", "Amazon.com"]),
    ("shopping", &["淘宝网", "Amazon.com"]),
    ("网购", &["淘宝网", "Amazon.com"]),
    ("邮箱", &["Gmail"]),
    ("email", &["Gmail"]),
    ("翻译", &["Google 翻译"]),
    ("火车票", &["12306"]),
    ("听歌", &["网易云音乐"]),
    ("music", &["网易云音乐"]),
    ("天气预报", &["中国天气网"]),
    ("tech news", &["TechCrunch", "Hacker News"]),
    ("编程问答", &["Stack Overflow"]),
    ("rust 教程", &["The Rust Programming Language"]),
    ("设计工具", &["Figma"]),
    ("电影", &["Netflix"]),
    ("问答社区", &["知乎", "Stack Overflow"]),
    ("payment", &["Announcing native support for Stripe"]),
];

/// Unrelated reference pages: a query's mean similarity to these is its own
/// baseline (e5 scores some queries high against everything).
const BACKGROUND: &[&str] = &[
    "NBA scores, schedule and standings\nSports\nhttps://www.nba.com/",
    "下厨房 - 家常菜谱大全\n生活\nhttps://www.xiachufang.com/",
    "Yahoo Finance - Stock Market Live, Quotes\nFinance\nhttps://finance.yahoo.com/",
    "携程旅行 - 酒店预订、机票预订\n出行\nhttps://www.ctrip.com/",
    "国家政务服务平台\n政务\nhttps://gjzwfw.www.gov.cn/",
    "Coursera | Online Courses From Top Universities\nLearning\nhttps://www.coursera.org/",
    "Mayo Clinic - Health information\nHealth\nhttps://www.mayoclinic.org/",
    "汽车之家\n汽车\nhttps://www.autohome.com.cn/",
    "Zillow: Real Estate, Apartments, Mortgages\nHome\nhttps://www.zillow.com/",
    "Steam 商店\n游戏\nhttps://store.steampowered.com/",
    "Python.org\nDev\nhttps://www.python.org/",
    "Wikipedia, the free encyclopedia\nReference\nhttps://en.wikipedia.org/",
    "小红书 - 你的生活指南\n社区\nhttps://www.xiaohongshu.com/",
    "BBC News - Home\nNews\nhttps://www.bbc.com/news",
    "Airbnb | Vacation rentals\nTravel\nhttps://www.airbnb.com/",
    "豆瓣电影\n娱乐\nhttps://movie.douban.com/",
    "LinkedIn: Log In or Sign Up\nWork\nhttps://www.linkedin.com/",
    "Notion – The all-in-one workspace\nTools\nhttps://www.notion.so/",
    "百度地图\n出行\nhttps://map.baidu.com/",
    "Reddit - Dive into anything\nCommunity\nhttps://www.reddit.com/",
    "招商银行\n金融\nhttps://www.cmbchina.com/",
    "arXiv.org e-Print archive\nResearch\nhttps://arxiv.org/",
    "Dribbble - Discover the World's Top Designers\nDesign\nhttps://dribbble.com/",
    "Duolingo - The world's best way to learn a language\nLearning\nhttps://www.duolingo.com/",
    "IKEA 宜家家居\n家居\nhttps://www.ikea.cn/",
    "Twitch\nEntertainment\nhttps://www.twitch.tv/",
    "Medium – Where good ideas find you\nReading\nhttps://medium.com/",
    "美团外卖\n生活\nhttps://waimai.meituan.com/",
    "Canva: Visual Suite for Everyone\nDesign\nhttps://www.canva.com/",
    "Kaggle: Your Machine Learning and Data Science Community\nData\nhttps://www.kaggle.com/",
];
/// Queries none of these bookmarks answers (as in issue #5: "bili" and
/// "test" against three unrelated bookmarks).
const NOISE: &[&str] = &[
    "bili", "test", "hello", "abc", "magpie", "发票", "报销", "docker compose", "kubernetes",
    "机器学习", "晚饭吃什么", "护照", "健身", "xyz123", "打印机驱动", "tauri", "水电费", "简历",
];

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).expect("models dir");
    let mut e = Embedder::new(std::path::Path::new(&dir))?;
    let docs: Vec<String> = DOCS.iter().map(|(t, f, u)| format!("{t}\n{f}\n{u}")).collect();
    let dv = e.embed_passages(&docs)?;
    let sim = |q: &[f32], d: &[f32]| q.iter().zip(d).map(|(a, b)| a * b).sum::<f32>();
    let bgv = e.embed_passages(&BACKGROUND.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;
    // a query's own baseline: its mean similarity to unrelated pages
    let base = |q: &[f32]| bgv.iter().map(|d| sim(q, d)).sum::<f32>() / bgv.len() as f32;
    let (mut mw, mut mu, mut mn) = (Vec::new(), Vec::new(), Vec::new());

    let mut wanted = Vec::new(); // sims of pairs that should be found
    let mut unwanted = Vec::new(); // best sim of anything else, per query
    for (q, want) in WANTED {
        let qv = e.embed_query(q)?;
        let b = base(&qv);
        let mut best_other = f32::MIN;
        let mut line = Vec::new();
        for (i, (t, _, _)) in DOCS.iter().enumerate() {
            let s = sim(&qv, &dv[i]);
            if want.iter().any(|w| t.starts_with(w)) {
                wanted.push(s);
                mw.push(s - b);
                line.push(format!("{}={s:.3}", t.chars().take(10).collect::<String>()));
            } else {
                best_other = best_other.max(s);
            }
        }
        unwanted.push(best_other);
        mu.push(best_other - b);
        println!("want  {q:<10} {}  | best other {best_other:.3} | base {b:.3}", line.join(" "));
    }
    let mut noise = Vec::new();
    for q in NOISE {
        let qv = e.embed_query(q)?;
        let (best, i) = dv.iter().enumerate().map(|(i, d)| (sim(&qv, d), i)).fold((f32::MIN, 0), |a, b| if b.0 > a.0 { b } else { a });
        noise.push(best);
        let b = base(&qv);
        mn.push(best - b);
        println!("noise {q:<10} best {best:.3} base {b:.3} margin {:.3} ({})", best - b, DOCS[i].0.chars().take(24).collect::<String>());
    }
    let stats = |name: &str, v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        let p = |q: f32| v[((v.len() - 1) as f32 * q).round() as usize];
        println!("{name:<28} n={:<3} min {:.3}  p10 {:.3}  median {:.3}  p90 {:.3}  max {:.3}", v.len(), v[0], p(0.1), p(0.5), p(0.9), v[v.len() - 1]);
    };
    println!();
    stats("wanted (semantic only)", &mut wanted);
    stats("best unwanted per query", &mut unwanted);
    stats("best hit for noise queries", &mut noise);
    println!("margin over the query's own baseline:");
    stats("wanted (semantic only)", &mut mw);
    stats("best unwanted per query", &mut mu);
    stats("best hit for noise queries", &mut mn);
    Ok(())
}
