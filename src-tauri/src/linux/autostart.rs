//! 开机自启（XDG autostart：~/.config/autostart/*.desktop）
//!
//! 对应 Windows 的 HKCU\...\Run 项；同样带 `--minimized` 参数，仅用于单实例去重。

use std::path::PathBuf;

const FILE_NAME: &str = "com.remote-bridge-hub.app.desktop";

fn autostart_path() -> Option<PathBuf> {
    Some(autostart_dir()?.join(FILE_NAME))
}

#[cfg(not(test))]
fn autostart_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("autostart"))
}

/// 单元测试一律指向临时目录：上游的 reconcile 测试会调用「关闭自启」，不能删掉真实的自启项
#[cfg(test)]
fn autostart_dir() -> Option<PathBuf> {
    Some(std::env::temp_dir().join(format!("voice-vibecoding-test-autostart-{}", std::process::id())))
}

/// AppImage 运行时 current_exe 是临时挂载点，要用 $APPIMAGE
fn exec_path() -> Option<String> {
    if let Some(p) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(p).display().to_string());
    }
    std::env::current_exe().ok().map(|p| p.display().to_string())
}

fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Voice VibeCoding\n\
         Comment=小米遥控器语音/按键桥接\n\
         Exec=\"{exe}\" --minimized\n\
         Icon=voice-vibecoding\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n\
         X-GNOME-Autostart-Delay=5\n"
    )
}

pub fn is_enabled() -> bool {
    autostart_path().map(|p| p.is_file()).unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let path = autostart_path().ok_or("无法确定 ~/.config/autostart 目录")?;
    if enabled {
        let exe = exec_path().ok_or("无法确定程序路径")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建 autostart 目录失败: {e}"))?;
        }
        std::fs::write(&path, desktop_entry(&exe)).map_err(|e| format!("写入自启项失败: {e}"))?;
        log::info!("LINUX autostart enabled: {}", path.display());
    } else if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("删除自启项失败: {e}"))?;
        log::info!("LINUX autostart disabled");
    }
    Ok(())
}

/// 启动时对齐：设置开着就刷新 Exec 路径（程序被移动/升级后仍能自启）
pub fn reconcile(settings_autostart: Option<bool>) {
    match settings_autostart {
        Some(true) => {
            if let Err(e) = set_enabled(true) {
                log::warn!("LINUX autostart reconcile: {e}");
            }
        }
        Some(false) => {
            let _ = set_enabled(false);
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_never_touch_the_real_autostart_dir() {
        let p = autostart_path().unwrap();
        assert!(p.starts_with(std::env::temp_dir()), "{}", p.display());
    }

    #[test]
    fn entry_quotes_exec_and_passes_minimized() {
        let e = desktop_entry("/opt/Voice VibeCoding/app");
        assert!(e.contains("Exec=\"/opt/Voice VibeCoding/app\" --minimized\n"));
        assert!(e.starts_with("[Desktop Entry]\n"));
    }
}
