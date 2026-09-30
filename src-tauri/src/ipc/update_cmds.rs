//! 应用更新相关 IPC

use crate::config::manager::ConfigManager;
use tauri::{AppHandle, State};

/// 检查应用更新（Gitee latest.json 优先）
/// `force` 为 true 时不触发被动弹窗事件（供设置页主动检查，由前端自行 openModal）
#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    force: Option<bool>,
    config_manager: State<'_, ConfigManager>,
) -> Result<crate::app_update::UpdateCheckResult, String> {
    let result = crate::app_update::check_for_update(config_manager.inner());
    if !force.unwrap_or(false) {
        crate::app_update::emit_if_available(&app, &result);
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_app_update_state() -> Result<crate::app_update::UpdateCheckResult, String> {
    Ok(crate::app_update::last_result())
}

#[tauri::command]
pub async fn ignore_app_update(
    version: String,
    config_manager: State<'_, ConfigManager>,
) -> Result<crate::app_update::UpdateCheckResult, String> {
    crate::app_update::ignore_version(config_manager.inner(), &version)
}

#[tauri::command]
pub fn download_app_update(
    app: AppHandle,
    config_manager: State<'_, ConfigManager>,
    url: String,
    version: String,
) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        let _ = (app, config_manager, url, version);
        return Err("Linux 版请在源码目录执行 git pull 后重新运行 linux/install.sh 升级".into());
    }
    crate::app_update::spawn_download(app, config_manager.inner(), url, version)
}
