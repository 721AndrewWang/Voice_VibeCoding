//! 把识别出的文字「上屏」到当前焦点输入框
//!
//! 中文无法靠逐个按键打出来（会被 fcitx5/Rime 当拼音处理），所以走剪贴板：
//! 同时写 CLIPBOARD 与 PRIMARY → 用虚拟键盘按 Shift+Insert（GTK/Qt/Electron/
//! VTE 终端都认，终端里读 PRIMARY 也拿得到）→ 稍后把原来的剪贴板文字放回去。

use std::time::Duration;

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind, SetExtLinux};
use evdev::KeyCode;
use parking_lot::Mutex;

use super::settings::PasteMethod;

/// 剪贴板上下文必须常驻：X11 选区由本进程持有并应答其它程序的读取请求
static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);
/// 恢复剪贴板的「代数」：连续两次上屏时，只让最后一次去恢复
static COMMIT_GEN: Mutex<u64> = Mutex::new(0);

const RESTORE_DELAY: Duration = Duration::from_millis(900);

fn with_clipboard<R>(f: impl FnOnce(&mut Clipboard) -> Result<R, String>) -> Result<R, String> {
    let mut guard = CLIPBOARD.lock();
    if guard.is_none() {
        *guard = Some(Clipboard::new().map_err(|e| format!("无法访问剪贴板: {e}"))?);
    }
    let cb = guard.as_mut().expect("clipboard initialised above");
    f(cb)
}

fn paste_keys(method: PasteMethod) -> Vec<KeyCode> {
    match method {
        PasteMethod::ShiftInsert => vec![KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_INSERT],
        PasteMethod::CtrlV => vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V],
        PasteMethod::CtrlShiftV => {
            vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_V]
        }
    }
}

/// 上屏一段文字（阻塞约 60ms；剪贴板恢复在后台线程）
pub fn commit_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    let settings = super::settings::get();
    let previous = if settings.restore_clipboard {
        with_clipboard(|cb| Ok(cb.get().clipboard(LinuxClipboardKind::Clipboard).text().ok()))?
    } else {
        None
    };

    with_clipboard(|cb| {
        cb.set()
            .clipboard(LinuxClipboardKind::Clipboard)
            .text(text.to_string())
            .map_err(|e| format!("写剪贴板失败: {e}"))?;
        cb.set()
            .clipboard(LinuxClipboardKind::Primary)
            .text(text.to_string())
            .map_err(|e| format!("写 PRIMARY 选区失败: {e}"))
    })?;

    // 给 X 服务器一点时间确认选区归属，再按粘贴键
    std::thread::sleep(Duration::from_millis(30));
    if !super::uinput_kbd::tap_keys(&paste_keys(settings.paste_method), 25) {
        return Err(super::uinput_kbd::last_error()
            .unwrap_or_else(|| "虚拟键盘不可用，无法粘贴".into()));
    }

    let my_gen = {
        let mut g = COMMIT_GEN.lock();
        *g += 1;
        *g
    };
    if let Some(prev) = previous {
        if prev != text {
            std::thread::Builder::new()
                .name("linux-clipboard-restore".into())
                .spawn(move || {
                    std::thread::sleep(RESTORE_DELAY);
                    if *COMMIT_GEN.lock() != my_gen {
                        return;
                    }
                    let _ = with_clipboard(|cb| {
                        cb.set()
                            .clipboard(LinuxClipboardKind::Clipboard)
                            .text(prev)
                            .map_err(|e| e.to_string())
                    });
                })
                .ok();
        }
    }
    Ok(())
}
