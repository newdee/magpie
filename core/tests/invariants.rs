//! Invariants and degenerate inputs across the pure modules. Each block is
//! one of the acceptance lenses: idempotence / round trips (logic), and a
//! deterministic fuzz loop (boundary inputs) that must never panic.
use magpie_core::{calc, filters, notes, transform, worktree};
use std::path::PathBuf;

/// xorshift64*, so the fuzz corpus is the same on every run
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn string(&mut self, max_len: usize) -> String {
        const ALPHABET: &[&str] = &[
            "a", "b", "z", "0", "1", "9", " ", "  ", ".", ",", ":", "+", "-", "*", "/", "^", "(", ")",
            "<", ">", "=", "\n", "\t", "ext:", "in:", "mb", "gb", "kb", "d", "w", "m", "y", "today",
            "tomorrow", "until", "since", "json", "slug", "lines", "count", "upper", "note", "#",
            "ff6600", "0x", "0b", "发票", "报销", "é", "𝄞", "\u{200b}", "2026-10-01", "2026/13/40",
            "..", "\\", "\"", "'", "#报销", "#ff", "#𝄞𝄞", "rgb(", ")", "%E4", "%", "unb64", "unurl",
            "b64", "url", "ts", "pwd", "999999999999", "1e308", "-", "🙂", "\r\n",
        ];
        let n = (self.next() % (max_len as u64 + 1)) as usize;
        (0..n).map(|_| ALPHABET[(self.next() % ALPHABET.len() as u64) as usize]).collect()
    }
}

/// A verb on its own reads the clipboard, which is the environment's state,
/// not the parser's; the fuzz loops skip exactly that shape.
fn bare_verb(q: &str, verbs: &[&str]) -> bool {
    let mut toks = q.split_whitespace();
    let first = toks.next().unwrap_or("").to_lowercase();
    verbs.contains(&first.as_str()) && toks.next().is_none()
}

#[test]
fn filters_parse_is_idempotent_on_its_own_text() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for _ in 0..3000 {
        let q = rng.string(12);
        let (f1, text1) = filters::parse(&q);
        let (f2, text2) = filters::parse(&text1);
        assert!(f2.is_empty(), "text left after parsing must carry no filters: {q:?} -> {text1:?}");
        assert_eq!(text1, text2, "re-parsing the leftover text must not change it");
        let _ = f1.matches(Some("pdf"), 1, 1, "x", 0);
    }
}

#[test]
fn slug_and_lines_are_idempotent() {
    let mut rng = Rng(0xD1B54A32D192ED03);
    for _ in 0..2000 {
        let s = rng.string(10);
        if s.trim().is_empty() {
            continue;
        }
        let once = transform::transform(&format!("slug {s}")).map(|r| r.value);
        if let Some(once) = once {
            let twice = transform::transform(&format!("slug {once}")).map(|r| r.value);
            if !once.is_empty() {
                assert_eq!(twice.as_deref(), Some(once.as_str()), "slug(slug(x)) == slug(x) for {s:?}");
            }
        }
        let lines = transform::transform(&format!("lines {s}")).map(|r| r.value);
        if let Some(l1) = lines {
            let l2 = transform::transform(&format!("lines {l1}")).map(|r| r.value);
            assert_eq!(l2.as_deref(), Some(l1.as_str()), "lines(lines(x)) == lines(x) for {s:?}");
        }
    }
}

#[test]
fn date_math_round_trips() {
    // d + n days - n days == d, for a spread of n, from a fixed date
    for n in [1, 7, 30, 365, 1000] {
        let fwd = calc::eval(&format!("2026-10-01 + {n}d")).expect("forward").value;
        let day = &fwd[..10];
        let back = calc::eval(&format!("{day} - {n}d")).expect("back").value;
        assert!(back.starts_with("2026-10-01"), "{n}d round trip: {fwd} -> {back}");
        let diff = calc::eval(&format!("{day} - 2026-10-01")).expect("diff").value;
        assert_eq!(diff, format!("{n} days"));
    }
}

#[test]
fn calc_and_transform_never_panic_on_random_input() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let verbs = ["json", "upper", "lower", "trim", "slug", "lines", "count"];
    let mut evaluated = 0;
    for _ in 0..40_000 {
        let q = rng.string(14);
        let _ = calc::eval(&q);
        // a bare verb would read the clipboard: environment, not logic
        if !bare_verb(&q, &verbs) && transform::transform(&q).is_some() {
            evaluated += 1;
        }
        let _ = filters::parse(&q);
    }
    assert!(evaluated > 0, "the corpus should hit some transforms");
}

/// Same loop, another seed and much longer strings, so multi-token and
/// deeply nested inputs (parentheses, repeated operators) get exercised.
#[test]
fn long_random_inputs_never_panic_either() {
    let mut rng = Rng(0xA5A5_5A5A_DEAD_BEEF);
    let verbs = ["json", "upper", "lower", "trim", "slug", "lines", "count"];
    for _ in 0..15_000 {
        let q = rng.string(60);
        let _ = calc::eval(&q);
        if !bare_verb(&q, &verbs) {
            let _ = transform::transform(&q);
        }
        let (f, text) = filters::parse(&q);
        let _ = f.matches(None, i64::MAX, i64::MIN, &text, i64::MAX);
        let _ = f.matches(Some("PDF"), i64::MIN, i64::MAX, "", i64::MIN);
    }
}

#[test]
fn notes_take_huge_and_odd_text() {
    let dir = std::env::temp_dir().join(format!("magpie-inv-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("notes.md");
    let huge = "x".repeat(200_000) + "\n\n" + &"y".repeat(10);
    notes::append(&path, &huge).unwrap();
    notes::append(&path, "𝄞 \u{200b} 发票\r\nline").unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert_eq!(body.lines().count(), 2, "each note is exactly one line");
    assert!(body.lines().all(|l| l.starts_with("- ")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worktree_pointer_with_invalid_utf8_is_not_a_worktree() {
    let dir = std::env::temp_dir().join(format!("magpie-inv-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".git"), [0x67, 0x69, 0x74, 0x64, 0x69, 0x72, 0x3a, 0x20, 0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(worktree::gitdir_pointer(&dir), None);
    assert!(!worktree::is_shadowed(&dir, &[PathBuf::from(&dir)]));
    let _ = std::fs::remove_dir_all(&dir);
}
