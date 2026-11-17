// baritone.api.utils.NotificationHelper: a desktop notification, by whatever the operating system offers. Windows gets a balloon
// from the tray icon (a hidden PowerShell shows it, as the Java's AWT tray icon does), macOS goes through osascript and Linux
// through notify-send. The helper runs on its own: nothing here waits for it or reads what it says.
use std::process::{Command, Stdio};

/// `NotificationHelper.notify(text, error)`.
pub fn notify(text: &str, error: bool) {
    let mut command = if cfg!(target_os = "windows") {
        windows(text, error)
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("osascript");
        c.arg("-e").arg(format!("display notification \"{}\" with title \"Baritone\"", text.replace('\\', "\\\\").replace('"', "\\\"")));
        c
    } else {
        let mut c = Command::new("notify-send");
        c.arg("-a").arg("Baritone").arg(text);
        c
    };
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let _ = command.spawn();
}

/// `Util.getPlatform().openUri(uri)`: the address in the default browser.
pub fn open_uri(uri: &str) {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler").arg(uri);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(uri);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(uri);
        c
    };
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let _ = command.spawn();
}

/// The PowerShell that shows one balloon tip and then lets go of the tray icon. The text goes in as a single-quoted string,
/// where only the quote itself needs doubling.
fn windows(text: &str, error: bool) -> Command {
    let kind = if error { "Error" } else { "Info" };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; \
         $n = New-Object System.Windows.Forms.NotifyIcon; $n.Icon = [System.Drawing.SystemIcons]::Information; $n.Visible = $true; \
         $n.ShowBalloonTip(5000, 'Baritone', '{}', [System.Windows.Forms.ToolTipIcon]::{kind}); Start-Sleep -Seconds 6; $n.Dispose()",
        text.replace('\'', "''")
    );
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes up beside the game.
        c.creation_flags(0x0800_0000);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_in_the_text_cannot_end_the_powershell_string() {
        let c = windows("it's done", false);
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let script = args.last().unwrap();
        assert!(script.contains("'it''s done'"), "{script}");
        assert!(script.contains("ToolTipIcon]::Info"));
        let error = windows("x", true);
        assert!(error.get_args().last().unwrap().to_string_lossy().contains("ToolTipIcon]::Error"));
    }
}
