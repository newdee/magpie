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
        _ => eprintln!("usage: dbtool <db> meta-get|meta-set|clips|clips-clear ..."),
    }
    Ok(())
}
