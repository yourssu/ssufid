use time::{OffsetDateTime, UtcOffset, macros::format_description};

use super::{SsufidCalendar, SsufidCalendarSiteData};

const PROD_ID: &str = "-//ssufid//calendar//KO";

pub fn to_ics(site: &SsufidCalendarSiteData) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        format!("PRODID:{PROD_ID}"),
        format!("X-WR-CALNAME:{}", escape_text(&site.title)),
        format!("X-WR-CALDESC:{}", escape_text(&site.description)),
        format!("URL:{}", site.source),
    ];

    for item in &site.items {
        lines.extend(event_lines(item));
    }

    lines.push("END:VCALENDAR".to_string());
    lines
        .into_iter()
        .map(|line| fold_line(&line))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

fn event_lines(item: &SsufidCalendar) -> Vec<String> {
    let mut lines = vec![
        "BEGIN:VEVENT".to_string(),
        format!("UID:{}", item.id),
        format!("SUMMARY:{}", escape_text(&item.title)),
        format!("DTSTAMP:{}", format_ics_datetime(item.starts_at)),
        format!("DTSTART:{}", format_ics_datetime(item.starts_at)),
    ];

    if let Some(description) = &item.description {
        lines.push(format!("DESCRIPTION:{}", escape_text(description)));
    }
    if let Some(ends_at) = item.ends_at {
        lines.push(format!("DTEND:{}", format_ics_datetime(ends_at)));
    }
    if let Some(location) = &item.location {
        lines.push(format!("LOCATION:{}", escape_text(location)));
    }
    if let Some(url) = &item.url {
        lines.push(format!("URL:{url}"));
    }

    lines.push("END:VEVENT".to_string());
    lines
}

fn format_ics_datetime(datetime: OffsetDateTime) -> String {
    let format = format_description!("[year][month][day]T[hour][minute][second]Z");
    datetime
        .to_offset(UtcOffset::UTC)
        .format(&format)
        .expect("ICS datetime format should always be valid")
}

fn escape_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\r', "")
        .replace('\n', "\\n")
}

fn fold_line(line: &str) -> String {
    const MAX_LEN: usize = 75;

    let mut folded = String::new();
    let mut current_len = 0;

    for ch in line.chars() {
        let ch_len = ch.len_utf8();
        if current_len + ch_len > MAX_LEN {
            folded.push_str("\r\n ");
            current_len = 1;
        }
        folded.push(ch);
        current_len += ch_len;
    }

    folded
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn test_calendar_site_data_to_ics() {
        let site = SsufidCalendarSiteData {
            title: "Test Calendar".to_string(),
            source: "https://example.com/calendar".to_string(),
            description: "Calendar Description".to_string(),
            items: vec![
                SsufidCalendar {
                    id: "event-1".to_string(),
                    title: "Event 1".to_string(),
                    description: Some("Description 1".to_string()),
                    starts_at: datetime!(2024-03-22 12:00:00 +09:00),
                    ends_at: Some(datetime!(2024-03-22 13:00:00 +09:00)),
                    location: Some("Seoul".to_string()),
                    url: Some("https://example.com/events/1".to_string()),
                },
                SsufidCalendar {
                    id: "event-2".to_string(),
                    title: "Event 2".to_string(),
                    description: None,
                    starts_at: datetime!(2024-03-23 09:00:00 UTC),
                    ends_at: None,
                    location: None,
                    url: None,
                },
            ],
        };

        let ics = to_ics(&site);

        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert!(ics.contains("VERSION:2.0"));
        assert!(ics.contains("PRODID:-//ssufid//calendar//KO"));
        assert!(ics.contains("X-WR-CALNAME:Test Calendar"));
        assert!(ics.contains("X-WR-CALDESC:Calendar Description"));
        assert!(ics.contains("BEGIN:VEVENT"));
        assert!(ics.contains("UID:event-1"));
        assert!(ics.contains("SUMMARY:Event 1"));
        assert!(ics.contains("DESCRIPTION:Description 1"));
        assert!(ics.contains("DTSTART:20240322T030000Z"));
        assert!(ics.contains("DTEND:20240322T040000Z"));
        assert!(ics.contains("LOCATION:Seoul"));
        assert!(ics.contains("URL:https://example.com/events/1"));
        assert!(ics.contains("UID:event-2"));
        assert!(ics.contains("SUMMARY:Event 2"));
        assert!(ics.contains("END:VCALENDAR"));
    }

    #[test]
    fn test_escape_text() {
        assert_eq!(
            escape_text("Hello, world;\nLine 2\\"),
            "Hello\\, world\\;\\nLine 2\\\\"
        );
    }
}
