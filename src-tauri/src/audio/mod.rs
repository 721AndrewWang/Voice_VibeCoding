// VB-CABLE 输出路由（cpal）只在 Windows 上存在；Linux 语音走 crate::linux::voice_sink
#[cfg(target_os = "windows")]
pub mod mixer;
pub mod udp_server;
#[cfg(target_os = "windows")]
pub mod pcm_router;
#[cfg(not(target_os = "windows"))]
#[path = "pcm_router_stub.rs"]
pub mod pcm_router;
pub mod vb_cable;
