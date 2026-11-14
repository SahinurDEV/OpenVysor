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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Text,
}

/// A value such as `String (first line` whose text goes on over the next lines. Values
/// other than the title and text are followed only to know where they end.
struct Unfinished {
    field: Option<Field>,
    indent: usize,
    value: String,
}

fn set(item: &mut PhoneNotification, field: Field, value: String) {
    match field {
        Field::Title => item.title = value,
        Field::Text => item.text = value,
    }
}

/// Parses `dumpsys notification --noredact`, using only the active list.
///
/// Each record can hold a `publicNotification=` copy after the real one: the version shown
/// on a locked screen, such as "WhatsApp" and "2 new messages". It is skipped.
pub fn parse_notifications(output: &str) -> Vec<PhoneNotification> {
    let mut found: Vec<PhoneNotification> = Vec::new();
    let mut in_list = false;
    let mut in_public = false;
    let mut unfinished: Option<Unfinished> = None;
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
        // Text that goes on over several lines can start at the left edge, so it is read
        // before the check for the end of the list.
        if let Some(mut open) = unfinished.take() {
            // A new key, record or section means the closing bracket never came, so the
            // text read so far is kept and that line is read as usual.
            let next_key = (indent(line) <= open.indent && trimmed.starts_with("android"))
                || trimmed.starts_with("NotificationRecord(")
                || (indent(line) == 2 && trimmed.ends_with(':'));
            if !next_key {
                let text = line.trim_end();
                open.value.push('\n');
                open.value.push_str(text.strip_suffix(')').unwrap_or(text));
                if !text.ends_with(')') {
                    unfinished = Some(open);
                    continue;
                }
            }
            if let (Some(item), Some(field)) = (&mut current, open.field) {
                set(item, field, open.value.trim().to_string());
            }
            if !next_key {
                continue;
            }
        }
        // Sections such as "Snoozed notifications:" are indented two spaces. Lines nearer
        // the edge are a value that wrapped, such as a tag that holds a line break.
        if !trimmed.is_empty() && indent(line) == 2 {
            break;
        }
        if trimmed.starts_with("NotificationRecord(") {
            finish(current.take());
            in_public = false;
            let package = trimmed.split("pkg=").nth(1).and_then(|rest| rest.split_whitespace().next());
            current = package.map(|package| PhoneNotification { package: package.to_string(), title: String::new(), text: String::new() });
        } else if trimmed == "publicNotification=" {
            in_public = true;
        } else if let Some((key, raw)) = trimmed.strip_prefix("android").and_then(|_| trimmed.split_once('=')) {
            let field = match key {
                "android.title" => Some(Field::Title),
                "android.text" => Some(Field::Text),
                _ => None,
            }
            .filter(|_| !in_public);
            let value = extra_value(raw).unwrap_or_default();
            if raw.contains(" (") && !raw.ends_with(')') {
                unfinished = Some(Unfinished { field, indent: indent(line), value });
            } else if let (Some(item), Some(field)) = (&mut current, field) {
                set(item, field, value);
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

    /// The layout printed by a Nothing Phone (3a), Android 16, with made-up packages and
    /// text. Indents, keys and the order of sections are as the phone printed them.
    const ANDROID_16: &str = "Current Notification Manager state:
  Notification List:
    NotificationRecord(0x04c00e53: pkg=com.example.bank user=UserHandle{0} id=0 tag=campaign_1 importance=3 key=0|com.example.bank|0|campaign_1|10367: Notification(channel=fcm_fallback_notification_channel shortcut=null contentView=null vibrate=null sound=null defaults=0 flags=AUTO_CANCEL color=0x00000000 vis=PRIVATE))
      uid=10367 userId=0
      opPkg=com.example.bank
      flags=AUTO_CANCEL
      notification=
            fullscreenIntent=null
            tickerText=null
            extras={
                android.title=String (Pay school fees from home )
                android.reduced.images=Boolean (true)
                android.subText=null
                android.template=String (android.app.Notification$BigTextStyle)
                android.text=String (Pay fees for school, college and university in a minute!

Open the app and choose Education.)
                android.progress=Integer (0)
                android.bigText=String (Pay fees for school, college and university in a minute!

Open the app and choose Education.)
            }
      stats=SingleNotificationStats{posttimeElapsedMs=500803822, airtimeCount=1}
      mImportance=DEFAULT
    NotificationRecord(0x06c9e899: pkg=com.whatsapp user=UserHandle{0} id=1 tag=null importance=2 key=0|com.whatsapp|1|null|10257: Notification(channel=silent_notifications_3 shortcut=null contentView=null vibrate=null sound=null defaults=0 flags=GROUP_SUMMARY color=0xff21c063 category=msg groupKey=group_key_messages vis=PRIVATE publicVersion=Notification(channel=null shortcut=null contentView=null vibrate=null sound=null defaults=0 flags=0 color=0xff21c063 category=msg vis=PRIVATE)))
      uid=10257 userId=0
      notification=
            number=2
            tickerText=Message from Sam
            extras={
                android.title=String (Weekend plans)
                android.reduced.images=Boolean (true)
                android.textLines=CharSequence[] (2)
                  [0] Sam: Are we still on for Saturday?
                  [1] Robin: Yes, 10 am
                android.subText=null
                android.template=String (android.app.Notification$InboxStyle)
                android.text=String (2 new messages)
            }
      publicNotification=
            number=0
            tickerText=null
            extras={
                android.title=String (WhatsApp)
                android.subText=null
                android.text=String (2 new messages)
            }
      stats=SingleNotificationStats{posttimeElapsedMs=500568767, airtimeCount=1}
    NotificationRecord(0x0238292d: pkg=com.whatsapp user=UserHandle{0} id=1 tag=QmFzZTY0IHRhZw==
 importance=4 key=0|com.whatsapp|1|QmFzZTY0IHRhZw==
|10257: Notification(channel=group_chat_defaults_2 shortcut=null contentView=null vibrate=null sound=null defaults=0 flags=0 color=0xff21c063 category=msg groupKey=group_key_messages vis=PRIVATE))
      uid=10257 userId=0
      key=0|com.whatsapp|1|QmFzZTY0IHRhZw==
|10257
      notification=
            extras={
                android.title=String (Sam)
                android.text=String (Are we still on for Saturday?)
            }
      publicNotification=
            extras={
                android.title=String (WhatsApp)
                android.text=String (1 new message)
            }
    NotificationRecord(0x0f00ba12: pkg=android user=UserHandle{0} id=26 tag=null importance=2 key=0|android|26|null|1000: Notification(channel=DEVELOPER_IMPORTANT shortcut=null contentView=null vibrate=null sound=null defaults=0 flags=ONGOING_EVENT|ONLY_ALERT_ONCE|NO_CLEAR color=0xff607d8b vis=PUBLIC))
      notification=
            extras={
                android.title=String (USB debugging connected)
                android.text=String (Tap to turn off USB debugging)
            }
  mMaxPackageEnqueueRate=5.0
  Snoozed notifications:
";

    #[test]
    fn reads_android_16_without_the_lock_screen_copy() {
        let items = parse_notifications(ANDROID_16);
        assert_eq!(items.len(), 3, "{items:?}");
        assert_eq!(items[0].package, "com.example.bank");
        assert_eq!(items[0].title, "Pay school fees from home");
        assert_eq!(items[0].text, "Pay fees for school, college and university in a minute!\n\nOpen the app and choose Education.");
        assert_eq!(items[1].package, "com.whatsapp");
        assert_eq!(items[1].title, "Weekend plans", "the lock screen copy must not replace the real title");
        assert_eq!(items[1].text, "2 new messages");
        // Its tag holds a line break, so the record's first line wraps to the left edge.
        assert_eq!(items[2].title, "Sam");
        assert_eq!(items[2].text, "Are we still on for Saturday?");
    }

    #[test]
    fn keeps_text_whose_closing_bracket_never_comes() {
        let output = "  Notification List:
    NotificationRecord(0x1: pkg=com.example.notes user=UserHandle{0} id=1 tag=null importance=3)
      notification=
            extras={
                android.text=String (first line
second line
                android.title=String (Shopping)
            }
";
        let items = parse_notifications(output);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Shopping");
        assert_eq!(items[0].text, "first line\nsecond line");
    }

    #[test]
    fn returns_nothing_for_unexpected_output() {
        assert!(parse_notifications("").is_empty());
        assert!(parse_notifications("Permission Denial").is_empty());
    }
}
