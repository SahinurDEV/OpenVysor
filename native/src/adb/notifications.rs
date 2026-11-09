//! Reads the notifications currently showing on the phone.
//!
//! Their text is private, so this runs only after the user turns it on, and
//! nothing read here is written to disk or to the log.

use std::time::Duration;

use super::Adb;
use crate::error::Result;

const TIMEOUT: Duration = Duration::from_secs(10);
/// These post status items such as "USB debugging connected", not messages.
const IGNORED_PACKAGES: [&str; 2] = ["android", "com.android.systemui"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneNotification {
    pub package: String,
    pub title: String,
    pub text: String,
}

/// Extracts `X` from `String (X)` or `SpannableString (X)`.
fn extra_value(raw: &str) -> Option<String> {
    let (_, value) = raw.split_once(" (")?;
    let value = value.strip_suffix(')').unwrap_or(value).trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Parses `dumpsys notification --noredact`, using only the active list.
pub fn parse_notifications(output: &str) -> Vec<PhoneNotification> {
    let mut found: Vec<PhoneNotification> = Vec::new();
    let mut in_list = false;
    let mut current: Option<PhoneNotification> = None;
    let mut finish = |item: Option<PhoneNotification>| {
        let Some(item) = item else { return };
        let useful = !(item.title.is_empty() && item.text.is_empty()) && !IGNORED_PACKAGES.contains(&item.package.as_str());
        if useful && !found.contains(&item) {
            found.push(item);
        }
    };
    for line in output.lines() {
        let trimmed = line.trim();
        if !in_list {
            in_list = trimmed == "Notification List:";
            continue;
        }
        if !trimmed.is_empty() && indent(line) <= 2 {
            break;
        }
        if trimmed.starts_with("NotificationRecord(") {
            finish(current.take());
            let package = trimmed.split("pkg=").nth(1).and_then(|rest| rest.split_whitespace().next());
            current = package.map(|package| PhoneNotification { package: package.to_string(), title: String::new(), text: String::new() });
        } else if let Some(item) = &mut current {
            if let Some(raw) = trimmed.strip_prefix("android.title=") {
                item.title = extra_value(raw).unwrap_or_default();
            } else if let Some(raw) = trimmed.strip_prefix("android.text=") {
                item.text = extra_value(raw).unwrap_or_default();
            }
        }
    }
    finish(current);
    found
}

pub fn fetch(adb: &Adb, serial: &str) -> Result<Vec<PhoneNotification>> {
    Ok(parse_notifications(&adb.device_text(serial, &["shell", "dumpsys", "notification", "--noredact"], TIMEOUT)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = "Current Notification Manager state:
  Notification List:
    NotificationRecord(0x0a: pkg=com.whatsapp user=UserHandle{0} id=1 tag=null importance=4)
      uid=10123 userId=0
      extras={
        android.title=String (Alice)
        android.text=SpannableString (See you at 5 (maybe))
        android.subText=null
      }
    NotificationRecord(0x0b: pkg=com.android.systemui user=UserHandle{0} id=2 tag=null importance=2)
      extras={
        android.title=String (USB debugging connected)
      }
    NotificationRecord(0x0c: pkg=com.example.empty user=UserHandle{0} id=3 tag=null importance=2)
      extras={
        android.title=null
      }
    NotificationRecord(0x0d: pkg=com.whatsapp user=UserHandle{0} id=4 tag=null importance=4)
      extras={
        android.title=String (Alice)
        android.text=SpannableString (See you at 5 (maybe))
      }
  Snoozed notifications:
    NotificationRecord(0x0e: pkg=com.snoozed user=UserHandle{0} id=5 tag=null importance=2)
      extras={
        android.title=String (Later)
      }
";

    #[test]
    fn reads_the_active_list_without_noise_or_repeats() {
        let items = parse_notifications(OUTPUT);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].package, "com.whatsapp");
        assert_eq!(items[0].title, "Alice");
        assert_eq!(items[0].text, "See you at 5 (maybe)");
    }

    #[test]
    fn returns_nothing_for_unexpected_output() {
        assert!(parse_notifications("").is_empty());
        assert!(parse_notifications("Permission Denial").is_empty());
    }
}
