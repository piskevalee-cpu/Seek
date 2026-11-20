//! User-facing notifications, using the protocol of oh-my-pi (MIT).
//!
//! Most terminals understand OSC 777 (`notify`), Kitty wants OSC 99, and Windows Terminal can
//! show a native toast through PowerShell. The builders are pure so they can be tested; [`notify`]
//! writes the chosen sequence to stdout, where the terminal picks it up.

use std::io::Write;

/// OSC 777: `ESC ] 777 ; notify ; title ; body BEL`.
pub fn osc777(title: &str, body: &str) -> String {
    format!("\x1b]777;notify;{title};{body}\x07")
}

/// OSC 99: Kitty's two-part notification, terminated by ST (`ESC \`).
pub fn osc99(title: &str, body: &str) -> String {
    format!("\x1b]99;i=1:d=0;{title}\x1b\\\x1b]99;i=1:p=body;{body}\x1b\\")
}

/// The sequence for this terminal. An empty string means "use the Windows toast instead".
pub fn sequence(title: &str, body: &str) -> String {
    if std::env::var_os("WT_SESSION").is_some() {
        String::new()
    } else if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        osc99(title, body)
    } else {
        osc777(title, body)
    }
}

/// A PowerShell toast for Windows Terminal, with quotes removed so it cannot break the script.
pub fn windows_toast_script(title: &str, body: &str) -> String {
    let clean = |value: &str| value.replace(['\'', '"', ';'], " ");
    format!(
        "$ErrorActionPreference='SilentlyContinue'; \
         [void][System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms'); \
         $n=New-Object System.Windows.Forms.NotifyIcon; \
         $n.Icon=[System.Drawing.SystemIcons]::Information; \
         $n.Visible=$true; \
         $n.ShowBalloonTip(5000,'{}','{}',[System.Windows.Forms.ToolTipIcon]::Info);",
        clean(title),
        clean(body)
    )
}

/// Send a notification to the terminal. Best effort: a failure is not worth failing the action.
pub fn notify(title: &str, body: &str) {
    if std::env::var_os("WT_SESSION").is_some() {
        let _ = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &windows_toast_script(title, body)])
            .spawn();
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(sequence(title, body).as_bytes());
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc777_carries_the_title_and_body() {
        assert_eq!(osc777("SEEK", "Copied"), "\x1b]777;notify;SEEK;Copied\x07");
    }

    #[test]
    fn osc99_uses_kitty_two_part_form() {
        let seq = osc99("SEEK", "Copied");
        assert!(seq.contains("99;i=1:d=0;SEEK"));
        assert!(seq.contains("99;i=1:p=body;Copied"));
        assert!(seq.ends_with("\x1b\\"));
    }

    #[test]
    fn windows_toast_is_sanitized() {
        let script = windows_toast_script("S'E;EK", "a\"b");
        assert!(script.contains("'S E EK'"), "{script}");
        assert!(script.contains("'a b'"), "{script}");
    }
}
