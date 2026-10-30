//! 各运行时的「停止更新日期」时间表。
//!
//! 数据来自 endoflife.date 的公开接口，按产品线一次拉全、缓存一天。
//! 拉不到就整条链路降级成"不知道"（返回 None），界面照常列版本，只是不标 EOL，
//! 绝不因为时间表失败就让版本清单报错。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// 一个版本线（cycle）的维护状况。
#[derive(Debug, Clone)]
pub struct Row {
    /// 停止普通维护的日期，endoflife.date 的 support 字段；没有就用 eol。
    pub support_end: Option<String>,
    pub eol_date: Option<String>,
    /// 已经停止更新；None = 时间表没这一条。
    pub eol: Option<bool>,
    pub lts: bool,
}

#[derive(Debug, Clone)]
pub struct Schedule(HashMap<String, Row>);

impl Schedule {
    pub fn get(&self, cycle: &str) -> Option<&Row> {
        self.0.get(cycle)
    }
}

/// 到期日判断：日期字符串（YYYY-MM-DD）跟今天比。
pub fn is_past(date: &str) -> bool {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    date <= today.as_str()
}

type Cache = HashMap<String, (Instant, Schedule)>;

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const TTL: Duration = Duration::from_secs(86_400);

#[derive(Debug, Deserialize)]
struct Raw {
    cycle: String,
    #[serde(default)]
    lts: BoolOrStr,
    #[serde(default)]
    support: DateOrBool,
    #[serde(default)]
    eol: DateOrBool,
}

/// 这些字段官方写法是"日期字符串 / true / false"混着来，两种都得收。
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(untagged)]
enum DateOrBool {
    #[default]
    Unknown,
    Str(String),
    Flag(bool),
}

#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
enum BoolOrStr {
    #[default]
    Off,
    Flag(bool),
    Str(String),
}

fn date_of(v: &DateOrBool) -> Option<String> {
    match v {
        DateOrBool::Str(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// 时间表里的 lts 可能写成 true，也可能写成年份这样的标记，非空都算长期支持。
fn is_lts(v: &BoolOrStr) -> bool {
    match v {
        BoolOrStr::Flag(b) => *b,
        BoolOrStr::Str(s) => !s.is_empty(),
        BoolOrStr::Off => false,
    }
}

pub async fn schedule_for(client: &reqwest::Client, product: &str) -> Option<Schedule> {
    if let Ok(guard) = cache().lock()
        && let Some((at, map)) = guard.get(product)
        && at.elapsed() < TTL
    {
        return Some(map.clone());
    }
    let url = format!("https://endoflife.date/api/{product}.json");
    let resp = client
        .get(&url)
        .header("User-Agent", "envswitch/0.1")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let rows: Vec<Raw> = serde_json::from_str(&text).ok()?;
    let mut map = HashMap::new();
    for r in rows {
        let eol_date = date_of(&r.eol);
        let support_end = date_of(&r.support).or_else(|| eol_date.clone());
        // eol 写成 true 表示已经停止，写成 false 表示"还没定日期"，都算已知状态。
        let eol = match (&eol_date, &r.eol) {
            (Some(d), _) => Some(is_past(d)),
            (None, DateOrBool::Flag(b)) => Some(*b),
            _ => None,
        };
        map.insert(
            r.cycle.clone(),
            Row {
                support_end,
                eol_date,
                eol,
                lts: is_lts(&r.lts),
            },
        );
    }
    if map.is_empty() {
        return None;
    }
    let out = Schedule(map);
    if let Ok(mut guard) = cache().lock() {
        guard.insert(product.to_string(), (Instant::now(), out.clone()));
    }
    Some(out)
}

/// 把时间表的结果落到一条版本上：EOL 标记 + 一句话说明。
pub fn note_for(row: Option<&Row>) -> (Option<bool>, Option<String>, Option<String>) {
    let Some(r) = row else {
        return (None, None, None);
    };
    (
        r.eol,
        r.eol_date.clone(),
        if r.eol == Some(true) {
            r.eol_date
                .as_ref()
                .map(|d| format!("这条线已于 {d} 停止更新，只用它复现老项目"))
        } else {
            None
        },
    )
}

/// 只给界面看的"维护到 X"，没过期时才提示。
pub fn supported_until(row: Option<&Row>) -> Option<String> {
    let r = row?;
    if r.eol == Some(true) {
        return None;
    }
    r.support_end.clone()
}

#[cfg(test)]
mod tests {
    use super::{DateOrBool, Raw, is_past, note_for};

    fn row(json: &str) -> Raw {
        serde_json::from_str(json).expect("时间表条目解析")
    }

    #[test]
    fn 日期与布尔两种写法都读得到() {
        let a = row(r#"{"cycle":"3.12","eol":"2028-10-02","support":"2026-10-01","lts":false}"#);
        assert_eq!(a.eol, DateOrBool::Str("2028-10-02".into()));
        let b = row(r#"{"cycle":"1.27","eol":false,"lts":false}"#);
        assert!(matches!(b.eol, DateOrBool::Flag(false)));
        let c = row(r#"{"cycle":"1.21","eol":true}"#);
        assert!(matches!(c.eol, DateOrBool::Flag(true)));
    }

    #[test]
    fn 过期与未过期判得清() {
        assert!(is_past("2000-01-01"));
        assert!(!is_past("2999-12-31"));
    }

    #[test]
    fn 没有时间表时不编造结论() {
        assert_eq!(note_for(None).0, None);
    }
}
