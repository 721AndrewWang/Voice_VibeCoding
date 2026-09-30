//! 平台相关 IPC：前端据此隐藏 Windows 专属功能、显示 Linux 的语音输出设置
//!
//! 命令在所有平台都注册（`generate_handler!` 不支持按平台条件注册），
//! 非 Linux 平台上 Linux 专属命令返回错误。

use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// windows | linux | macos
    pub os: String,
    /// Linux 桌面会话：x11 | wayland | tty | unknown
    pub session_type: Option<String>,
    pub app_version: String,
}

#[tauri::command]
pub fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.to_string(),
        session_type: if cfg!(target_os = "linux") {
            Some(std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into()))
        } else {
            None
        },
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::linux;

    pub fn get_linux_voice_settings() -> Result<linux::settings::LinuxVoiceSettings, String> {
        Ok(linux::settings::get())
    }

    pub fn save_linux_voice_settings(
        settings: linux::settings::LinuxVoiceSettings,
    ) -> Result<linux::settings::LinuxVoiceSettings, String> {
        let before = linux::settings::get();
        let saved = linux::settings::save(settings)?;
        let engine_changed = before.asr_model != saved.asr_model
            || before.asr_language != saved.asr_language
            || before.asr_threads != saved.asr_threads
            || before.asr_hotwords != saved.asr_hotwords;
        let mode_changed = before.voice_mode != saved.voice_mode;
        if engine_changed || (mode_changed && saved.voice_mode != linux::settings::VoiceMode::Asr) {
            // 换模型/参数要重建；改用虚拟麦克风则把识别模型占的内存还给系统
            linux::asr::invalidate();
        }
        // 改回本地识别：虚拟麦克风用不到了，从系统声音设置里撤掉
        let remove_mic = mode_changed && saved.voice_mode == linux::settings::VoiceMode::Asr;
        if engine_changed || mode_changed {
            // 立刻准备好对应输出（虚拟麦克风 / 预加载识别模型）；pactl 较慢，不占 IPC 线程
            std::thread::spawn(move || {
                if remove_mic {
                    linux::virtual_mic::remove();
                }
                if let Err(e) = linux::voice_sink::ensure_ready() {
                    log::warn!("LINUX voice output after settings change: {e}");
                }
            });
        }
        Ok(saved)
    }

    pub fn get_asr_status() -> Result<linux::asr::AsrStatus, String> {
        Ok(linux::asr::status())
    }

    pub fn download_asr_model(app: AppHandle, model_id: Option<String>) -> Result<(), String> {
        linux::asr::start_download(app, model_id)
    }

    pub fn get_asr_models() -> Result<Vec<linux::asr::ModelInfo>, String> {
        Ok(linux::asr::models_overview())
    }

    pub fn cancel_asr_model_download() -> Result<(), String> {
        linux::asr::cancel_download();
        Ok(())
    }

    pub fn get_bluetooth_diagnostics() -> Result<linux::bluez::BtDiagnostics, String> {
        Ok(linux::bluez::diagnostics())
    }

    pub fn get_remote_input_status() -> Result<linux::remote_input::RemoteInputStatus, String> {
        Ok(linux::remote_input::status())
    }

    pub fn open_bluetooth_settings() -> Result<(), String> {
        // GNOME：gnome-control-center bluetooth；其它桌面退回 blueman / bluedevil
        let candidates: &[(&str, &[&str])] = &[
            ("gnome-control-center", &["bluetooth"]),
            ("blueman-manager", &[]),
            ("systemsettings", &["kcm_bluetooth"]),
        ];
        for (cmd, args) in candidates {
            if std::process::Command::new(cmd).args(*args).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("没有找到系统蓝牙设置程序".into())
    }

    pub fn open_models_folder() -> Result<(), String> {
        let dir = linux::asr::models_dir().ok_or("模型目录未初始化")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("打开模型目录失败: {e}"))?;
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;
    const ONLY_LINUX: &str = "仅 Linux 版可用";
    pub fn get_linux_voice_settings() -> Result<serde_json::Value, String> {
        Err(ONLY_LINUX.into())
    }
    pub fn save_linux_voice_settings(_s: serde_json::Value) -> Result<serde_json::Value, String> {
        Err(ONLY_LINUX.into())
    }
    pub fn get_asr_status() -> Result<serde_json::Value, String> {
        Err(ONLY_LINUX.into())
    }
    pub fn download_asr_model(_app: AppHandle, _model_id: Option<String>) -> Result<(), String> {
        Err(ONLY_LINUX.into())
    }
    pub fn get_asr_models() -> Result<serde_json::Value, String> {
        Err(ONLY_LINUX.into())
    }
    pub fn cancel_asr_model_download() -> Result<(), String> {
        Err(ONLY_LINUX.into())
    }
    pub fn get_bluetooth_diagnostics() -> Result<serde_json::Value, String> {
        Err(ONLY_LINUX.into())
    }
    pub fn get_remote_input_status() -> Result<serde_json::Value, String> {
        Err(ONLY_LINUX.into())
    }
    pub fn open_bluetooth_settings() -> Result<(), String> {
        Err(ONLY_LINUX.into())
    }
    pub fn open_models_folder() -> Result<(), String> {
        Err(ONLY_LINUX.into())
    }
}

#[cfg(target_os = "linux")]
type VoiceSettings = crate::linux::settings::LinuxVoiceSettings;
#[cfg(not(target_os = "linux"))]
type VoiceSettings = serde_json::Value;
#[cfg(target_os = "linux")]
type AsrStatus = crate::linux::asr::AsrStatus;
#[cfg(not(target_os = "linux"))]
type AsrStatus = serde_json::Value;
#[cfg(target_os = "linux")]
type AsrModels = Vec<crate::linux::asr::ModelInfo>;
#[cfg(not(target_os = "linux"))]
type AsrModels = serde_json::Value;
#[cfg(target_os = "linux")]
type BtDiagnostics = crate::linux::bluez::BtDiagnostics;
#[cfg(not(target_os = "linux"))]
type BtDiagnostics = serde_json::Value;
#[cfg(target_os = "linux")]
type RemoteInputStatus = crate::linux::remote_input::RemoteInputStatus;
#[cfg(not(target_os = "linux"))]
type RemoteInputStatus = serde_json::Value;

#[tauri::command]
pub fn get_linux_voice_settings() -> Result<VoiceSettings, String> {
    imp::get_linux_voice_settings()
}

#[tauri::command]
pub fn save_linux_voice_settings(settings: VoiceSettings) -> Result<VoiceSettings, String> {
    imp::save_linux_voice_settings(settings)
}

#[tauri::command]
pub async fn get_asr_status() -> Result<AsrStatus, String> {
    imp::get_asr_status()
}

#[tauri::command]
pub fn download_asr_model(app: AppHandle, model_id: Option<String>) -> Result<(), String> {
    imp::download_asr_model(app, model_id)
}

#[tauri::command]
pub fn get_asr_models() -> Result<AsrModels, String> {
    imp::get_asr_models()
}

#[tauri::command]
pub fn cancel_asr_model_download() -> Result<(), String> {
    imp::cancel_asr_model_download()
}

#[tauri::command]
pub async fn get_bluetooth_diagnostics() -> Result<BtDiagnostics, String> {
    tokio::task::spawn_blocking(imp::get_bluetooth_diagnostics)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_remote_input_status() -> Result<RemoteInputStatus, String> {
    imp::get_remote_input_status()
}

#[tauri::command]
pub fn open_bluetooth_settings() -> Result<(), String> {
    imp::open_bluetooth_settings()
}

#[tauri::command]
pub fn open_models_folder() -> Result<(), String> {
    imp::open_models_folder()
}
