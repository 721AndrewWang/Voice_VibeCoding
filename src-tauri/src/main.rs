// Windows GUI：始终无控制台窗口（含子进程 CLI 入口）。调试日志写文件；需要控制台时设 RUST_LOG 并自行 AllocConsole。
#![windows_subsystem = "windows"]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("xiaomi-hid-injector") => {
            let code =
                remote_bridge_hub_lib::bridges::xiaomi::hid_tap_injector::run_injector_cli(&args);
            std::process::exit(code);
        }
        Some("xiaomi-audio-router") => {
            let code = remote_bridge_hub_lib::audio::pcm_router::run_audio_router_cli(&args);
            std::process::exit(code);
        }
        // Linux 联调：伪造一只小米遥控器发键（无实物时测试按键链路）
        #[cfg(target_os = "linux")]
        Some("simulate-remote") => {
            let code = remote_bridge_hub_lib::linux::simulate::run_simulate_remote_cli(&args);
            std::process::exit(code);
        }
        // Linux 排障：列出 BlueZ 看到的蓝牙设备（是否已配对 / 有无 ATVV 服务）
        #[cfg(target_os = "linux")]
        Some("diag-bluetooth") => {
            let diag = remote_bridge_hub_lib::linux::bluez::diagnostics();
            println!(
                "{}",
                serde_json::to_string_pretty(&diag).unwrap_or_else(|e| e.to_string())
            );
            std::process::exit(0);
        }
        _ => remote_bridge_hub_lib::run(),
    }
}
