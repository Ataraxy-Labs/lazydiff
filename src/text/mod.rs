use std::time::{SystemTime, UNIX_EPOCH};

mod markdown;
pub(crate) use markdown::body_preview_lines;

pub(crate) fn markdown_preview_lines(body: &str, limit: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    for raw in body.replace('\r', "").lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        let line = if in_code_block {
            raw.replace('\t', "  ").trim_end().to_string()
        } else {
            trimmed
                .trim_start_matches('#')
                .trim_start_matches(['-', '*'])
                .trim()
                .to_string()
        };
        if line.is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= limit {
            break;
        }
    }
    lines
}

pub(crate) fn _wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let extra = usize::from(!current.is_empty());
        if !current.is_empty() && current.chars().count() + extra + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub(crate) fn relative_age(timestamp: &str) -> String {
    let Some((year, month, day)) = parse_yyyy_mm_dd(timestamp) else {
        return "now".to_string();
    };
    let then = days_from_civil(year, month, day);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| (duration.as_secs() / 86_400) as i64)
        .unwrap_or(then);
    let days = now.saturating_sub(then).max(0);
    match days {
        0 => "now".to_string(),
        1..=59 => format!("{days}d"),
        60..=729 => format!("{}mo", days / 30),
        _ => format!("{}y", days / 365),
    }
}

pub(crate) fn relative_unix_age(timestamp: i64) -> String {
    let now = now_stamp() as i64;
    let seconds = now.saturating_sub(timestamp).max(0);
    match seconds {
        0..=59 => "now".to_string(),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        86_400..=5_097_599 => format!("{}d", seconds / 86_400),
        5_097_600..=63_071_999 => format!("{}mo", seconds / 2_592_000),
        _ => format!("{}y", seconds / 31_536_000),
    }
}

fn parse_yyyy_mm_dd(value: &str) -> Option<(i32, u32, u32)> {
    let date = value.get(0..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    Some((year, month, day))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
}

fn now_stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
