//! RSS feed reading and rule matching.

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::settings::{Feed, RssRule};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FeedItem {
    pub feed: String,
    pub title: String,
    /// Magnet link or URL of a .torrent file.
    pub link: String,
    pub guid: String,
    pub published: String,
}

/// What we've already grabbed, so nothing downloads twice.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RssHistory {
    pub seen: HashSet<String>,
    /// rule name -> episode keys like "S02E05"
    pub episodes: HashMap<String, HashSet<String>>,
}

impl RssHistory {
    fn path() -> std::path::PathBuf {
        crate::settings::data_dir().join("rss_history.json")
    }
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::path(), t);
        }
    }
}

pub fn fetch(feed: &Feed) -> Result<Vec<FeedItem>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0")
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = client
        .get(&feed.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't reach the feed: {e}"))?
        .bytes()
        .map_err(|e| e.to_string())?;
    let channel =
        rss::Channel::read_from(&bytes[..]).map_err(|e| format!("not a valid RSS feed: {e}"))?;
    Ok(channel
        .items()
        .iter()
        .filter_map(|item| {
            let title = item.title().unwrap_or_default().trim().to_string();
            let link = torrent_link(item)?;
            Some(FeedItem {
                feed: feed.name.clone(),
                guid: item
                    .guid()
                    .map(|g| g.value().to_string())
                    .unwrap_or_else(|| link.clone()),
                published: item.pub_date().unwrap_or_default().to_string(),
                title,
                link,
            })
        })
        .collect())
}

/// Best guess at the torrent behind a feed item.
fn torrent_link(item: &rss::Item) -> Option<String> {
    // Torznab-style <torznab:attr name="magneturl" value="..."/>
    for ext in item.extensions().values() {
        for elems in ext.values() {
            for e in elems {
                if e.attrs().get("name").map(|n| n == "magneturl").unwrap_or(false) {
                    if let Some(v) = e.attrs().get("value") {
                        return Some(v.clone());
                    }
                }
            }
        }
    }
    if let Some(enc) = item.enclosure() {
        if !enc.url().is_empty() {
            return Some(enc.url().to_string());
        }
    }
    let link = item.link()?.trim();
    if link.is_empty() {
        None
    } else {
        Some(link.to_string())
    }
}

/// Season/episode found in a title, like (2, 5) for "S02E05" or "2x05".
pub fn episode_of(title: &str) -> Option<(u32, u32)> {
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(?:\bS(\d{1,2})[ ._-]?E(\d{1,3})\b|\b(\d{1,2})x(\d{1,3})\b)").unwrap()
    });
    let c = RE.captures(title)?;
    let (s, e) = if let (Some(s), Some(e)) = (c.get(1), c.get(2)) {
        (s, e)
    } else {
        (c.get(3)?, c.get(4)?)
    };
    Some((s.as_str().parse().ok()?, e.as_str().parse().ok()?))
}

/// Parses episode filters like "S02", "S02E05-", "S01E01-S01E10", "S03E02, S03E07".
fn episode_allowed(filter: &str, ep: Option<(u32, u32)>) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }
    let Some((s, e)) = ep else { return false };
    let parse = |t: &str| -> Option<(u32, Option<u32>)> {
        let t = t.trim().to_uppercase();
        let t = t.strip_prefix('S')?;
        match t.split_once('E') {
            Some((a, b)) => Some((a.parse().ok()?, Some(b.parse().ok()?))),
            None => Some((t.parse().ok()?, None)),
        }
    };
    let key = |(a, b): (u32, Option<u32>), end: bool| -> (u32, u32) {
        (a, b.unwrap_or(if end { u32::MAX } else { 0 }))
    };
    let me = (s, e);
    for part in filter.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((from, to)) = part.split_once('-') {
            let lo = parse(from).map(|p| key(p, false));
            let hi = if to.trim().is_empty() {
                Some((u32::MAX, u32::MAX))
            } else {
                parse(to).map(|p| key(p, true))
            };
            if let (Some(lo), Some(hi)) = (lo, hi) {
                if me >= lo && me <= hi {
                    return true;
                }
            }
        } else if let Some(p) = parse(part) {
            if me >= key(p, false) && me <= key(p, true) {
                return true;
            }
        }
    }
    false
}

/// Does `rule` want this item? Ignores history.
pub fn rule_matches(rule: &RssRule, item: &FeedItem) -> Result<bool, String> {
    if !rule.feeds.is_empty() && !rule.feeds.iter().any(|f| f == &item.feed) {
        return Ok(false);
    }
    let title = item.title.as_str();
    if rule.use_regex {
        if !rule.must_contain.trim().is_empty() {
            let re = Regex::new(&format!("(?i){}", rule.must_contain))
                .map_err(|e| format!("\"Must contain\" isn't a valid regex: {e}"))?;
            if !re.is_match(title) {
                return Ok(false);
            }
        }
        if !rule.must_not_contain.trim().is_empty() {
            let re = Regex::new(&format!("(?i){}", rule.must_not_contain))
                .map_err(|e| format!("\"Must not contain\" isn't a valid regex: {e}"))?;
            if re.is_match(title) {
                return Ok(false);
            }
        }
    } else {
        let lower = normalize(title);
        for word in rule.must_contain.split_whitespace() {
            if !lower.contains(&normalize(word)) {
                return Ok(false);
            }
        }
        for phrase in rule.must_not_contain.split(',') {
            let p = normalize(phrase.trim());
            if !p.is_empty() && lower.contains(&p) {
                return Ok(false);
            }
        }
    }
    Ok(episode_allowed(&rule.episodes, episode_of(title)))
}

/// Dots, underscores and dashes count as spaces when matching words.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c == '.' || c == '_' { ' ' } else { c })
        .collect()
}

/// The episode key used for duplicate checks.
pub fn episode_key(title: &str) -> Option<String> {
    episode_of(title).map(|(s, e)| format!("S{s:02}E{e:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> FeedItem {
        FeedItem {
            feed: "f".into(),
            title: title.into(),
            link: "magnet:?xt=urn:btih:abc".into(),
            guid: title.into(),
            published: String::new(),
        }
    }

    #[test]
    fn episodes() {
        assert_eq!(episode_of("Show.Name.S02E05.1080p"), Some((2, 5)));
        assert_eq!(episode_of("Show Name 2x05 720p"), Some((2, 5)));
        assert_eq!(episode_of("Movie 2024 1080p"), None);
        assert!(episode_allowed("S02", Some((2, 9))));
        assert!(!episode_allowed("S02", Some((3, 1))));
        assert!(episode_allowed("S02E05-", Some((2, 5))));
        assert!(episode_allowed("S02E05-", Some((4, 1))));
        assert!(!episode_allowed("S02E05-", Some((2, 4))));
        assert!(episode_allowed("S01E01-S01E10", Some((1, 10))));
        assert!(!episode_allowed("S01E01-S01E10", Some((1, 11))));
        assert!(episode_allowed("S03E02, S03E07", Some((3, 7))));
        assert!(episode_allowed("", None));
        assert!(!episode_allowed("S01", None));
    }

    #[test]
    fn words() {
        let rule = RssRule {
            must_contain: "show name 1080p".into(),
            must_not_contain: "CAM, HDTS".into(),
            ..Default::default()
        };
        assert!(rule_matches(&rule, &item("Show.Name.S01E01.1080p.WEB")).unwrap());
        assert!(!rule_matches(&rule, &item("Show.Name.S01E01.720p.WEB")).unwrap());
        assert!(!rule_matches(&rule, &item("Show Name S01E01 1080p HDTS")).unwrap());
        let re = RssRule {
            must_contain: r"show\.name.*(1080p|2160p)".into(),
            use_regex: true,
            ..Default::default()
        };
        assert!(rule_matches(&re, &item("Show.Name.S01E01.2160p")).unwrap());
        assert!(!rule_matches(&re, &item("Other.S01E01.2160p")).unwrap());
    }
}
