//! Small DB poke tool for diagnostics and tests.
//! Usage:
//!   dbtool <db> meta-get <key>
//!   dbtool <db> meta-set <key> <value>
//!   dbtool <db> clips [pattern]

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (db, cmd) = (args.first().expect("db path"), args.get(1).map(String::as_str));
    let conn = magpie_core::db::open(std::path::Path::new(db))?;
    match cmd {
        Some("meta-get") => {
            let v = magpie_core::db::meta_get(&conn, &args[2])?;
            println!("{}", v.unwrap_or_else(|| "<unset>".into()));
        }
        Some("meta-set") => {
            magpie_core::db::meta_set(&conn, &args[2], &args[3])?;
            println!("ok");
        }
        Some("clips") => {
            let n = magpie_core::clips::clip_count(&conn)?;
            println!("clip_count={n}");
            for hit in magpie_core::clips::recent_clips(&conn, 10)? {
                let head: String = hit.content.chars().take(60).collect();
                let matched = args
                    .get(2)
                    .map(|p| hit.content.contains(p.as_str()))
                    .unwrap_or(false);
                println!(
                    "  id={} count={} last={}{} {head}",
                    hit.id,
                    hit.copy_count,
                    hit.last_copied,
                    if matched { " [MATCH]" } else { "" }
                );
            }
        }
        Some("clips-clear") => {
            magpie_core::clips::clear_clips(&conn)?;
            println!("cleared");
        }
        Some("sync-history") => {
            let r = magpie_core::history::sync_history(&conn)?;
            println!("browsers={:?} total={} removed={}", r.browsers, r.total, r.removed);
            for h in magpie_core::history::history_by_ids(
                &conn,
                &magpie_core::history::history_fts_search(&conn, args.get(2).map(String::as_str).unwrap_or("a"), 5)?,
                &Default::default(),
            )? {
                println!("  {}x {} — {}", h.visit_count, h.title, h.url);
            }
        }
        Some("apps") => {
            let apps = magpie_core::apps::list_apps();
            println!("app_count={}", apps.len());
            for a in magpie_core::apps::match_apps(&apps, args.get(2).map(String::as_str).unwrap_or(""), 8, true) {
                println!("  {:.2} {} — {}", a.score, a.name, a.target);
            }
        }
        // dbtool <db> index-videos <model_dir> <video_path>
        // E2E: register the video as a files row, shot-detect + embed it,
        // then query with a frame from the first scene and print the ranking.
        Some("index-videos") => {
            let model_dir = std::path::PathBuf::from(args.get(2).expect("model dir"));
            let video = args.get(3).expect("video path");
            let name = std::path::Path::new(video)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let folder: i64 = conn
                .query_row("SELECT id FROM folders LIMIT 1", [], |r| r.get(0))
                .expect("a folder row");
            conn.execute(
                "INSERT OR IGNORE INTO files (folder_id, path, name, ext, size, mtime)
                 VALUES (?1, ?2, ?3, 'mp4', 0, 1)",
                magpie_core::rusqlite::params![folder, video, name],
            )?;
            println!("ffmpeg: {:?}", magpie_core::videos::ensure_ffmpeg()?);
            let mut sig = magpie_core::siglip::Siglip::new(&model_dir)?;
            for (fid, path, mtime) in magpie_core::videos::pending_videos(&conn)? {
                let n = magpie_core::videos::index_video(&conn, &mut sig, fid, &path, mtime)?;
                println!("indexed {path}: {n} shots embedded");
            }
            let mut stmt = conn.prepare(
                "SELECT vs.start_ms, vs.end_ms, vs.ts_ms FROM video_shots vs
                 JOIN files f ON f.id = vs.file_id WHERE f.path = ?1 ORDER BY vs.start_ms",
            )?;
            for row in stmt.query_map([video], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
            })? {
                let (s, e, t) = row?;
                println!("  shot {s}..{e} ms (rep @{t})");
            }
            // query with a frame from 1.5s into the video (first scene)
            let qimg = magpie_core::videos::frame_at(video, 1500)?;
            let mut qvec = sig.embed_dynamic(qimg)?;
            let norm = qvec.iter().map(|x| x * x).sum::<f32>().sqrt();
            qvec.iter_mut().for_each(|x| *x /= norm.max(1e-12));
            let store = magpie_core::search::VectorStore::load(&conn)?;
            for h in magpie_core::search::search_video_shots(&conn, &store, &qvec, 5)? {
                println!(
                    "  hit: {} {}..{}ms score {:.3} thumb={}B",
                    h.name,
                    h.start_ms,
                    h.end_ms,
                    h.score,
                    h.thumb.as_deref().map(str::len).unwrap_or(0)
                );
            }
        }
        _ => eprintln!("usage: dbtool <db> meta-get|meta-set|clips|clips-clear|index-videos ..."),
    }
    Ok(())
}
