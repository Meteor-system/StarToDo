use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::tasks::Priority;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskDraft {
    pub title: String,
    pub notes: String,
    pub planned_date: Option<String>,
    pub priority: Priority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateKind {
    Today,
    Tomorrow,
    Weekday(u8),
}

#[derive(Debug, Clone, Copy)]
struct DateKeyword {
    text: &'static str,
    kind: DateKind,
}

const DATE_KEYWORDS: &[DateKeyword] = &[
    DateKeyword {
        text: "星期一",
        kind: DateKind::Weekday(0),
    },
    DateKeyword {
        text: "星期二",
        kind: DateKind::Weekday(1),
    },
    DateKeyword {
        text: "星期三",
        kind: DateKind::Weekday(2),
    },
    DateKeyword {
        text: "星期四",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "星期五",
        kind: DateKind::Weekday(4),
    },
    DateKeyword {
        text: "星期六",
        kind: DateKind::Weekday(5),
    },
    DateKeyword {
        text: "星期日",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "星期天",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "礼拜一",
        kind: DateKind::Weekday(0),
    },
    DateKeyword {
        text: "礼拜二",
        kind: DateKind::Weekday(1),
    },
    DateKeyword {
        text: "礼拜三",
        kind: DateKind::Weekday(2),
    },
    DateKeyword {
        text: "礼拜四",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "礼拜五",
        kind: DateKind::Weekday(4),
    },
    DateKeyword {
        text: "礼拜六",
        kind: DateKind::Weekday(5),
    },
    DateKeyword {
        text: "礼拜日",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "礼拜天",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "周一",
        kind: DateKind::Weekday(0),
    },
    DateKeyword {
        text: "周二",
        kind: DateKind::Weekday(1),
    },
    DateKeyword {
        text: "周三",
        kind: DateKind::Weekday(2),
    },
    DateKeyword {
        text: "周四",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "周五",
        kind: DateKind::Weekday(4),
    },
    DateKeyword {
        text: "周六",
        kind: DateKind::Weekday(5),
    },
    DateKeyword {
        text: "周日",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "周天",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "今日",
        kind: DateKind::Today,
    },
    DateKeyword {
        text: "明日",
        kind: DateKind::Tomorrow,
    },
    DateKeyword {
        text: "今天",
        kind: DateKind::Today,
    },
    DateKeyword {
        text: "明天",
        kind: DateKind::Tomorrow,
    },
    DateKeyword {
        text: "monday",
        kind: DateKind::Weekday(0),
    },
    DateKeyword {
        text: "tuesday",
        kind: DateKind::Weekday(1),
    },
    DateKeyword {
        text: "wednesday",
        kind: DateKind::Weekday(2),
    },
    DateKeyword {
        text: "thursday",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "friday",
        kind: DateKind::Weekday(4),
    },
    DateKeyword {
        text: "saturday",
        kind: DateKind::Weekday(5),
    },
    DateKeyword {
        text: "sunday",
        kind: DateKind::Weekday(6),
    },
    DateKeyword {
        text: "tomorrow",
        kind: DateKind::Tomorrow,
    },
    DateKeyword {
        text: "today",
        kind: DateKind::Today,
    },
    DateKeyword {
        text: "mon",
        kind: DateKind::Weekday(0),
    },
    DateKeyword {
        text: "tue",
        kind: DateKind::Weekday(1),
    },
    DateKeyword {
        text: "tues",
        kind: DateKind::Weekday(1),
    },
    DateKeyword {
        text: "wed",
        kind: DateKind::Weekday(2),
    },
    DateKeyword {
        text: "thu",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "thur",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "thurs",
        kind: DateKind::Weekday(3),
    },
    DateKeyword {
        text: "fri",
        kind: DateKind::Weekday(4),
    },
    DateKeyword {
        text: "sat",
        kind: DateKind::Weekday(5),
    },
    DateKeyword {
        text: "sun",
        kind: DateKind::Weekday(6),
    },
];

fn is_boundary_before(text: &str, start: usize) -> bool {
    text[..start]
        .chars()
        .next_back()
        .is_none_or(|character| !character.is_alphanumeric())
}

fn is_boundary_after(text: &str, end: usize, keyword: &str) -> bool {
    if !keyword.is_ascii() {
        return true;
    }
    text[end..]
        .chars()
        .next()
        .is_none_or(|character| !character.is_ascii_alphanumeric())
}

fn find_keyword_at(
    text: &str,
    lower_text: &str,
    start: usize,
) -> Option<(&'static DateKeyword, usize)> {
    if !is_boundary_before(text, start) {
        return None;
    }

    DATE_KEYWORDS
        .iter()
        .filter(|keyword| {
            lower_text[start..].starts_with(keyword.text)
                && is_boundary_after(text, start + keyword.text.len(), keyword.text)
        })
        .max_by_key(|keyword| keyword.text.len())
        .map(|keyword| (keyword, start + keyword.text.len()))
}

fn weekday_from_index(index: u8) -> Weekday {
    match index {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

fn planned_date(kind: DateKind, today: NaiveDate) -> NaiveDate {
    match kind {
        DateKind::Today => today,
        DateKind::Tomorrow => today + Duration::days(1),
        DateKind::Weekday(index) => {
            let target = weekday_from_index(index).num_days_from_monday() as i64;
            let current = today.weekday().num_days_from_monday() as i64;
            today + Duration::days((target - current + 7) % 7)
        }
    }
}

fn make_draft(raw: &str, kind: Option<DateKind>, today: NaiveDate) -> Option<TaskDraft> {
    let mut lines = raw.trim().lines();
    let title = lines.next()?.trim().to_string();
    if title.is_empty() {
        return None;
    }

    let notes = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    let planned_date = kind.map(|kind| planned_date(kind, today).format("%Y-%m-%d").to_string());
    Some(TaskDraft {
        title,
        notes,
        planned_date,
        priority: Priority::None,
    })
}

pub fn parse_task_drafts(text: &str, today_local: &str) -> Result<Vec<TaskDraft>, String> {
    let today = NaiveDate::parse_from_str(today_local, "%Y-%m-%d")
        .map_err(|_| "todayLocal must be a valid YYYY-MM-DD date".to_string())?;
    if today.format("%Y-%m-%d").to_string() != today_local {
        return Err("todayLocal must be a valid YYYY-MM-DD date".to_string());
    }

    let lower_text = text.to_lowercase();
    let mut segments = Vec::new();
    let mut segment_start = 0;
    let mut segment_kind = None;
    let mut position = 0;

    while position < text.len() {
        let Some((_, character)) = text[position..].char_indices().next() else {
            break;
        };
        if let Some((keyword, marker_end)) = find_keyword_at(text, &lower_text, position) {
            if let Some(draft) = make_draft(&text[segment_start..position], segment_kind, today) {
                segments.push(draft);
            }
            segment_kind = Some(keyword.kind);
            segment_start = marker_end;
            position = marker_end;
        } else {
            position += character.len_utf8();
        }
    }

    if let Some(draft) = make_draft(&text[segment_start..], segment_kind, today) {
        segments.push(draft);
    }

    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dates(text: &str) -> Vec<(String, Option<String>)> {
        parse_task_drafts(text, "2026-08-07")
            .expect("parser should succeed")
            .into_iter()
            .map(|draft| (draft.title, draft.planned_date))
            .collect()
    }

    #[test]
    fn parses_chinese_today_tomorrow_and_segments() {
        assert_eq!(
            dates("今天 写报告\n明天 发布"),
            vec![
                ("写报告".to_string(), Some("2026-08-07".to_string())),
                ("发布".to_string(), Some("2026-08-08".to_string())),
            ]
        );
    }

    #[test]
    fn parses_english_case_insensitively() {
        assert_eq!(
            dates("TODAY review\nMonday plan"),
            vec![
                ("review".to_string(), Some("2026-08-07".to_string())),
                ("plan".to_string(), Some("2026-08-10".to_string())),
            ]
        );
    }

    #[test]
    fn weekday_on_today_stays_today_and_wraps_to_next_week() {
        assert_eq!(
            dates("周五 task\n周四 next"),
            vec![
                ("task".to_string(), Some("2026-08-07".to_string())),
                ("next".to_string(), Some("2026-08-13".to_string())),
            ]
        );
    }

    #[test]
    fn preserves_unmarked_text_and_notes() {
        let drafts =
            parse_task_drafts("普通任务\n补充说明", "2026-08-07").expect("parser should succeed");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].title, "普通任务");
        assert_eq!(drafts[0].notes, "补充说明");
        assert_eq!(drafts[0].planned_date, None);
    }

    #[test]
    fn ignores_keyword_inside_an_english_word() {
        assert_eq!(
            dates("someday todayish\n today real"),
            vec![
                ("someday todayish".to_string(), None),
                ("real".to_string(), Some("2026-08-07".to_string())),
            ]
        );
    }

    #[test]
    fn rejects_invalid_local_date() {
        assert_eq!(
            parse_task_drafts("today task", "2026-02-30").unwrap_err(),
            "todayLocal must be a valid YYYY-MM-DD date"
        );
    }
}
