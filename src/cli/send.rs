use anyhow::Result;

#[cfg(target_os = "macos")]
use anyhow::{bail, Context};
#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
const SEND_SCRIPT: &str = r#"
on run argv
    if (count of argv) < 2 then error "chat and message are required"
    set chatName to item 1 of argv
    set messageText to item 2 of argv
    set previousClipboard to missing value
    try
        set previousClipboard to the clipboard as text
    end try
    try
        tell application id "com.tencent.xinWeChat" to activate
        delay 0.3
        tell application "System Events"
            set wxProc to first application process whose bundle identifier is "com.tencent.xinWeChat"
            set frontmost of wxProc to true
            delay 0.2
            keystroke "f" using command down
            delay 0.1
            keystroke "a" using command down
            delay 0.05
            my pasteText(chatName)
            delay 1.5
            key code 36
            delay 0.8
            my pasteText(messageText)
            delay 0.1
            key code 36
        end tell
        my restoreClipboard(previousClipboard)
    on error errorMessage number errorNumber
        my restoreClipboard(previousClipboard)
        error errorMessage number errorNumber
    end try
end run

on pasteText(textValue)
    tell application "System Events"
        set the clipboard to textValue
        delay 0.05
        keystroke "v" using command down
    end tell
end pasteText

on restoreClipboard(previousClipboard)
    if previousClipboard is not missing value then set the clipboard to previousClipboard
end restoreClipboard
"#;

#[cfg(target_os = "macos")]
pub fn cmd_send(chat: String, message: String) -> Result<()> {
    if chat.trim().is_empty() {
        bail!("聊天对象名称不能为空");
    }
    if message.is_empty() {
        bail!("消息不能为空");
    }

    let output = Command::new("osascript")
        .arg("-e")
        .arg(SEND_SCRIPT)
        .arg(&chat)
        .arg(&message)
        .output()
        .context("无法运行 osascript")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let reason = if stderr.is_empty() {
            format!("osascript exited with status {}", output.status)
        } else {
            stderr
        };
        bail!(
            "发送微信消息失败：{}。请确认微信已登录，并已给当前终端/应用开启“辅助功能”权限",
            reason
        );
    }

    println!("已发送到 {}", chat);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn cmd_send(_chat: String, _message: String) -> Result<()> {
    anyhow::bail!("send 命令目前只支持 macOS");
}
