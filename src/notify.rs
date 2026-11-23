//! Desktop notifications when a long run of the hooks ends (a pre-push of a few minutes),
//! so that you can do something else meanwhile. Best effort: nothing happens when the
//! system has no way to show them.
//!
//! `HOOKER_NOTIFY=0` turns them off, `HOOKER_NOTIFY=30` changes the minimum duration (seconds).

use std::{process::Command, time::Duration};

/// Default minimum duration of a run to be notified
const MIN_DURATION: Duration = Duration::from_secs(10);

/// Minimum duration from `HOOKER_NOTIFY` (`None`: turned off)
pub fn threshold() -> Option<Duration> {
    match std::env::var("HOOKER_NOTIFY")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        None | Some("") | Some("1") | Some("true") => Some(MIN_DURATION),
        Some("0") | Some("false") | Some("off") => None,
        Some(seconds) => seconds
            .parse::<f64>()
            .ok()
            .filter(|s| *s > 0.)
            .map(Duration::from_secs_f64),
    }
}

/// Shows a notification (without waiting for it)
pub fn send(title: &str, body: &str) {
    let mut command = if cfg!(target_os = "macos") {
        let script = format!(
            "display notification {} with title {}",
            apple_string(body),
            apple_string(title)
        );
        let mut command = Command::new("osascript");
        command.args(["-e", &script]);
        command
    } else if cfg!(windows) {
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $text = $xml.GetElementsByTagName('text'); \
             $text.Item(0).AppendChild($xml.CreateTextNode({})) > $null; \
             $text.Item(1).AppendChild($xml.CreateTextNode({})) > $null; \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Goat Hooker').Show([Windows.UI.Notifications.ToastNotification]::new($xml))",
            powershell_string(title),
            powershell_string(body)
        );
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-Command", &script]);
        command
    } else {
        let mut command = Command::new("notify-send");
        command.args(["--app-name=Goat Hooker", title, body]);
        command
    };
    let _ = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn apple_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn powershell_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        assert_eq!(apple_string(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(powershell_string("it's"), "'it''s'");
    }
}
