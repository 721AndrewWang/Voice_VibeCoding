//! 无实物联调工具
//!
//! 1. `remote-bridge-hub simulate-remote up down ok ...`
//!    用 uinput 伪造一只「MI RC」（蓝牙总线、VID 2717 / PID 32B8，带 MSC_SCAN），
//!    按顺序发键 —— 走的是与真遥控器完全相同的 evdev 独占 → 映射 → 注入链路。
//! 2. `remote-bridge-hub --simulate-voice some.wav`（发给已在运行的实例）
//!    把 16 kHz 单声道 WAV 编码成 ATVV 的 IMA-ADPCM 帧，按实时节奏喂给语音状态机：
//!    AUDIO_START → 音频帧 → AUDIO_STOP，经过解码/增益/识别/上屏整条链路。

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, InputEvent, InputId, KeyCode, KeyEvent, MiscCode, MiscEvent};
use tauri::AppHandle;

use crate::bridges::xiaomi::input_session::{self, AtvvVoiceState};
use crate::bridges::xiaomi::key_log::KeyEmitGate;

const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX_TABLE: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// IMA-ADPCM 编码（与 `adpcm_decoder` 完全对称：高半字节在前）
pub fn ima_encode(samples: &[i16]) -> Vec<u8> {
    let mut pred: i32 = 0;
    let mut idx: i32 = 0;
    let mut nibble = |s: i16| -> u8 {
        let step = STEP_TABLE[idx as usize];
        let mut diff = s as i32 - pred;
        let mut code = 0u8;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut vpdiff = step >> 3;
        if diff >= step {
            code |= 4;
            diff -= step;
            vpdiff += step;
        }
        if diff >= step >> 1 {
            code |= 2;
            diff -= step >> 1;
            vpdiff += step >> 1;
        }
        if diff >= step >> 2 {
            code |= 1;
            vpdiff += step >> 2;
        }
        pred = if code & 8 != 0 { pred - vpdiff } else { pred + vpdiff }.clamp(-32768, 32767);
        idx = (idx + INDEX_TABLE[(code & 7) as usize]).clamp(0, 88);
        code
    };
    samples
        .chunks(2)
        .map(|pair| {
            let hi = nibble(pair[0]);
            let lo = nibble(*pair.get(1).unwrap_or(&0));
            (hi << 4) | lo
        })
        .collect()
}

/// 读 16 kHz 单声道 16-bit PCM WAV（测试音频用，只支持这一种格式）
pub fn read_wav_16k_mono(path: &str) -> Result<Vec<i16>, String> {
    let data = std::fs::read(path).map_err(|e| format!("读取 {path} 失败: {e}"))?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("不是 WAV 文件".into());
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= data.len() {
            let f = &data[body..body + 16];
            fmt = Some((
                u16::from_le_bytes([f[0], f[1]]),
                u16::from_le_bytes([f[2], f[3]]),
                u32::from_le_bytes([f[4], f[5], f[6], f[7]]),
                u16::from_le_bytes([f[14], f[15]]),
            ));
        } else if id == b"data" {
            let (format, channels, rate, bits) = fmt.ok_or("WAV 缺少 fmt 块")?;
            if format != 1 || channels != 1 || rate != 16_000 || bits != 16 {
                return Err(format!(
                    "只支持 16kHz/单声道/16bit PCM（当前 format={format} ch={channels} rate={rate} bits={bits}）"
                ));
            }
            let end = (body + size).min(data.len());
            return Ok(data[body..end]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect());
        }
        pos = body + size + (size & 1);
    }
    Err("WAV 缺少 data 块".into())
}

/// 模拟一次「按住语音键说完这段 WAV 再松开」
pub fn simulate_voice(app: AppHandle, wav_path: String) {
    std::thread::Builder::new()
        .name("linux-simulate-voice".into())
        .spawn(move || {
            let samples = match read_wav_16k_mono(&wav_path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("SIMULATE voice: {e}");
                    return;
                }
            };
            log::info!(
                "SIMULATE voice: {} ({} ms)",
                wav_path,
                samples.len() as u64 * 1000 / 16_000
            );
            let gate = KeyEmitGate::new(60);
            let state = Arc::new(StdMutex::new(AtvvVoiceState::new()));
            let adpcm = ima_encode(&samples);
            input_session::on_voice_remote_press(&app, &gate, &state);
            // 与真机一致：120 字节一帧（240 个采样 = 15ms），拆成 20 字节的 BLE 通知
            for frame in adpcm.chunks(120) {
                for piece in frame.chunks(20) {
                    input_session::handle_atvv_audio(&state, piece);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            input_session::on_voice_remote_release(&app, &gate, &state);
            log::info!("SIMULATE voice: done");
        })
        .ok();
}

fn usage_for(name: &str) -> Option<(u32, KeyCode)> {
    let (usage, key) = match name {
        "mic" | "voice" => (0x3E, KeyCode::KEY_F5),
        "power" => (0x66, KeyCode::KEY_POWER),
        "up" => (0x52, KeyCode::KEY_UP),
        "down" => (0x51, KeyCode::KEY_DOWN),
        "left" => (0x50, KeyCode::KEY_LEFT),
        "right" => (0x4F, KeyCode::KEY_RIGHT),
        "ok" => (0x28, KeyCode::KEY_ENTER),
        "back" => (0xF1, KeyCode::KEY_BACK),
        "home" => (0x4A, KeyCode::KEY_HOME),
        "menu" => (0x65, KeyCode::KEY_COMPOSE),
        "tv" => (0x35, KeyCode::KEY_GRAVE),
        "volume_up" => (0x80, KeyCode::KEY_VOLUMEUP),
        "volume_down" => (0x81, KeyCode::KEY_VOLUMEDOWN),
        _ => return None,
    };
    Some((0x0007_0000 | usage, key))
}

/// CLI：`simulate-remote [--hold-ms N] key[:hold_ms] ...`
pub fn run_simulate_remote_cli(args: &[String]) -> i32 {
    let mut default_hold = 80u64;
    let mut seq: Vec<(String, u64)> = Vec::new();
    let mut it = args.iter().skip(2);
    while let Some(a) = it.next() {
        if a == "--hold-ms" {
            default_hold = it.next().and_then(|v| v.parse().ok()).unwrap_or(80);
            continue;
        }
        let (name, hold) = match a.split_once(':') {
            Some((n, h)) => (n.to_string(), h.parse().unwrap_or(default_hold)),
            None => (a.clone(), default_hold),
        };
        seq.push((name, hold));
    }
    if seq.is_empty() {
        eprintln!("用法: remote-bridge-hub simulate-remote up down ok back:600 ...");
        return 2;
    }

    let mut keys = AttributeSet::<KeyCode>::new();
    for n in [
        "mic", "power", "up", "down", "left", "right", "ok", "back", "home", "menu", "tv",
        "volume_up", "volume_down",
    ] {
        keys.insert(usage_for(n).unwrap().1);
    }
    let mut misc = AttributeSet::<MiscCode>::new();
    misc.insert(MiscCode::MSC_SCAN);
    let dev = VirtualDevice::builder()
        .and_then(|b| {
            b.name("MI RC (simulated)")
                .input_id(InputId::new(BusType::BUS_BLUETOOTH, 0x2717, 0x32B8, 0x00A4))
                .with_keys(&keys)
        })
        .and_then(|b| b.with_msc(&misc))
        .and_then(|b| b.build());
    let mut dev = match dev {
        Ok(d) => d,
        Err(e) => {
            eprintln!("创建模拟遥控器失败: {e}（需要 /dev/uinput 权限，先运行 linux/setup-system.sh）");
            return 1;
        }
    };
    // 等主程序扫描到并 grab（扫描周期 700ms）
    std::thread::sleep(Duration::from_millis(1600));
    for (name, hold) in seq {
        let Some((scan, key)) = usage_for(&name) else {
            eprintln!("未知按键: {name}");
            continue;
        };
        let press = |value: i32| -> Vec<InputEvent> {
            vec![*MiscEvent::new(MiscCode::MSC_SCAN, scan as i32), *KeyEvent::new(key, value)]
        };
        println!("press {name} ({hold}ms)");
        let _ = dev.emit(&press(1));
        std::thread::sleep(Duration::from_millis(hold));
        let _ = dev.emit(&press(0));
        std::thread::sleep(Duration::from_millis(350));
    }
    std::thread::sleep(Duration::from_millis(300));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridges::xiaomi::adpcm_decoder::AdpcmDecoder;

    #[test]
    fn encoder_round_trips_through_app_decoder() {
        let samples: Vec<i16> = (0..3200)
            .map(|i| ((i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 16000.0).sin() * 8000.0) as i16)
            .collect();
        let enc = ima_encode(&samples);
        assert_eq!(enc.len(), samples.len() / 2);
        let mut dec = AdpcmDecoder::new_ima();
        let out = dec.decode_bytes(&enc);
        // IMA 有量化误差；跳过起步收敛段后误差应很小
        let err: f64 = samples[400..]
            .iter()
            .zip(&out[400..])
            .map(|(a, b)| ((*a as f64) - (*b as f64)).abs())
            .sum::<f64>()
            / (samples.len() - 400) as f64;
        assert!(err < 300.0, "mean abs error {err}");
    }
}
