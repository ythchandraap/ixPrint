use std::fmt;
use std::str::FromStr;

/// 1-based page ranges, e.g. `1-3,5,8-` (open end = until the last page).
#[derive(Clone, Debug, PartialEq)]
pub struct PageRange(Vec<(u32, Option<u32>)>);

impl PageRange {
    /// Selected 0-based page indexes, in the order written; out-of-range pages are skipped.
    pub fn indexes(&self, page_count: u32) -> Vec<u32> {
        let mut out = Vec::new();
        for &(start, end) in &self.0 {
            let end = end.unwrap_or(page_count).min(page_count);
            for p in start..=end {
                if p >= 1 && p <= page_count {
                    out.push(p - 1);
                }
            }
        }
        out
    }
}

impl FromStr for PageRange {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let invalid = || format!("Pages '{}' are invalid (e.g. 1-3,5,8-)", s);
        let mut ranges = Vec::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let range = match part.split_once('-') {
                Some((a, b)) => {
                    let start: u32 = a.trim().parse().map_err(|_| invalid())?;
                    let end = match b.trim() {
                        "" => None,
                        b => Some(b.parse::<u32>().map_err(|_| invalid())?),
                    };
                    (start, end)
                }
                None => {
                    let p: u32 = part.parse().map_err(|_| invalid())?;
                    (p, Some(p))
                }
            };
            if range.0 == 0 || range.1.is_some_and(|e| e < range.0) {
                return Err(invalid());
            }
            ranges.push(range);
        }
        if ranges.is_empty() {
            return Err(invalid());
        }
        Ok(PageRange(ranges))
    }
}

impl fmt::Display for PageRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|&(start, end)| match end {
                Some(e) if e == start => start.to_string(),
                Some(e) => format!("{}-{}", start, e),
                None => format!("{}-", start),
            })
            .collect();
        f.write_str(&parts.join(","))
    }
}
string_serde!(PageRange);
