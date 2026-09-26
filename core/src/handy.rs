//! Small local helpers behind query-box verbs: amounts in Chinese capitals,
//! pinyin, full/half width, escapes, random picks, a system summary, QR
//! decoding, line diffs, and timer durations. All offline; the verbs that
//! use them live in `transform` (or in the app, for those needing state).

/// An amount of money in Chinese capital numerals, as written on invoices
/// and contracts: `1234.56` → 壹仟贰佰叁拾肆元伍角陆分, `100` → 壹佰元整.
/// Accepts `¥`, `￥`, commas and spaces; up to 9999亿, two decimals
/// (rounded to the fen).
pub fn rmb_upper(input: &str) -> Option<String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !matches!(c, ',' | '，' | ' ' | '¥' | '￥' | '元'))
        .collect();
    let (neg, digits) = match cleaned.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, cleaned.as_str()),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') || digits.matches('.').count() > 1 {
        return None;
    }
    let value: f64 = digits.parse().ok()?;
    let fen_total = (value * 100.0).round();
    if !(0.0..1e14).contains(&fen_total) {
        return None;
    }
    let fen_total = fen_total as u64;
    let (yuan, jiao, fen) = (fen_total / 100, (fen_total / 10) % 10, fen_total % 10);
    const NUM: [&str; 10] = ["零", "壹", "贰", "叁", "肆", "伍", "陆", "柒", "捌", "玖"];
    let mut out = String::new();
    if neg {
        out.push('负');
    }
    if yuan > 0 {
        out.push_str(&integer_upper(yuan));
        out.push('元');
    }
    match (jiao, fen) {
        (0, 0) => out.push_str(if yuan > 0 { "整" } else { "零元整" }),
        (j, 0) => {
            out.push_str(NUM[j as usize]);
            out.push_str("角整");
        }
        (j, f) => {
            if j > 0 {
                out.push_str(NUM[j as usize]);
                out.push('角');
            } else if yuan > 0 {
                out.push('零');
            }
            out.push_str(NUM[f as usize]);
            out.push('分');
        }
    }
    Some(out)
}

/// The integer part in capitals: groups of four digits (个、万、亿), zeros
/// collapsed to one 零 where a gap needs saying.
fn integer_upper(mut n: u64) -> String {
    const NUM: [&str; 10] = ["零", "壹", "贰", "叁", "肆", "伍", "陆", "柒", "捌", "玖"];
    const UNIT: [&str; 4] = ["", "拾", "佰", "仟"];
    const GROUP: [&str; 3] = ["", "万", "亿"];
    let mut groups = Vec::new();
    while n > 0 {
        groups.push(n % 10_000);
        n /= 10_000;
    }
    let mut out = String::new();
    let mut need_zero = false;
    for (gi, &g) in groups.iter().enumerate().rev() {
        if g == 0 {
            need_zero = !out.is_empty();
            continue;
        }
        // a group under 1000 after a higher one reads with a 零 in front
        if !out.is_empty() && (need_zero || g < 1000) {
            out.push('零');
        }
        need_zero = false;
        let digits = [g / 1000, (g / 100) % 10, (g / 10) % 10, g % 10];
        let mut zero = false;
        let mut started = false;
        for (i, &d) in digits.iter().enumerate() {
            if d == 0 {
                zero = started;
                continue;
            }
            if zero {
                out.push('零');
                zero = false;
            }
            out.push_str(NUM[d as usize]);
            out.push_str(UNIT[3 - i]);
            started = true;
        }
        out.push_str(GROUP[gi.min(2)]);
    }
    out
}

/// Pinyin with tone marks, syllables separated by spaces; other characters
/// kept as they are. The library gives each character its most common
/// reading, so a short list of frequent words with a different reading
/// (重庆, 银行, 长大 …) is applied first.
pub fn pinyin_of(text: &str) -> String {
    use pinyin::ToPinyin;
    const WORDS: &[(&str, &str)] = &[
        ("重庆", "chóng qìng"), ("重新", "chóng xīn"), ("重复", "chóng fù"), ("银行", "yín háng"),
        ("行业", "háng yè"), ("行长", "háng zhǎng"), ("长大", "zhǎng dà"), ("成长", "chéng zhǎng"),
        ("校长", "xiào zhǎng"), ("音乐", "yīn yuè"), ("乐器", "yuè qì"), ("会计", "kuài jì"),
        ("还是", "hái shì"), ("还有", "hái yǒu"), ("还钱", "huán qián"), ("归还", "guī huán"),
        ("觉得", "jué de"), ("睡觉", "shuì jiào"), ("地方", "dì fang"), ("东西", "dōng xi"),
        ("什么", "shén me"), ("为了", "wèi le"), ("因为", "yīn wèi"), ("了解", "liǎo jiě"),
        ("数据", "shù jù"), ("数学", "shù xué"), ("调查", "diào chá"), ("调整", "tiáo zhěng"),
        ("空调", "kōng tiáo"), ("朝阳", "zhāo yáng"), ("朝代", "cháo dài"), ("厦门", "xià mén"),
        ("大厦", "dà shà"), ("单于", "chán yú"), ("单位", "dān wèi"), ("便宜", "pián yi"),
        ("方便", "fāng biàn"), ("不得不", "bù dé bù"), ("得到", "dé dào"), ("长城", "cháng chéng"),
        ("着急", "zháo jí"), ("看着", "kàn zhe"), ("好奇", "hào qí"), ("爱好", "ài hào"),
        ("处理", "chǔ lǐ"), ("到处", "dào chù"), ("差不多", "chà bu duō"), ("出差", "chū chāi"),
        ("参差", "cēn cī"), ("降落", "jiàng luò"), ("投降", "tóu xiáng"), ("的确", "dí què"),
        ("目的", "mù dì"), ("觉悟", "jué wù"), ("血液", "xuè yè"), ("流血", "liú xuè"),
    ];
    // (text, is a syllable): runs of other text stay together ("iPhone 16")
    let mut out: Vec<(String, bool)> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    'outer: while i < chars.len() {
        for (w, py) in WORDS {
            let n = w.chars().count();
            if i + n <= chars.len() && chars[i..i + n].iter().collect::<String>() == *w {
                out.extend(py.split(' ').map(|p| (p.to_string(), true)));
                i += n;
                continue 'outer;
            }
        }
        let c = chars[i];
        match c.to_pinyin() {
            Some(p) => out.push((p.with_tone().to_string(), true)),
            None => match out.last_mut() {
                Some((last, false)) => last.push(c),
                _ => out.push((c.to_string(), false)),
            },
        }
        i += 1;
    }
    out.iter()
        .map(|(s, _)| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Full-width letters, digits, symbols and the ideographic space to their
/// ASCII forms (what PDFs and some sites hand you). The marks of a Chinese
/// sentence (，！？：；（）) stay: in Chinese text they are meant to be wide.
pub fn to_halfwidth(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '，' | '！' | '？' | '：' | '；' | '（' | '）' => c,
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// ASCII letters, digits, punctuation and space to their full-width forms.
pub fn to_fullwidth(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => '\u{3000}',
            '!'..='~' => char::from_u32(c as u32 + 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Unicode escapes (backslash, u, four hex digits) back to text when the
/// text holds any, surrogate pairs joined; otherwise non-ASCII characters to
/// such escapes. Returns (result,
/// decoded?).
pub fn unicode_toggle(s: &str) -> (String, bool) {
    if s.contains("\\u") || s.contains("\\U") {
        let mut units: Vec<u16> = Vec::new();
        let mut out = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        let flush = |units: &mut Vec<u16>, out: &mut String| {
            if !units.is_empty() {
                out.push_str(&String::from_utf16_lossy(units));
                units.clear();
            }
        };
        while i < chars.len() {
            if chars[i] == '\\' && matches!(chars.get(i + 1), Some('u') | Some('U')) {
                let hex: String = chars[i + 2..(i + 6).min(chars.len())].iter().collect();
                if hex.len() == 4 {
                    if let Ok(u) = u16::from_str_radix(&hex, 16) {
                        units.push(u);
                        i += 6;
                        continue;
                    }
                }
            }
            flush(&mut units, &mut out);
            out.push(chars[i]);
            i += 1;
        }
        flush(&mut units, &mut out);
        (out, true)
    } else {
        let mut out = String::new();
        for c in s.chars() {
            if c.is_ascii() {
                out.push(c);
            } else {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
        }
        (out, false)
    }
}

/// `&lt;b&gt; &amp; &#20013;` → `<b> & 中` when the text holds entities;
/// otherwise `< > & " '` → entities. Returns (result, decoded?).
pub fn html_toggle(s: &str) -> (String, bool) {
    let has_entity = regex_like_entity(s);
    if has_entity {
        let mut out = String::new();
        let mut rest = s;
        while let Some(amp) = rest.find('&') {
            out.push_str(&rest[..amp]);
            let tail = &rest[amp..];
            match tail.find(';').filter(|&semi| semi <= 10) {
                Some(semi) => {
                    let name = &tail[1..semi];
                    let decoded = match name {
                        "amp" => Some("&".to_string()),
                        "lt" => Some("<".to_string()),
                        "gt" => Some(">".to_string()),
                        "quot" => Some("\"".to_string()),
                        "apos" | "#39" => Some("'".to_string()),
                        "nbsp" => Some("\u{a0}".to_string()),
                        n if n.starts_with("#x") || n.starts_with("#X") => {
                            u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32).map(|c| c.to_string())
                        }
                        n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32).map(|c| c.to_string()),
                        _ => None,
                    };
                    match decoded {
                        Some(d) => {
                            out.push_str(&d);
                            rest = &tail[semi + 1..];
                        }
                        None => {
                            out.push('&');
                            rest = &tail[1..];
                        }
                    }
                }
                None => {
                    out.push('&');
                    rest = &tail[1..];
                }
            }
        }
        out.push_str(rest);
        (out, true)
    } else {
        let out = s
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;");
        (out, false)
    }
}

fn regex_like_entity(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(p) = s[i..].find('&') {
        let start = i + p + 1;
        let end = (start + 10).min(b.len());
        if let Some(semi) = s[start..end].find(';') {
            let name = &s[start..start + semi];
            if !name.is_empty() && (name.chars().all(|c| c.is_ascii_alphanumeric()) || name.starts_with('#')) {
                return true;
            }
        }
        i = start;
        if i >= b.len() {
            break;
        }
    }
    false
}

/// A uniform random integer in `lo..=hi` from the OS RNG (rejection
/// sampling, no modulo bias).
pub fn random_between(lo: i64, hi: i64) -> Option<i64> {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let span = (hi as i128 - lo as i128 + 1) as u128;
    if span == 0 || span > u64::MAX as u128 {
        return None;
    }
    let span = span as u64;
    let zone = u64::MAX - (u64::MAX % span);
    loop {
        let mut buf = [0u8; 8];
        getrandom::fill(&mut buf).ok()?;
        let v = u64::from_le_bytes(buf);
        if v < zone {
            return Some(lo + (v % span) as i64);
        }
    }
}

/// The items of `pick 火锅 烧烤, 麻辣烫`: split on spaces, commas and 、.
pub fn pick_items(s: &str) -> Vec<String> {
    s.split(|c: char| c.is_whitespace() || matches!(c, ',' | '，' | '、' | ';' | '；' | '|'))
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

/// CPU load, memory and each disk's free space, one line. Takes ~250 ms
/// (CPU load needs two samples).
pub fn system_summary() -> String {
    use sysinfo::{Disks, System};
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(200)));
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let gb = |b: u64| b as f64 / 1_073_741_824.0;
    let mut parts = vec![
        format!("CPU {:.0}%", sys.global_cpu_usage()),
        format!("memory {:.1} of {:.0} GB", gb(sys.used_memory()), gb(sys.total_memory())),
    ];
    let disks = Disks::new_with_refreshed_list();
    let mut seen = std::collections::HashSet::new();
    for d in disks.list() {
        let mount = d.mount_point().to_string_lossy().to_string();
        // one line per volume: skip tiny system and duplicate mounts
        if d.total_space() < 2 * 1_073_741_824 || !seen.insert(d.total_space() ^ d.available_space()) {
            continue;
        }
        parts.push(format!("{} {:.0} GB free of {:.0} GB", mount.trim_end_matches(['\\', '/']).trim_end_matches(':').to_string() + if cfg!(windows) { ":" } else { "" }, gb(d.available_space()), gb(d.total_space())));
    }
    parts.join(" · ")
}

/// The text of the first QR code found in an image.
pub fn decode_qr(img: &image::DynamicImage) -> Option<String> {
    let luma = img.to_luma8();
    let (w, h) = (luma.width() as usize, luma.height() as usize);
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| luma.get_pixel(x as u32, y as u32).0[0]);
    prepared
        .detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, text)| text))
}

/// A line diff of `old` → `new`: ('-', line), ('+', line) or (' ', line).
pub fn line_diff(old: &str, new: &str) -> Vec<(char, String)> {
    use similar::{ChangeTag, TextDiff};
    TextDiff::from_lines(old, new)
        .iter_all_changes()
        .map(|c| {
            let tag = match c.tag() {
                ChangeTag::Delete => '-',
                ChangeTag::Insert => '+',
                ChangeTag::Equal => ' ',
            };
            (tag, c.value().trim_end_matches(['\n', '\r']).to_string())
        })
        .collect()
}

/// A timer length: `25m`, `1h30m`, `90s`, `1:30` (minutes:seconds), a bare
/// number (minutes), or Chinese (`25分钟`, `1小时`, `30秒`). Up to 24 hours.
pub fn parse_duration(s: &str) -> Option<u64> {
    let t = s.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    if let Some((m, sec)) = t.split_once(':') {
        let (m, sec): (u64, u64) = (m.parse().ok()?, sec.parse().ok()?);
        return (sec < 60).then_some(m * 60 + sec).filter(|&x| x > 0 && x <= 86_400);
    }
    if let Ok(n) = t.parse::<f64>() {
        let secs = (n * 60.0).round() as u64;
        return (n > 0.0 && secs <= 86_400).then_some(secs);
    }
    let mut total = 0u64;
    let mut num = String::new();
    let mut any = false;
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        let mut unit = c.to_string();
        // two-character Chinese units: 小时, 分钟
        if matches!(c, '小' | '分') {
            if let Some(&n) = chars.peek() {
                if (c == '小' && n == '时') || (c == '分' && n == '钟') {
                    unit.push(n);
                    chars.next();
                }
            }
        }
        if unit == "i" || unit == "n" || unit == "e" || unit == "c" || unit == "o" || unit == "r" || unit == "u" || unit == "t" {
            continue; // letters of min / sec / hour spelled out
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        let mult = match unit.as_str() {
            "h" | "小时" | "时" => 3600.0,
            "m" | "分钟" | "分" => 60.0,
            "s" | "秒" => 1.0,
            _ => return None,
        };
        total += (n * mult).round() as u64;
        any = true;
    }
    if !num.is_empty() {
        return None; // a number with no unit after others
    }
    (any && total > 0 && total <= 86_400).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_in_capitals() {
        for (n, want) in [
            ("1234.56", "壹仟贰佰叁拾肆元伍角陆分"),
            ("100", "壹佰元整"),
            ("0.5", "伍角整"),
            ("0.05", "伍分"),
            ("1005", "壹仟零伍元整"),
            ("1010.1", "壹仟零壹拾元壹角整"),
            ("10000", "壹万元整"),
            ("100010", "壹拾万零壹拾元整"),
            ("12345678.9", "壹仟贰佰叁拾肆万伍仟陆佰柒拾捌元玖角整"),
            ("100000000", "壹亿元整"),
            ("1,000.07", "壹仟元零柒分"),
            ("¥ 0", "零元整"),
            ("-20.5", "负贰拾元伍角整"),
            ("200000001", "贰亿零壹元整"),
        ] {
            assert_eq!(rmb_upper(n).as_deref(), Some(want), "{n}");
        }
        assert_eq!(rmb_upper("abc"), None);
        assert_eq!(rmb_upper("1.2.3"), None);
    }

    #[test]
    fn pinyin_with_tones_and_common_words() {
        assert_eq!(pinyin_of("中文"), "zhōng wén");
        assert_eq!(pinyin_of("重庆银行"), "chóng qìng yín háng");
        assert_eq!(pinyin_of("我长大了"), "wǒ zhǎng dà le");
        assert_eq!(pinyin_of("用 iPhone 拍照"), "yòng iPhone pāi zhào");
    }

    #[test]
    fn width_conversions_round_trip() {
        assert_eq!(to_halfwidth("ＡＢＣ１２３，！　ｘ＠＃"), "ABC123，！ x@#");
        assert_eq!(to_fullwidth("AB 1!"), "ＡＢ　１！");
        assert_eq!(to_halfwidth(&to_fullwidth("Hello World 42 @#")), "Hello World 42 @#");
        assert_eq!(to_halfwidth("中文不变"), "中文不变");
    }

    #[test]
    fn escapes_toggle_both_ways() {
        let esc = |hex: &[&str]| hex.iter().map(|h| format!("{}u{h}", '\\')).collect::<String>();
        assert_eq!(unicode_toggle(&(esc(&["4e2d", "6587"]) + " ok")), ("中文 ok".to_string(), true));
        assert_eq!(unicode_toggle("中 a"), (esc(&["4e2d"]) + " a", false));
        assert_eq!(unicode_toggle(&esc(&["d83d", "de00"])).0, "😀", "surrogate pair joined");
        assert_eq!(unicode_toggle("😀").0, esc(&["d83d", "de00"]));
        assert_eq!(html_toggle("&lt;b&gt; &amp; &#20013; &#x6587;"), ("<b> & 中 文".to_string(), true));
        assert_eq!(html_toggle(r#"<a href="x">T&C</a>"#).0, "&lt;a href=&quot;x&quot;&gt;T&amp;C&lt;/a&gt;");
        assert_eq!(html_toggle("R&D dept").0, "R&amp;D dept", "a bare & is not an entity");
    }

    #[test]
    fn random_picks_stay_in_range() {
        for _ in 0..500 {
            let v = random_between(1, 6).unwrap();
            assert!((1..=6).contains(&v));
        }
        assert_eq!(random_between(5, 5), Some(5));
        assert!((1..=3).contains(&random_between(3, 1).unwrap()));
        let seen: std::collections::HashSet<i64> = (0..300).map(|_| random_between(1, 3).unwrap()).collect();
        assert_eq!(seen.len(), 3, "every value comes up");
        assert_eq!(pick_items("火锅 烧烤，麻辣烫、 面"), vec!["火锅", "烧烤", "麻辣烫", "面"]);
    }

    #[test]
    fn durations() {
        for (s, want) in [
            ("25m", Some(1500)), ("1h30m", Some(5400)), ("90s", Some(90)), ("1:30", Some(90)),
            ("25", Some(1500)), ("0.5", Some(30)), ("25分钟", Some(1500)), ("1小时", Some(3600)),
            ("30秒", Some(30)), ("1h", Some(3600)), ("2min", Some(120)), ("10sec", Some(10)),
            ("0", None), ("25x", None), ("25h", None), ("", None), ("m", None),
        ] {
            assert_eq!(parse_duration(s), want, "{s}");
        }
    }

    #[test]
    fn qr_codes_decode() {
        let code = qrcode::QrCode::new("https://github.com/newdee/magpie").unwrap();
        let img = code.render::<image::Luma<u8>>().min_dimensions(240, 240).build();
        let text = decode_qr(&image::DynamicImage::ImageLuma8(img));
        assert_eq!(text.as_deref(), Some("https://github.com/newdee/magpie"));
        let blank = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(200, 200, image::Luma([255])));
        assert_eq!(decode_qr(&blank), None);
    }

    #[test]
    fn line_diffs() {
        let d = line_diff("a\nb\nc\n", "a\nB\nc\nd\n");
        assert_eq!(d, vec![(' ', "a".into()), ('-', "b".into()), ('+', "B".into()), (' ', "c".into()), ('+', "d".into())]);
    }

    #[test]
    fn system_summary_has_cpu_memory_and_a_disk() {
        let s = system_summary();
        assert!(s.starts_with("CPU ") && s.contains("memory ") && s.contains("GB free of"), "{s}");
    }
}
