//! 非 Windows 平台没有 VB-CABLE / audio_router 子进程。
//! Linux 的语音输出在进程内完成（`crate::linux::voice_sink`），这里保留同名接口，
//! 让共用代码（重启桥接、冲突检测、状态栏）无需分支。

pub const DEFAULT_PCM_PORT: u16 = 31680;

pub fn run_audio_router_cli(_args: &[String]) -> i32 {
    eprintln!("xiaomi-audio-router 仅用于 Windows（VB-CABLE）");
    2
}

pub fn spawn_audio_router_process() -> Result<(), String> {
    Ok(())
}

pub fn stop_audio_router_process() {}

pub fn audio_router_child_pid() -> Option<u32> {
    None
}

/// 进程内语音路由，恒为可用
pub fn audio_router_process_alive() -> bool {
    true
}

pub fn audio_router_ready() -> bool {
    true
}
