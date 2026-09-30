// 汉王 V60 HID 驱动目前只有 Windows（hidapi）实现，且尚未接入界面
#[cfg(target_os = "windows")]
pub mod hid_driver;
pub mod config;
