//! Every day 1901-02-19 … 2100-12-31 as the Chinese calendar has it, one
//! line each: `2026-09-29 2026 8 0 19` (Gregorian date, lunar year, month,
//! leap 0/1, day). For checking chinese_calendar.rs against another
//! implementation, e.g. .NET's ChineseLunisolarCalendar.
use chrono::{Datelike, NaiveDate};
use tyme4rs::tyme::solar::SolarDay;

fn main() {
    let mut d = NaiveDate::from_ymd_opt(1901, 2, 19).unwrap();
    let end = NaiveDate::from_ymd_opt(2100, 12, 31).unwrap();
    let mut out = String::new();
    while d <= end {
        let l = SolarDay::from_ymd(d.year() as isize, d.month() as usize, d.day() as usize).get_lunar_day();
        let m = l.get_lunar_month();
        out.push_str(&format!(
            "{} {} {} {} {}\n",
            d.format("%Y-%m-%d"),
            m.get_lunar_year().get_year(),
            m.get_month(),
            u8::from(m.is_leap()),
            l.get_day()
        ));
        d = d.succ_opt().unwrap();
    }
    print!("{out}");
}
