//! 农历 in the box, computed on this machine (tyme4rs, after the 寿星天文历
//! algorithms): no network, no table to refresh.
//!
//! - `农历` / `阴历` / `nl` / `lunar`: today in the Chinese calendar, with the
//!   zodiac year, festivals and the solar term.
//! - `农历 2026-10-1`, `农历 10月1日`, `农历 明天`: the same for that day.
//! - `农历 八月十五`, `农历 腊月廿三`, `农历 2027年正月初一`, `农历 中秋`: when
//!   that is on the Gregorian calendar (the next one, unless a year is given).
//! - `节气`: the next solar term; `节气 清明` (or `农历 清明`): the next 清明.
//!
//! Arabic numerals are Gregorian dates, Chinese numerals lunar ones, so
//! `8月15日` and `八月十五` never mean the same day by accident. Statutory
//! holidays and swapped workdays are left out: they are announced each year
//! and cannot be computed.

use chrono::{Datelike, NaiveDate};
use tyme4rs::tyme::lunar::{LunarDay, LunarMonth};
use tyme4rs::tyme::solar::{SolarDay, SolarTerm};
use tyme4rs::tyme::{Culture, Tyme};

/// Years answered. The algorithms go further; dates this far out are
/// enough for a launcher and keep every answer checkable.
const YEARS: std::ops::RangeInclusive<i32> = 1900..=2100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub value: String,
    pub label: String,
    /// the input names a day that does not exist (腊月三十 in a short year)
    pub error: bool,
}

/// Festivals of the lunar calendar: names people type, and (month, day);
/// day 0 is the last day of the month (除夕 falls on 廿九 or 三十).
const FESTIVALS: &[(&str, &[&str], i32, usize)] = &[
    ("春节", &["春节", "过年", "大年初一", "新年"], 1, 1),
    ("元宵节", &["元宵", "元宵节", "上元节"], 1, 15),
    ("龙抬头", &["龙抬头", "二月二"], 2, 2),
    ("端午节", &["端午", "端午节"], 5, 5),
    ("七夕", &["七夕", "七夕节"], 7, 7),
    ("中元节", &["中元", "中元节", "鬼节"], 7, 15),
    ("中秋节", &["中秋", "中秋节"], 8, 15),
    ("重阳节", &["重阳", "重阳节"], 9, 9),
    ("腊八节", &["腊八", "腊八节"], 12, 8),
    ("小年（北方，南方多为腊月廿四）", &["小年"], 12, 23),
    ("除夕", &["除夕", "大年三十", "年三十"], 12, 0),
];

/// Verse for each solar term, five couplets to a term, in calendar order from
/// 立春. Every line, author and title was checked against published texts
/// before it went in; attributions in doubt (李白《立冬》, among others)
/// were left out. See docs/ACCEPTANCE.md.
pub const TERM_POEMS: [(&str, [(&str, &str); 5]); 24] = [
    ("立春", [
        ("律回岁晚冰霜少，春到人间草木知。", "宋·张栻《立春偶成》"),
        ("春日春盘细生菜，忽忆两京梅发时。", "唐·杜甫《立春》"),
        ("春风如贵客，一到便繁华。", "清·袁枚《春风》"),
        ("碧玉妆成一树高，万条垂下绿丝绦。", "唐·贺知章《咏柳》"),
        ("东风随春归，发我枝上花。", "唐·李白《落日忆山中》"),
    ]),
    ("雨水", [
        ("好雨知时节，当春乃发生。", "唐·杜甫《春夜喜雨》"),
        ("天街小雨润如酥，草色遥看近却无。", "唐·韩愈《早春呈水部张十八员外》"),
        ("小楼一夜听春雨，深巷明朝卖杏花。", "宋·陆游《临安春雨初霁》"),
        ("沾衣欲湿杏花雨，吹面不寒杨柳风。", "宋·志南《绝句》"),
        ("渭城朝雨浥轻尘，客舍青青柳色新。", "唐·王维《送元二使安西》"),
    ]),
    ("惊蛰", [
        ("微雨众卉新，一雷惊蛰始。", "唐·韦应物《观田家》"),
        ("残雪压枝犹有橘，冻雷惊笋欲抽芽。", "宋·欧阳修《戏答元珍》"),
        ("草长莺飞二月天，拂堤杨柳醉春烟。", "清·高鼎《村居》"),
        ("千里莺啼绿映红，水村山郭酒旗风。", "唐·杜牧《江南春》"),
        ("春风又绿江南岸，明月何时照我还。", "宋·王安石《泊船瓜洲》"),
    ]),
    ("春分", [
        ("雪入春分省见稀，半开桃李不胜威。", "宋·苏轼《癸丑春分后雪》"),
        ("竹外桃花三两枝，春江水暖鸭先知。", "宋·苏轼《惠崇春江晚景》"),
        ("几处早莺争暖树，谁家新燕啄春泥。", "唐·白居易《钱塘湖春行》"),
        ("等闲识得东风面，万紫千红总是春。", "宋·朱熹《春日》"),
        ("春眠不觉晓，处处闻啼鸟。", "唐·孟浩然《春晓》"),
    ]),
    ("清明", [
        ("清明时节雨纷纷，路上行人欲断魂。", "唐·杜牧《清明》"),
        ("梨花风起正清明，游子寻春半出城。", "宋·吴惟信《苏堤清明即事》"),
        ("春城无处不飞花，寒食东风御柳斜。", "唐·韩翃《寒食》"),
        ("燕子来时新社，梨花落后清明。", "宋·晏殊《破阵子·燕子来时新社》"),
        ("佳节清明桃李笑，野田荒冢只生愁。", "宋·黄庭坚《清明》"),
    ]),
    ("谷雨", [
        ("谷雨春光晓，山川黛色青。", "唐·元稹《咏廿四气诗·谷雨三月中》"),
        ("落红不是无情物，化作春泥更护花。", "清·龚自珍《己亥杂诗》"),
        ("唯有牡丹真国色，花开时节动京城。", "唐·刘禹锡《赏牡丹》"),
        ("人间四月芳菲尽，山寺桃花始盛开。", "唐·白居易《大林寺桃花》"),
        ("春色恼人眠不得，月移花影上栏杆。", "宋·王安石《夜直》"),
    ]),
    ("立夏", [
        ("绿树阴浓夏日长，楼台倒影入池塘。", "唐·高骈《山亭夏日》"),
        ("小荷才露尖尖角，早有蜻蜓立上头。", "宋·杨万里《小池》"),
        ("纷纷红紫已成尘，布谷声中夏令新。", "宋·陆游《初夏绝句》"),
        ("四月清和雨乍晴，南山当户转分明。", "宋·司马光《客中初夏》"),
        ("树阴满地日当午，梦觉流莺时一声。", "宋·苏舜钦《夏意》"),
    ]),
    ("小满", [
        ("夜来南风起，小麦覆陇黄。", "唐·白居易《观刈麦》"),
        ("梅子金黄杏子肥，麦花雪白菜花稀。", "宋·范成大《四时田园杂兴》"),
        ("昼出耘田夜绩麻，村庄儿女各当家。", "宋·范成大《四时田园杂兴》"),
        ("乡村四月闲人少，才了蚕桑又插田。", "宋·翁卷《乡村四月》"),
        ("麦穗初齐稚子娇，桑叶正肥蚕食饱。", "宋·欧阳修《归田园四时乐春夏二首》"),
    ]),
    ("芒种", [
        ("时雨及芒种，四野皆插秧。", "宋·陆游《时雨》"),
        ("田家少闲月，五月人倍忙。", "唐·白居易《观刈麦》"),
        ("黄梅时节家家雨，青草池塘处处蛙。", "宋·赵师秀《约客》"),
        ("手把青秧插满田，低头便见水中天。", "五代·布袋和尚《插秧偈》"),
        ("梅子黄时日日晴，小溪泛尽却山行。", "宋·曾几《三衢道中》"),
    ]),
    ("夏至", [
        ("昼晷已云极，宵漏自此长。", "唐·韦应物《夏至避暑北池》"),
        ("东边日出西边雨，道是无晴却有晴。", "唐·刘禹锡《竹枝词》"),
        ("接天莲叶无穷碧，映日荷花别样红。", "宋·杨万里《晓出净慈寺送林子方》"),
        ("竹深树密虫鸣处，时有微凉不是风。", "宋·杨万里《夏夜追凉》"),
        ("蝉噪林逾静，鸟鸣山更幽。", "南朝梁·王籍《入若耶溪》"),
    ]),
    ("小暑", [
        ("倏忽温风至，因循小暑来。", "唐·元稹《咏廿四气诗·小暑六月节》"),
        ("荷风送香气，竹露滴清响。", "唐·孟浩然《夏日南亭怀辛大》"),
        ("仲夏苦夜短，开轩纳微凉。", "唐·杜甫《夏夜叹》"),
        ("稻花香里说丰年，听取蛙声一片。", "宋·辛弃疾《西江月·夜行黄沙道中》"),
        ("人皆苦炎热，我爱夏日长。", "唐·李昂《夏日联句》"),
    ]),
    ("大暑", [
        ("大暑三秋近，林钟九夏移。", "唐·元稹《咏廿四气诗·大暑六月中》"),
        ("赤日几时过，清风无处寻。", "宋·曾几《大暑》"),
        ("何以销烦暑，端居一院中。", "唐·白居易《销暑》"),
        ("竹深留客处，荷净纳凉时。", "唐·杜甫《陪诸贵公子丈八沟携妓纳凉晚际遇雨》"),
        ("赤日炎炎似火烧，野田禾稻半枯焦。", "明·施耐庵《水浒传》"),
    ]),
    ("立秋", [
        ("乳鸦啼散玉屏空，一枕新凉一扇风。", "宋·刘翰《立秋》"),
        ("自古逢秋悲寂寥，我言秋日胜春朝。", "唐·刘禹锡《秋词》"),
        ("空山新雨后，天气晚来秋。", "唐·王维《山居秋暝》"),
        ("秋风起兮白云飞，草木黄落兮雁南归。", "汉·刘彻《秋风辞》"),
        ("解落三秋叶，能开二月花。", "唐·李峤《风》"),
    ]),
    ("处暑", [
        ("处暑无三日，新凉直万金。", "宋·苏泂《长江二首》"),
        ("离离暑云散，袅袅凉风起。", "唐·白居易《早秋曲江感怀》"),
        ("天阶夜色凉如水，卧看牵牛织女星。", "唐·杜牧《秋夕》"),
        ("萧萧梧叶送寒声，江上秋风动客情。", "宋·叶绍翁《夜书所见》"),
        ("七月流火，九月授衣。", "《诗经·豳风·七月》"),
    ]),
    ("白露", [
        ("露从今夜白，月是故乡明。", "唐·杜甫《月夜忆舍弟》"),
        ("蒹葭苍苍，白露为霜。", "《诗经·秦风·蒹葭》"),
        ("玉阶生白露，夜久侵罗袜。", "唐·李白《玉阶怨》"),
        ("中庭地白树栖鸦，冷露无声湿桂花。", "唐·王建《十五夜望月》"),
        ("秋风萧瑟天气凉，草木摇落露为霜。", "三国魏·曹丕《燕歌行》"),
    ]),
    ("秋分", [
        ("金气秋分，风清露冷秋期半。", "宋·谢逸《点绛唇·金气秋分》"),
        ("秋分客尚在，竹露夕微微。", "唐·杜甫《晚晴》"),
        ("明月几时有？把酒问青天。", "宋·苏轼《水调歌头·明月几时有》"),
        ("暮云收尽溢清寒，银汉无声转玉盘。", "宋·苏轼《阳关曲·中秋月》"),
        ("海上生明月，天涯共此时。", "唐·张九龄《望月怀远》"),
    ]),
    ("寒露", [
        ("袅袅凉风动，凄凄寒露零。", "唐·白居易《池上》"),
        ("寒露惊秋晚，朝看菊渐黄。", "唐·元稹《咏廿四气诗·寒露九月节》"),
        ("独在异乡为异客，每逢佳节倍思亲。", "唐·王维《九月九日忆山东兄弟》"),
        ("待到重阳日，还来就菊花。", "唐·孟浩然《过故人庄》"),
        ("秋阴不散霜飞晚，留得枯荷听雨声。", "唐·李商隐《宿骆氏亭寄怀崔雍崔衮》"),
    ]),
    ("霜降", [
        ("月落乌啼霜满天，江枫渔火对愁眠。", "唐·张继《枫桥夜泊》"),
        ("停车坐爱枫林晚，霜叶红于二月花。", "唐·杜牧《山行》"),
        ("荷尽已无擎雨盖，菊残犹有傲霜枝。", "宋·苏轼《赠刘景文》"),
        ("鸡声茅店月，人迹板桥霜。", "唐·温庭筠《商山早行》"),
        ("霜降水返壑，风落木归山。", "唐·白居易《岁晚》"),
    ]),
    ("立冬", [
        ("霜降向人寒，轻冰渌水漫。", "唐·元稹《咏廿四气诗·立冬十月节》"),
        ("细雨生寒未有霜，庭前木叶半青黄。", "宋·仇远《立冬即事二首》"),
        ("室小才容膝，墙低仅及肩。", "宋·陆游《立冬日作》"),
        ("绿蚁新醅酒，红泥小火炉。", "唐·白居易《问刘十九》"),
        ("寒雨连江夜入吴，平明送客楚山孤。", "唐·王昌龄《芙蓉楼送辛渐》"),
    ]),
    ("小雪", [
        ("花雪随风不厌看，更多还肯失林峦。", "唐·戴叔伦《小雪》"),
        ("莫怪虹无影，如今小雪时。", "唐·元稹《咏廿四气诗·小雪十月中》"),
        ("夜深知雪重，时闻折竹声。", "唐·白居易《夜雪》"),
        ("北风其凉，雨雪其雱。", "《诗经·邶风·北风》"),
        ("忽如一夜春风来，千树万树梨花开。", "唐·岑参《白雪歌送武判官归京》"),
    ]),
    ("大雪", [
        ("千山鸟飞绝，万径人踪灭。", "唐·柳宗元《江雪》"),
        ("燕山雪花大如席，片片吹落轩辕台。", "唐·李白《北风行》"),
        ("欲渡黄河冰塞川，将登太行雪满山。", "唐·李白《行路难》"),
        ("窗含西岭千秋雪，门泊东吴万里船。", "唐·杜甫《绝句》"),
        ("夜来城外一尺雪，晓驾炭车辗冰辙。", "唐·白居易《卖炭翁》"),
    ]),
    ("冬至", [
        ("天时人事日相催，冬至阳生春又来。", "唐·杜甫《小至》"),
        ("邯郸驿里逢冬至，抱膝灯前影伴身。", "唐·白居易《邯郸冬至夜思家》"),
        ("冬至子之半，天心无改移。", "宋·邵雍《冬至吟》"),
        ("黄钟应律好风催，阴伏阳升淑气回。", "宋·朱淑真《冬至》"),
        ("今日日南至，吾门方寂然。", "宋·陆游《辛酉冬至》"),
    ]),
    ("小寒", [
        ("小寒连大吕，欢鹊垒新巢。", "唐·元稹《咏廿四气诗·小寒十二月节》"),
        ("墙角数枝梅，凌寒独自开。", "宋·王安石《梅花》"),
        ("疏影横斜水清浅，暗香浮动月黄昏。", "宋·林逋《山园小梅》"),
        ("梅须逊雪三分白，雪却输梅一段香。", "宋·卢钺《雪梅》"),
        ("寒夜客来茶当酒，竹炉汤沸火初红。", "宋·杜耒《寒夜》"),
    ]),
    ("大寒", [
        ("旧雪未及消，新雪又拥户。", "宋·邵雍《大寒吟》"),
        ("柴门闻犬吠，风雪夜归人。", "唐·刘长卿《逢雪宿芙蓉山主人》"),
        ("爆竹声中一岁除，春风送暖入屠苏。", "宋·王安石《元日》"),
        ("岁暮阴阳催短景，天涯霜雪霁寒宵。", "唐·杜甫《阁夜》"),
        ("年年雪里，常插梅花醉。", "宋·李清照《清平乐·年年雪里》"),
    ]),
];

/// The couplet for `term` in `year`: one of its five, the same all year (so
/// the reminder and the palette show the same one) and a different one from
/// year to year.
pub fn poem_for(term: &str, year: i32) -> Option<(&'static str, &'static str)> {
    let (i, (_, lines)) = TERM_POEMS.iter().enumerate().find(|(_, (t, _))| *t == term)?;
    // a small integer hash: neighbouring years and terms land far apart
    let mut h = (year as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    Some(lines[(h % 5) as usize])
}

/// The solar term that begins on `day`, if one does.
pub fn term_starting(day: NaiveDate) -> Option<String> {
    let td = solar(day)?.get_term_day();
    (td.get_day_index() == 0).then(|| td.get_solar_term().get_name())
}

/// Whether a 节气 reminder is due at `now`: the reminder is on, it is 09:00
/// or later on the first day of a term, and none was shown that day
/// (`last_shown` is the date of the last one, `YYYY-MM-DD`). Returns the term.
pub fn term_notice_due(now: chrono::NaiveDateTime, enabled: bool, last_shown: Option<&str>) -> Option<String> {
    if !enabled || now.time() < chrono::NaiveTime::from_hms_opt(9, 0, 0)? {
        return None;
    }
    let day = now.date();
    if last_shown == Some(day.format("%Y-%m-%d").to_string().as_str()) {
        return None;
    }
    term_starting(day)
}

/// A reminder to show: the day it is for (`YYYY-MM-DD`, to record), the term
/// and its couplet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermNotice {
    pub day: String,
    pub term: String,
    pub line: &'static str,
    pub source: &'static str,
}

/// Local time, or the clock a test sets in `MAGPIE_TEST_NOW`
/// (`2026-10-08T09:30`): the palette, the reminder and the tips then agree
/// on what day it is.
pub fn local_now() -> chrono::NaiveDateTime {
    clock_or_now(std::env::var("MAGPIE_TEST_NOW").ok().as_deref())
}

fn clock_or_now(clock: Option<&str>) -> chrono::NaiveDateTime {
    clock
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok())
        .unwrap_or_else(|| chrono::Local::now().naive_local())
}

/// [`term_notice_due`] against the local clock, or against `clock` when a
/// test sets one (`2026-10-08T09:30`).
pub fn term_notice_now(clock: Option<&str>, enabled: bool, last_shown: Option<&str>) -> Option<TermNotice> {
    let now = clock_or_now(clock);
    let term = term_notice_due(now, enabled, last_shown)?;
    let (line, source) = poem_for(&term, now.date().year())?;
    Some(TermNotice { day: now.date().format("%Y-%m-%d").to_string(), term, line, source })
}

/// What the empty search box shows on its right (#13): today, the weekday
/// (1 = Monday) and the Chinese-calendar day. The page words it in the
/// interface language.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TodayLine {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub weekday: u32,
    pub lunar: Option<String>,
}

pub fn today_line(now: chrono::NaiveDateTime) -> TodayLine {
    let d = now.date();
    TodayLine {
        year: d.year(),
        month: d.month(),
        day: d.day(),
        weekday: d.weekday().number_from_monday(),
        lunar: lunar_short(d),
    }
}

/// `day` in the Chinese calendar, short, for the date line on the empty box
/// (#13): the month and day (八月廿四), then the day's festivals and the
/// solar term when one begins (八月十五 · 中秋节). None outside the years
/// answered.
pub fn lunar_short(day: NaiveDate) -> Option<String> {
    let s = solar(day)?;
    let l = s.get_lunar_day();
    let mut parts = vec![format!("{}{}", l.get_lunar_month().get_name(), l.get_name())];
    for name in [l.get_festival().map(|f| f.get_name()), s.get_festival().map(|f| f.get_name())].into_iter().flatten() {
        parts.push(name);
    }
    let td = s.get_term_day();
    if td.get_day_index() == 0 {
        parts.push(td.get_solar_term().get_name());
    }
    Some(parts.join(" · "))
}

/// The term that begins on `day` with this year's couplet, for the tips on
/// the empty box (all day, not only from nine).
pub fn term_of_day(day: NaiveDate) -> Option<TermNotice> {
    let term = term_starting(day)?;
    let (line, source) = poem_for(&term, day.year())?;
    Some(TermNotice { day: day.format("%Y-%m-%d").to_string(), term, line, source })
}

pub fn answer(query: &str, today: NaiveDate) -> Option<Answer> {
    let q = query.trim();
    if let Some(rest) = strip_verb(q, &["节气"], &[]) {
        return terms(rest, today);
    }
    let rest = strip_verb(q, &["农历", "阴历"], &["nl", "lunar"])?;
    if rest.is_empty() {
        return Some(solar_to_lunar(today, today));
    }
    if let Some(day) = parse_solar(rest, today) {
        return Some(solar_to_lunar(day, today));
    }
    if let Some(a) = festival(rest, today) {
        return Some(a);
    }
    if term_names(today).iter().any(|n| n == rest) {
        return terms(rest, today);
    }
    let (year, month, day) = parse_lunar(rest)?;
    Some(lunar_to_solar(year, month, day, None, today))
}

/// The text after a verb: Chinese verbs may run straight into it (`农历八月十五`),
/// Latin ones need a space (`nl 10-1`, not `nlp`).
fn strip_verb<'a>(q: &'a str, joined: &[&str], spaced: &[&str]) -> Option<&'a str> {
    for v in joined {
        if let Some(rest) = q.strip_prefix(v) {
            return Some(rest.trim());
        }
    }
    let lower = q.to_lowercase();
    for v in spaced {
        if lower == *v {
            return Some("");
        }
        if lower.starts_with(v) && q[v.len()..].starts_with(char::is_whitespace) {
            return Some(q[v.len()..].trim());
        }
    }
    None
}

fn solar(d: NaiveDate) -> Option<SolarDay> {
    YEARS.contains(&d.year()).then_some(())?;
    SolarDay::new(d.year() as isize, d.month() as usize, d.day() as usize).ok()
}

fn naive(s: &SolarDay) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(s.get_year() as i32, s.get_month() as u32, s.get_day() as u32)
}

/// "2026年9月25日 周五"
fn show(d: NaiveDate) -> String {
    const WEEK: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];
    format!("{}年{}月{}日 周{}", d.year(), d.month(), d.day(), WEEK[d.weekday().num_days_from_monday() as usize])
}

fn countdown(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "就是今天".into(),
        n if n > 0 => format!("还有 {n} 天"),
        n => format!("{} 天前", -n),
    }
}

/// `2026-10-1`, `2026/10/1`, `2026.10.1`, `10-1`, `2026年10月1日`, `10月1号`,
/// `今天` / `明天` / `后天` / `昨天` / `前天`.
fn parse_solar(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let shift = match s {
        "今天" => Some(0),
        "明天" => Some(1),
        "后天" => Some(2),
        "昨天" => Some(-1),
        "前天" => Some(-2),
        _ => None,
    };
    if let Some(n) = shift {
        return today.checked_add_signed(chrono::Duration::days(n));
    }
    let t = s.trim_end_matches(['日', '号']).replace(['年', '月', '/', '.'], "-");
    let parts: Vec<&str> = t.split('-').collect();
    if parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.len() > 4) {
        return None;
    }
    let n: Vec<u32> = parts.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let (y, m, d) = match n[..] {
        [m, d] => (today.year(), m, d),
        [y, m, d] if parts[0].len() == 4 => (y as i32, m, d),
        _ => return None,
    };
    // a year out of range still parses: the row then says which years work
    NaiveDate::from_ymd_opt(y, m, d)
}

/// `八月十五`, `闰六月初一`, `腊月廿三`, `2027年正月初一`, `2027 正月初一`:
/// (year if given, month, negative for a leap month as tyme4rs has it, day).
fn parse_lunar(s: &str) -> Option<(Option<i32>, i32, usize)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let (year, s) = if digits == 4 {
        let y: i32 = s[..4].parse().ok()?;
        (Some(y), s[4..].trim_start_matches('年').trim())
    } else if digits == 0 {
        (None, s)
    } else {
        return None;
    };
    let (leap, s) = match s.strip_prefix('闰') {
        Some(r) => (true, r),
        None => (false, s),
    };
    const MONTHS: &[(&str, i32)] = &[
        ("十一", 11), ("十二", 12), ("正", 1), ("一", 1), ("二", 2), ("三", 3), ("四", 4), ("五", 5),
        ("六", 6), ("七", 7), ("八", 8), ("九", 9), ("十", 10), ("冬", 11), ("腊", 12),
    ];
    let (m, s) = MONTHS.iter().find_map(|(name, m)| s.strip_prefix(name).and_then(|r| r.strip_prefix('月')).map(|r| (*m, r)))?;
    let d = lunar_day_number(s.trim_end_matches('日'))?;
    Some((year, if leap { -m } else { m }, d))
}

/// 初一 … 初十, 十一 … 十九, 二十, 廿一 … 廿九 (or 二十一), 三十.
fn lunar_day_number(s: &str) -> Option<usize> {
    const DIGITS: [&str; 10] = ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];
    let digit = |t: &str| DIGITS.iter().position(|d| *d == t).map(|i| i + 1);
    let ones = |r: &str| digit(r).filter(|n| *n < 10);
    let d = match s {
        "十" => 10,
        "二十" => 20,
        "三十" => 30,
        _ => match (s.strip_prefix('初'), s.strip_prefix("二十").or_else(|| s.strip_prefix('廿')), s.strip_prefix('十')) {
            (Some(r), _, _) => digit(r)?,
            (_, Some(r), _) => 20 + ones(r)?,
            (_, _, Some(r)) => 10 + ones(r)?,
            _ => return None,
        },
    };
    (1..=30).contains(&d).then_some(d)
}

/// Today (or another day) in the Chinese calendar.
fn solar_to_lunar(day: NaiveDate, today: NaiveDate) -> Answer {
    let Some(s) = solar(day) else {
        return out_of_range();
    };
    let l = s.get_lunar_day();
    let year = l.get_lunar_month().get_lunar_year();
    let zodiac = year.get_sixty_cycle().get_earth_branch().get_zodiac().get_name();
    let mut label = Vec::new();
    if day != today {
        label.push(show(day));
    }
    label.push(format!("{zodiac}年"));
    for name in [l.get_festival().map(|f| f.get_name()), s.get_festival().map(|f| f.get_name())].into_iter().flatten() {
        label.push(name);
    }
    let td = s.get_term_day();
    let term = td.get_solar_term();
    label.push(if td.get_day_index() == 0 { format!("今天{}", term.get_name()) } else { format!("{}第 {} 天", term.get_name(), td.get_day_index() + 1) });
    let next = term.next(1);
    if let Some(nd) = naive(&next.get_solar_day()) {
        let when = format!("下个节气{} {}月{}日", next.get_name(), nd.month(), nd.day());
        label.push(if day == today { format!("{when}（{}）", countdown(nd, today)) } else { when });
    }
    Answer { value: l.to_string(), label: label.join(" · "), error: false }
}

/// When a lunar date falls: in `year` if given, else the next time from today.
fn lunar_to_solar(year: Option<i32>, month: i32, day: usize, festival: Option<&str>, today: NaiveDate) -> Answer {
    let this_year = match solar(today) {
        Some(s) => s.get_lunar_day().get_lunar_month().get_lunar_year().get_year() as i32,
        None => return out_of_range(),
    };
    let years: Vec<i32> = match year {
        Some(y) => vec![y],
        // a leap month comes back every few years, some (闰正月) far apart
        None if month < 0 => (this_year..this_year + 60).collect(),
        None => (this_year..this_year + 3).collect(),
    };
    for y in years {
        if !YEARS.contains(&y) {
            return out_of_range();
        }
        let Ok(m) = LunarMonth::new(y as isize, month as isize) else { continue };
        let d = if day == 0 { m.get_day_count() } else { day };
        let Ok(l) = LunarDay::new(y as isize, month as isize, d) else { continue };
        let Some(date) = naive(&l.get_solar_day()) else { continue };
        if year.is_none() && date < today {
            continue;
        }
        let mut label = Vec::new();
        if let Some(f) = festival {
            label.push(f.to_string());
        }
        label.push(l.to_string());
        label.push(countdown(date, today));
        return Answer { value: show(date), label: label.join(" · "), error: false };
    }
    let what = format!("{}{}月{}", if month < 0 { "闰" } else { "" }, month_name(month.abs()), day_name(day));
    Answer {
        value: match year {
            Some(y) => format!("农历 {y} 年没有{what}"),
            None => format!("近几年农历没有{what}"),
        },
        label: "农历".into(),
        error: true,
    }
}

fn month_name(m: i32) -> &'static str {
    ["正", "二", "三", "四", "五", "六", "七", "八", "九", "十", "冬", "腊"][(m.clamp(1, 12) - 1) as usize]
}

fn day_name(d: usize) -> String {
    const N: [&str; 10] = ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];
    match d {
        1..=10 => format!("初{}", N[d - 1]),
        11..=19 => format!("十{}", N[d - 11]),
        20 => "二十".into(),
        21..=29 => format!("廿{}", N[d - 21]),
        30 => "三十".into(),
        _ => d.to_string(),
    }
}

fn festival(s: &str, today: NaiveDate) -> Option<Answer> {
    let (name, _, m, d) = FESTIVALS.iter().find(|(_, names, _, _)| names.contains(&s))?;
    Some(lunar_to_solar(None, *m, *d, Some(name), today))
}

fn term_names(today: NaiveDate) -> Vec<String> {
    (0..24).map(|i| SolarTerm::from_index(today.year() as isize, i).get_name()).collect()
}

/// `节气`: the next one; `节气 清明`: the next 清明 (today counts).
fn terms(rest: &str, today: NaiveDate) -> Option<Answer> {
    let Some(s) = solar(today) else {
        return Some(out_of_range());
    };
    let td = s.get_term_day();
    let now = td.get_solar_term();
    if rest.is_empty() {
        let next = now.next(1);
        let date = naive(&next.get_solar_day())?;
        let current = if td.get_day_index() == 0 { format!("今天{}", now.get_name()) } else { format!("现在是{}第 {} 天", now.get_name(), td.get_day_index() + 1) };
        return Some(Answer {
            value: format!("{} {}", next.get_name(), show(date)),
            label: format!("下个节气 · {} · {current}", countdown(date, today)),
            error: false,
        });
    }
    if !term_names(today).iter().any(|n| n == rest) {
        return None;
    }
    (0..=24).map(|k| now.next(k)).find_map(|t| {
        let date = naive(&t.get_solar_day())?;
        if t.get_name() != rest || date < today {
            return None;
        }
        let (line, source) = poem_for(rest, date.year())?;
        // on the day itself the verse is the answer (the reminder opens here)
        Some(if date == today {
            Answer { value: line.into(), label: format!("今日{rest} · {} · {source}", show(date)), error: false }
        } else {
            Answer { value: format!("{rest} {}", show(date)), label: format!("节气 · {} · {line}", countdown(date, today)), error: false }
        })
    })
}

fn out_of_range() -> Answer {
    Answer { value: format!("农历只算 {} 到 {} 年", YEARS.start(), YEARS.end()), label: "农历".into(), error: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Dates checked against the published calendar (春节, 中秋, the leap
    /// months of 2023, 2025 and the much-discussed 2033, solar terms).
    #[test]
    fn known_dates_come_out_right() {
        let t = day(2026, 9, 29);
        let a = answer("农历", t).unwrap();
        assert_eq!(a.value, "农历丙午年八月十九");
        assert!(a.label.starts_with("马年 · 秋分第 7 天 · 下个节气寒露 10月8日（还有 9 天）"), "{}", a.label);
        for (q, v) in [
            ("农历 2026-2-17", "农历丙午年正月初一"),
            ("农历 2025/7/25", "农历乙巳年闰六月初一"),
            ("农历 2023年3月22日", "农历癸卯年闰二月初一"),
            ("农历2033-12-22", "农历癸丑年闰十一月初一"),
            ("农历 1900-01-31", "农历庚子年正月初一"),
            ("nl 10-1", "农历丙午年八月廿一"),
            ("lunar 明天", "农历丙午年八月二十"),
        ] {
            assert_eq!(answer(q, t).map(|a| a.value), Some(v.to_string()), "{q}");
        }
        let spring = answer("农历 2026-2-17", t).unwrap();
        assert!(spring.label.contains("春节") && spring.label.contains("2026年2月17日 周二"), "{}", spring.label);
    }

    #[test]
    fn lunar_dates_and_festivals_find_their_day() {
        let t = day(2026, 9, 29);
        // 中秋 2026 was on 9-25: the next one is in 2027
        let a = answer("农历 中秋", t).unwrap();
        assert_eq!(a.value, "2027年9月15日 周三");
        assert!(a.label.starts_with("中秋节 · 农历丁未年八月十五 · 还有 351 天"), "{}", a.label);
        assert_eq!(answer("农历八月十九", t).unwrap().label.split(" · ").last(), Some("就是今天"));
        assert_eq!(answer("农历 2027年正月初一", t).unwrap().value, "2027年2月6日 周六");
        assert_eq!(answer("农历 2026 正月初一", t).unwrap().label.split(" · ").last(), Some("224 天前"));
        assert_eq!(answer("农历 腊月廿三", t).unwrap().value, "2027年1月30日 周六");
        assert_eq!(answer("农历 二十三", t), None, "no month: not a lunar date");
        // 除夕: the last day of 腊月, 廿九 in some years
        assert_eq!(answer("农历 除夕", t).unwrap().value, "2027年2月5日 周五");
        assert_eq!(answer("农历 除夕", day(2025, 3, 1)).unwrap().value, "2026年2月16日 周一");
        // a leap month: the next year that has one (2036, per .NET too)
        assert_eq!(answer("农历 闰六月初一", t).unwrap().value, "2036年7月23日 周三");
    }

    #[test]
    fn every_day_name_reads_back() {
        for d in 1..=30 {
            assert_eq!(lunar_day_number(&day_name(d)), Some(d), "{}", day_name(d));
        }
        assert_eq!(lunar_day_number("二十一"), Some(21));
        for bad in ["", "初", "初十一", "十十", "廿十", "三十一", "四十", "卅", "15"] {
            assert_eq!(lunar_day_number(bad), None, "{bad:?}");
        }
        for m in 1..=12 {
            let (_, got, _) = parse_lunar(&format!("{}月初一", month_name(m))).unwrap();
            assert_eq!(got, m, "{}", month_name(m));
        }
    }

    #[test]
    fn a_day_that_does_not_exist_says_so() {
        let t = day(2026, 9, 29);
        let a = answer("农历 2026年腊月三十", t).unwrap();
        assert!(a.error && a.value.contains("没有腊月三十"), "{a:?}");
        assert!(answer("农历 2300-1-1", t).unwrap().error);
    }

    #[test]
    fn the_short_lunar_date() {
        assert_eq!(lunar_short(day(2026, 10, 5)).as_deref(), Some("八月廿五"));
        assert_eq!(lunar_short(day(2026, 9, 25)).as_deref(), Some("八月十五 · 中秋节"), "a festival comes along");
        assert_eq!(lunar_short(day(2026, 10, 8)).as_deref(), Some("八月廿八 · 寒露"), "a term on the day it begins");
        assert_eq!(lunar_short(day(2026, 10, 9)).as_deref(), Some("八月廿九"), "not on the days after");
        assert_eq!(lunar_short(day(1800, 1, 1)), None);
        let t = today_line(day(2026, 10, 5).and_hms_opt(23, 40, 0).unwrap());
        assert_eq!(
            t,
            TodayLine { year: 2026, month: 10, day: 5, weekday: 1, lunar: Some("八月廿五".into()) }
        );
    }

    #[test]
    fn solar_terms() {
        let t = day(2026, 9, 29);
        let a = answer("节气", t).unwrap();
        assert_eq!(a.value, "寒露 2026年10月8日 周四");
        assert_eq!(a.label, "下个节气 · 还有 9 天 · 现在是秋分第 7 天");
        assert_eq!(answer("节气 清明", t).unwrap().value, "清明 2027年4月5日 周一");
        assert_eq!(answer("农历 冬至", t).unwrap().value, "冬至 2026年12月22日 周二");
        // on the day: the verse is the answer, the date and source beside it
        let a = answer("节气 秋分", day(2026, 9, 23)).unwrap();
        let (line, source) = poem_for("秋分", 2026).unwrap();
        assert_eq!(a.value, line);
        assert_eq!(a.label, format!("今日秋分 · 2026年9月23日 周三 · {source}"));
        // before it: the date, with the verse in the row
        assert!(answer("节气 清明", t).unwrap().label.ends_with(poem_for("清明", 2027).unwrap().0));
    }

    /// 24 terms, five couplets each, every one with a source, none twice.
    #[test]
    fn the_verse_table_is_whole() {
        let names: std::collections::BTreeSet<String> = (0..24).map(|i| SolarTerm::from_index(2026, i).get_name()).collect();
        let mine: std::collections::BTreeSet<String> = TERM_POEMS.iter().map(|(t, _)| t.to_string()).collect();
        assert_eq!(mine, names, "exactly the 24 solar terms");
        let mut seen = std::collections::HashSet::new();
        for (term, lines) in TERM_POEMS {
            for (line, source) in lines {
                // two halves: 「…，…。」, or a question first (明月几时有？把酒问青天。)
                let middle = line.trim_end_matches(['。', '？']);
                assert!(middle.contains(['，', '？']) && (line.ends_with('。') || line.ends_with('？')), "{term}: {line}");
                assert!(source.contains('《') && source.ends_with('》'), "{term}: {source}");
                assert!(seen.insert(line), "{line} appears twice");
            }
        }
        assert_eq!(seen.len(), 120);
    }

    /// The pick is fixed for a year and moves between years: over a century
    /// every couplet of every term comes up.
    #[test]
    fn each_year_picks_one_couplet_and_all_of_them_come_up() {
        for (term, lines) in TERM_POEMS {
            assert_eq!(poem_for(term, 2026), poem_for(term, 2026));
            let used: std::collections::HashSet<_> = (2000..2100).filter_map(|y| poem_for(term, y)).collect();
            assert_eq!(used.len(), 5, "{term}");
            assert!(lines.contains(&poem_for(term, 2026).unwrap()));
        }
        assert_eq!(poem_for("端午", 2026), None);
    }

    #[test]
    fn a_reminder_is_due_once_on_the_first_day_after_nine() {
        let at = |d: NaiveDate, h: u32, m: u32| d.and_hms_opt(h, m, 0).unwrap();
        let hanlu = day(2026, 10, 8);
        assert_eq!(term_starting(hanlu).as_deref(), Some("寒露"));
        assert_eq!(term_starting(day(2026, 10, 9)), None, "the second day is not a start");
        assert_eq!(term_starting(day(2026, 12, 22)).as_deref(), Some("冬至"));
        assert_eq!(term_notice_due(at(hanlu, 9, 0), true, None).as_deref(), Some("寒露"));
        assert_eq!(term_notice_due(at(hanlu, 8, 59), true, None), None, "not before nine");
        assert_eq!(term_notice_due(at(hanlu, 23, 30), true, Some("2026-09-23")).as_deref(), Some("寒露"));
        assert_eq!(term_notice_due(at(hanlu, 10, 0), true, Some("2026-10-08")), None, "once a day");
        assert_eq!(term_notice_due(at(hanlu, 10, 0), false, None), None, "switched off");
        assert_eq!(term_notice_due(at(day(2026, 10, 9), 10, 0), true, None), None, "a missed day is not made up");
        // every term of a year starts on exactly one day
        let mut starts = 0;
        let mut d = day(2026, 1, 1);
        while d.year() == 2026 {
            starts += usize::from(term_starting(d).is_some());
            d = d.succ_opt().unwrap();
        }
        assert_eq!(starts, 24);
        // the clock a test sets, and the couplet of that year
        let n = term_notice_now(Some("2026-10-08T09:30"), true, None).unwrap();
        assert_eq!((n.day.as_str(), n.term.as_str()), ("2026-10-08", "寒露"));
        assert_eq!((n.line, n.source), poem_for("寒露", 2026).unwrap());
        assert_eq!(term_notice_now(Some("2026-10-08T09:30"), true, Some("2026-10-08")), None);
        assert_eq!(term_notice_now(Some("2026-10-08T08:30"), true, None), None);
        // the tips: all day on the first day, nothing the day after
        let t = term_of_day(hanlu).unwrap();
        assert_eq!((t.term.as_str(), t.line), ("寒露", poem_for("寒露", 2026).unwrap().0));
        assert_eq!(term_of_day(day(2026, 10, 9)), None);
    }

    /// Anything else stays a search.
    #[test]
    fn other_input_is_not_taken() {
        let t = day(2026, 9, 29);
        for q in ["nlp", "lunar lander", "农历新年.pdf", "节气表", "农历 8月15", "农历 abc", "月饼", "", "nl 2026-2-30", "农历 13-1"] {
            let a = answer(q, t);
            // "农历 8月15" is a Gregorian date: it answers, as a Gregorian one
            if q == "农历 8月15" {
                assert_eq!(a.map(|a| a.value), Some("农历丙午年七月初三".to_string()));
                continue;
            }
            assert_eq!(a, None, "{q:?}");
        }
    }
}
