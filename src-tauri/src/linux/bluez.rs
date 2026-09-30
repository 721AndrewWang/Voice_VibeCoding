//! BlueZ（D-Bus）后端：发现已配对的小米遥控器 2 Pro，订阅 ATVV 语音通道
//!
//! 与 Windows 版的分工：
//! - 按键不走 GATT：BlueZ 的 HOG 插件已经把遥控器变成内核输入设备，由
//!   `linux::remote_input` 读取。
//! - 这里只负责 ATVV（Android TV Voice over BLE）：CONTROL/AUDIO 通知、
//!   GET_CAPS / MIC_OPEN 写入，以及电量。
//! - 协议处理复用 Windows 版 `input_session` 里的状态机（按下/松开/解码）。
//!
//! 遥控器闲置会休眠断开；按任意键后 BlueZ 自动回连（已配对的 HID 设备），
//! 本模块等到 Connected=true 再重新订阅。

use std::str::FromStr;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use bluer::gatt::remote::{Characteristic, CharacteristicWriteRequest};
use bluer::gatt::WriteOp;
use bluer::{Address, Device, DeviceEvent, DeviceProperty, Uuid};
use futures::StreamExt;
use parking_lot::Mutex;
use tauri::{AppHandle, Manager};

use crate::bridges::xiaomi::connect::{
    mark_atvv_subscribed, normalize_bluetooth_address, reset_atvv_subscribed, XiaomiConnection,
    XiaomiRuntime,
};
use crate::bridges::xiaomi::input_session::{self, AtvvVoiceState};
use crate::bridges::xiaomi::key_log::{emit_message, KeyEmitGate};
use crate::bridges::{BridgeState, BridgeType};

pub const ATVV_SERVICE: Uuid = Uuid::from_u128(0xab5e0001_5a21_4f05_bc7d_af01f617b664);
pub const ATVV_TX: Uuid = Uuid::from_u128(0xab5e0002_5a21_4f05_bc7d_af01f617b664);
pub const ATVV_AUDIO: Uuid = Uuid::from_u128(0xab5e0003_5a21_4f05_bc7d_af01f617b664);
pub const ATVV_CONTROL: Uuid = Uuid::from_u128(0xab5e0004_5a21_4f05_bc7d_af01f617b664);

/// ATVV v1.0 GET_CAPS：版本 1.0、编码 0x0003（8k/16k ADPCM）、交互模式 0x03
pub const GET_CAPS_V10: [u8; 6] = [0x0A, 0x01, 0x00, 0x00, 0x03, 0x03];
pub const MIC_OPEN: [u8; 2] = [0x0C, 0x00];

const XIAOMI_NAMES: &[&str] = &["mi rc", "xiaomi bluetooth remote 2 pro"];
/// 等遥控器被按键唤醒并回连的最长时间（超时后由外层重连循环再来）
const WAIT_CONNECTED: Duration = Duration::from_secs(20);
const BATTERY_POLL: Duration = Duration::from_secs(60);

static RUNTIME: OnceLock<Arc<XiaomiRuntime>> = OnceLock::new();

/// 绑定桥接运行时，使「等待遥控器回连」能被「断开/重启桥接」及时打断
pub fn bind_runtime(runtime: Arc<XiaomiRuntime>) {
    let _ = RUNTIME.set(runtime);
}

fn stop_requested() -> bool {
    RUNTIME.get().map(|r| r.should_stop()).unwrap_or(false)
}

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("linux-bluez")
            .enable_all()
            .build()
            .expect("tokio runtime for BlueZ")
    })
}

/// 在 BlueZ 专用运行时上执行并阻塞等待结果。
/// 调用方可能身处 Tauri 的 tokio 运行时（async 命令）里，那里不能再 `block_on`，
/// 所以改为 spawn 到专用运行时、用 std 通道等结果。
fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        let (tx, rx) = std_mpsc::channel();
        rt().spawn(async move {
            let _ = tx.send(fut.await);
        });
        rx.recv().expect("BlueZ task dropped")
    } else {
        rt().block_on(fut)
    }
}

async fn session() -> Result<bluer::Session, String> {
    static SESSION: tokio::sync::OnceCell<bluer::Session> = tokio::sync::OnceCell::const_new();
    SESSION
        .get_or_try_init(|| async {
            bluer::Session::new()
                .await
                .map_err(|e| format!("无法连接 BlueZ（bluetooth 服务是否在运行？）: {e}"))
        })
        .await
        .cloned()
}

async fn adapter() -> Result<bluer::Adapter, String> {
    let s = session().await?;
    let adapter = s
        .default_adapter()
        .await
        .map_err(|e| format!("没有找到蓝牙适配器: {e}"))?;
    if !adapter.is_powered().await.unwrap_or(false) {
        return Err("蓝牙已关闭：请在系统「设置 → 蓝牙」里打开".into());
    }
    Ok(adapter)
}

#[derive(Debug, Clone)]
struct Candidate {
    address: Address,
    name: String,
    connected: bool,
    hardware_match: bool,
}

fn name_matches(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    XIAOMI_NAMES.iter().any(|x| n == *x) || n.starts_with("mi rc")
}

async fn candidates() -> Result<Vec<Candidate>, String> {
    let adapter = adapter().await?;
    let addrs = adapter
        .device_addresses()
        .await
        .map_err(|e| format!("枚举蓝牙设备失败: {e}"))?;
    let mut out = Vec::new();
    for addr in addrs {
        let Ok(dev) = adapter.device(addr) else { continue };
        if !dev.is_paired().await.unwrap_or(false) {
            continue;
        }
        let name = device_name(&dev).await;
        let has_atvv = dev
            .uuids()
            .await
            .ok()
            .flatten()
            .map(|u| u.contains(&ATVV_SERVICE))
            .unwrap_or(false);
        let hardware_match = dev
            .modalias()
            .await
            .ok()
            .flatten()
            .map(|m| m.vendor == 0x2717 && m.product == 0x32B8)
            .unwrap_or(false);
        if has_atvv || hardware_match || name_matches(&name) {
            out.push(Candidate {
                address: addr,
                name: if name.is_empty() { "MI RC".into() } else { name },
                connected: dev.is_connected().await.unwrap_or(false),
                hardware_match,
            });
        }
    }
    out.sort_by_key(|c| (!c.connected, !c.hardware_match, c.address.to_string()));
    Ok(out)
}

/// BlueZ 的 Name 属性偶尔缺失（名字未解析）时退回 Alias；
/// Alias 若只是地址本身（C0-5D-…）则视为无名，由调用方显示默认名
async fn device_name(dev: &Device) -> String {
    if let Some(n) = dev.name().await.ok().flatten().filter(|n| !n.trim().is_empty()) {
        return n;
    }
    let alias = dev.alias().await.unwrap_or_default();
    if alias.replace('-', ":").eq_ignore_ascii_case(&dev.address().to_string()) {
        String::new()
    } else {
        alias
    }
}

fn choose(list: &[Candidate], configured: Option<&str>) -> Result<Candidate, String> {
    if let Some(cfg) = configured.and_then(|a| normalize_bluetooth_address(a).ok()) {
        if let Some(c) = list.iter().find(|c| c.address.to_string() == cfg) {
            return Ok(c.clone());
        }
    }
    match list {
        [] => Err(
            "未找到已配对的小米遥控器 2 Pro：请先在系统「设置 → 蓝牙」里配对「MI RC」\
             （同时按住遥控器「主页」+「菜单」键约 3 秒进入配对模式）"
                .into(),
        ),
        [one] => Ok(one.clone()),
        many => {
            // 多个候选：优先已连接的；否则取硬件匹配的第一个
            Ok(many
                .iter()
                .find(|c| c.connected)
                .or_else(|| many.iter().find(|c| c.hardware_match))
                .unwrap_or(&many[0])
                .clone())
        }
    }
}

async fn wait_connected(dev: &Device, timeout: Duration) -> bool {
    if dev.is_connected().await.unwrap_or(false) {
        return true;
    }
    let Ok(events) = dev.events().await else {
        return false;
    };
    futures::pin_mut!(events);
    // 主动发起一次连接（遥控器若正在广播可立刻连上；否则等它被按键唤醒）
    let dev2 = dev.clone();
    tokio::spawn(async move {
        if let Err(e) = dev2.connect().await {
            log::debug!("LINUX BLE connect attempt: {e}");
        }
    });
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            ev = events.next() => match ev {
                Some(DeviceEvent::PropertyChanged(DeviceProperty::Connected(true))) => return true,
                Some(_) => {}
                None => return dev.is_connected().await.unwrap_or(false),
            },
            _ = tick.tick() => {
                if stop_requested() {
                    return false;
                }
            },
            _ = &mut deadline => return dev.is_connected().await.unwrap_or(false),
        }
    }
}

/// 发现 + 等待连接（阻塞，在重连线程里调用）
pub fn discover_and_connect(configured: Option<&str>) -> Result<XiaomiConnection, String> {
    let configured = configured.map(str::to_string);
    block_on(async move {
        let configured = configured.as_deref();
        let mut list = candidates().await?;
        if list.is_empty() {
            // 还没配对：每秒静默查一次（配对完成后 1 秒内就能连上），最多 15 秒再报错
            for _ in 0..15 {
                if stop_requested() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                list = candidates().await?;
                if !list.is_empty() {
                    break;
                }
            }
        }
        for c in &list {
            log::info!(
                "LINUX BLE candidate name={} address={} connected={} hw={}",
                c.name,
                c.address,
                c.connected,
                c.hardware_match
            );
        }
        let chosen = choose(&list, configured)?;
        let adapter = adapter().await?;
        let dev = adapter
            .device(chosen.address)
            .map_err(|e| format!("打开蓝牙设备失败: {e}"))?;
        if !wait_connected(&dev, WAIT_CONNECTED).await {
            return Err("遥控器未连接：按一下遥控器任意键唤醒它".into());
        }
        let address = chosen.address.to_string();
        let address_u64 = u64::from_str_radix(&address.replace(':', ""), 16).unwrap_or(0);
        Ok(XiaomiConnection {
            name: chosen.name,
            address,
            address_u64,
            atvv_interface_id: format!("{}/dev_{}", adapter.name(), chosen.address.to_string().replace(':', "_")),
        })
    })
}

enum Packet {
    Control(Vec<u8>),
    Audio(Vec<u8>),
    Shutdown,
}

struct AtvvChars {
    tx: Characteristic,
    audio: Option<Characteristic>,
    control: Characteristic,
    tx_op: WriteOp,
}

async fn find_atvv(dev: &Device) -> Result<AtvvChars, String> {
    let services = dev
        .services()
        .await
        .map_err(|e| format!("读取 GATT 服务失败: {e}"))?;
    for svc in services {
        if svc.uuid().await.ok() != Some(ATVV_SERVICE) {
            continue;
        }
        let mut tx = None;
        let mut audio = None;
        let mut control = None;
        for ch in svc
            .characteristics()
            .await
            .map_err(|e| format!("读取 ATVV 特征失败: {e}"))?
        {
            match ch.uuid().await.ok() {
                Some(u) if u == ATVV_TX => tx = Some(ch),
                Some(u) if u == ATVV_AUDIO => audio = Some(ch),
                Some(u) if u == ATVV_CONTROL => control = Some(ch),
                _ => {}
            }
        }
        let (Some(tx), Some(control)) = (tx, control) else {
            return Err("ATVV 服务缺少 TX/CONTROL 特征".into());
        };
        let tx_op = match tx.flags().await {
            Ok(f) if f.write_without_response => WriteOp::Command,
            _ => WriteOp::Request,
        };
        return Ok(AtvvChars {
            tx,
            audio,
            control,
            tx_op,
        });
    }
    Err("遥控器上没有找到 ATVV 语音服务".into())
}

async fn write_tx(chars: &AtvvChars, bytes: &[u8], label: &str) {
    let req = CharacteristicWriteRequest {
        op_type: chars.tx_op,
        ..Default::default()
    };
    match chars.tx.write_ext(bytes, &req).await {
        Ok(()) => log::info!("LINUX ATVV {label} sent"),
        Err(e) => log::warn!("LINUX ATVV {label} write failed: {e}"),
    }
}

fn update_battery(app: &AppHandle, pct: u8, conn: &XiaomiConnection) {
    if let Some(state) = app.try_state::<BridgeState>() {
        state.update_device_info(
            BridgeType::Xiaomi,
            Some(conn.name.clone()),
            Some(conn.address.clone()),
            Some(pct),
        );
    }
    emit_message(app, &format!("电量 {pct}%"));
}

/// ATVV CONTROL 通知（与 Windows `handle_atvv_control` 同语义，TX 写入改为回调）
fn handle_control(
    app: &AppHandle,
    gate: &KeyEmitGate,
    state: &Arc<StdMutex<AtvvVoiceState>>,
    payload: &[u8],
    write: &dyn Fn(&[u8], &'static str),
) {
    use crate::bridges::xiaomi::key_mapping;
    let Some(op) = payload.first() else { return };
    match *op {
        0x08 => {
            key_mapping::mark_direct_signal("voice");
            key_mapping::mark_direct_signal("mic");
            write(&MIC_OPEN, "MIC_OPEN");
            log::info!("LINUX ATVV MIC_OPEN request opcode=0x08");
        }
        0x04 => input_session::on_voice_remote_press(app, gate, state),
        0x00 => input_session::on_voice_remote_release(app, gate, state),
        0x0A if payload.len() >= 7 => {
            let predictor = i16::from_be_bytes([payload[4], payload[5]]) as i32;
            let step = payload[6] as i32;
            if let Ok(mut st) = state.lock() {
                st.pending.clear();
                st.pending_sync = Some((predictor, step));
            }
            log::debug!("LINUX ATVV AUDIO_SYNC predictor={predictor} step={step}");
        }
        0x0B if payload.len() >= 7 => {
            let version = u16::from_be_bytes([payload[1], payload[2]]);
            let frame_size = u16::from_be_bytes([payload[5], payload[6]]) as usize;
            if let Ok(mut st) = state.lock() {
                if frame_size > 0 {
                    st.frame_size = frame_size;
                }
            }
            log::info!("LINUX ATVV CAPS version=0x{version:04X} frame_size={frame_size} raw={payload:02X?}");
        }
        0x0C => log::warn!("LINUX ATVV MIC_OPEN_ERROR raw={payload:02X?}"),
        other => log::debug!("LINUX ATVV opcode=0x{other:02X} raw={payload:02X?}"),
    }
}

/// 保持会话直到断开或用户点「断开」（阻塞，在重连线程里调用）
pub fn monitor_connection(
    conn: &XiaomiConnection,
    runtime: Arc<XiaomiRuntime>,
    app: AppHandle,
) -> Result<(), String> {
    block_on(run_session(conn.clone(), runtime, app))
}

async fn run_session(
    conn: XiaomiConnection,
    runtime: Arc<XiaomiRuntime>,
    app: AppHandle,
) -> Result<(), String> {
    let adapter = adapter().await?;
    let addr = Address::from_str(&conn.address).map_err(|e| format!("地址无效: {e}"))?;
    let dev = adapter
        .device(addr)
        .map_err(|e| format!("打开蓝牙设备失败: {e}"))?;
    let events = dev
        .events()
        .await
        .map_err(|e| format!("监听设备状态失败: {e}"))?;
    futures::pin_mut!(events);

    crate::bridges::xiaomi::tv_gate::mark_connecting();
    reset_atvv_subscribed();

    // ATVV 服务：GATT 刚解析完时偶尔拿不到，重试几次
    let mut chars = None;
    let mut last_err = String::new();
    for attempt in 1..=8 {
        if runtime.should_stop() || !dev.is_connected().await.unwrap_or(false) {
            break;
        }
        match find_atvv(&dev).await {
            Ok(c) => {
                chars = Some(c);
                break;
            }
            Err(e) => {
                log::warn!("LINUX ATVV discover attempt {attempt}: {e}");
                last_err = e;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    // 电量（BlueZ battery 插件 → org.bluez.Battery1）
    if let Ok(Some(pct)) = dev.battery_percentage().await {
        update_battery(&app, pct, &conn);
    }

    let delay = app
        .try_state::<crate::config::manager::ConfigManager>()
        .and_then(|m| m.get_device_config("xiaomi").ok())
        .map(|c| Duration::from_secs_f32(c.tv_action_ready_delay.clamp(0.0, 10.0)))
        .unwrap_or(Duration::from_secs(2));

    let Some(chars) = chars else {
        crate::bridges::xiaomi::tv_gate::mark_ready(delay);
        emit_message(&app, &format!("ATVV 语音通道不可用: {last_err}"));
        // 按键仍可用（走 evdev）；保持连接监控，断开后外层重连时再试 ATVV
        return watch_until_disconnect(&dev, &mut events, &runtime, &app, &conn).await;
    };

    // 刚回连时链路加密可能还没完成，StartNotify 会报 ATT 0x0e：会话内快速重试，
    // 不退回外层重连循环（那边要等 retry_delay，按键唤醒后语音会晚好几秒才可用）
    let mut control_stream = None;
    let mut notify_err = String::new();
    for attempt in 1..=10 {
        match chars.control.notify().await {
            Ok(s) => {
                control_stream = Some(s);
                break;
            }
            Err(e) => {
                notify_err = e.to_string();
                log::info!("LINUX ATVV CONTROL notify attempt {attempt}: {e}");
                if runtime.should_stop() || !dev.is_connected().await.unwrap_or(false) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }
    let control_stream =
        control_stream.ok_or_else(|| format!("订阅 ATVV CONTROL 失败: {notify_err}"))?;
    futures::pin_mut!(control_stream);
    let mut audio_stream = None;
    if let Some(a) = &chars.audio {
        for attempt in 1..=5 {
            match a.notify().await {
                Ok(s) => {
                    audio_stream = Some(s);
                    break;
                }
                Err(e) => {
                    log::warn!("LINUX ATVV audio notify attempt {attempt}: {e}");
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        }
        if audio_stream.is_none() {
            // 有按键没声音比断开重来更糟：交给外层重连循环重新订阅
            return Err("订阅 ATVV 音频通道失败".into());
        }
    }
    let audio_live = audio_stream.is_some();
    let audio_stream = futures::stream::iter(audio_stream).flatten();
    futures::pin_mut!(audio_stream);

    // 协议处理放到独立线程，按到达顺序串行执行（按下/松开里有同步注入与短暂 sleep）
    let (pkt_tx, pkt_rx) = std_mpsc::channel::<Packet>();
    let (write_tx_chan, mut write_rx) = tokio::sync::mpsc::unbounded_channel::<(Vec<u8>, &'static str)>();
    let worker_app = app.clone();
    let worker = std::thread::Builder::new()
        .name("linux-atvv-worker".into())
        .spawn(move || {
            let gate = KeyEmitGate::new(60);
            let state = Arc::new(StdMutex::new(AtvvVoiceState::new()));
            let write = |bytes: &[u8], label: &'static str| {
                let _ = write_tx_chan.send((bytes.to_vec(), label));
            };
            for pkt in pkt_rx {
                match pkt {
                    Packet::Control(p) => handle_control(&worker_app, &gate, &state, &p, &write),
                    Packet::Audio(p) => input_session::handle_atvv_audio(&state, &p),
                    Packet::Shutdown => break,
                }
            }
            // 断线时若语音键仍按着：丢弃半句话，但一定抬起快捷键
            crate::bridges::xiaomi::voice_pcm::stop();
            input_session::on_voice_remote_release(&worker_app, &gate, &state);
        })
        .map_err(|e| format!("启动 ATVV 线程失败: {e}"))?;

    write_tx(&chars, &GET_CAPS_V10, "GET_CAPS").await;
    mark_atvv_subscribed(true);
    crate::bridges::xiaomi::tv_gate::mark_ready(delay);
    let _ = crate::bridges::xiaomi::voice_pcm::ensure_started();
    emit_message(&app, "ATVV 语音通道已订阅");
    log::info!("LINUX ATVV subscribed address={}", conn.address);

    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let mut last_battery = Instant::now();
    let reason: Result<(), String> = loop {
        tokio::select! {
            v = control_stream.next() => match v {
                Some(p) => { let _ = pkt_tx.send(Packet::Control(p)); }
                None => break Err("ATVV CONTROL 通知结束（遥控器断开）".into()),
            },
            v = audio_stream.next(), if audio_live => match v {
                Some(p) => { let _ = pkt_tx.send(Packet::Audio(p)); }
                None => break Err("ATVV 音频通知结束（遥控器断开）".into()),
            },
            w = write_rx.recv() => {
                if let Some((bytes, label)) = w { write_tx(&chars, &bytes, label).await; }
            },
            ev = events.next() => match ev {
                Some(DeviceEvent::PropertyChanged(DeviceProperty::Connected(false))) => {
                    break Err("遥控器已断开（休眠或超出范围）".into());
                }
                Some(DeviceEvent::PropertyChanged(DeviceProperty::BatteryPercentage(p))) => {
                    update_battery(&app, p, &conn);
                }
                Some(_) => {}
                None => break Err("BlueZ 设备事件流结束".into()),
            },
            _ = tick.tick() => {
                if runtime.should_stop() {
                    break Ok(());
                }
                if last_battery.elapsed() >= BATTERY_POLL {
                    last_battery = Instant::now();
                    if let Ok(Some(pct)) = dev.battery_percentage().await {
                        update_battery(&app, pct, &conn);
                    }
                }
            },
        }
    };

    mark_atvv_subscribed(false);
    crate::bridges::xiaomi::tv_gate::reset();
    let _ = pkt_tx.send(Packet::Shutdown);
    let _ = tokio::task::spawn_blocking(move || worker.join()).await;
    if let Err(e) = &reason {
        log::info!("LINUX ATVV session ended: {e}");
    }
    reason
}

async fn watch_until_disconnect(
    dev: &Device,
    events: &mut (impl futures::Stream<Item = DeviceEvent> + Unpin),
    runtime: &XiaomiRuntime,
    app: &AppHandle,
    conn: &XiaomiConnection,
) -> Result<(), String> {
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            ev = events.next() => match ev {
                Some(DeviceEvent::PropertyChanged(DeviceProperty::Connected(false))) => {
                    return Err("遥控器已断开".into());
                }
                Some(DeviceEvent::PropertyChanged(DeviceProperty::BatteryPercentage(p))) => {
                    update_battery(app, p, conn);
                }
                Some(_) => {}
                None => return Err("BlueZ 设备事件流结束".into()),
            },
            _ = tick.tick() => {
                if runtime.should_stop() {
                    return Ok(());
                }
                if !dev.is_connected().await.unwrap_or(false) {
                    return Err("遥控器已断开".into());
                }
            }
        }
    }
}

/// ATVV 没连上时按了语音键：限流提示（对齐 Windows 的「修复 ATVV」提醒）
pub fn notify_atvv_unavailable(app: &AppHandle) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock();
        if let Some(t) = *last {
            if t.elapsed() < Duration::from_secs(30) {
                return;
            }
        }
        *last = Some(Instant::now());
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("遥控器语音通道（ATVV）未连接")
        .body("语音键暂时只能触发快捷键，收不到声音。请打开 Voice VibeCoding，点「修复 ATVV 连接」。")
        .show();
}

// ---------------------------------------------------------------------------
// 诊断：列出 BlueZ 已知设备（界面「蓝牙诊断」与命令行 --diag-bluetooth 共用）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BtDeviceInfo {
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub has_atvv: bool,
    pub modalias: Option<String>,
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BtDiagnostics {
    pub adapter: Option<String>,
    pub powered: bool,
    pub devices: Vec<BtDeviceInfo>,
    pub error: Option<String>,
}

pub fn diagnostics() -> BtDiagnostics {
    block_on(async {
        let s = match session().await {
            Ok(s) => s,
            Err(e) => {
                return BtDiagnostics { adapter: None, powered: false, devices: vec![], error: Some(e) }
            }
        };
        let adapter = match s.default_adapter().await {
            Ok(a) => a,
            Err(e) => {
                return BtDiagnostics {
                    adapter: None,
                    powered: false,
                    devices: vec![],
                    error: Some(format!("没有蓝牙适配器: {e}")),
                }
            }
        };
        let powered = adapter.is_powered().await.unwrap_or(false);
        let mut devices = Vec::new();
        for addr in adapter.device_addresses().await.unwrap_or_default() {
            let Ok(dev) = adapter.device(addr) else { continue };
            devices.push(BtDeviceInfo {
                address: addr.to_string(),
                name: device_name(&dev).await,
                paired: dev.is_paired().await.unwrap_or(false),
                connected: dev.is_connected().await.unwrap_or(false),
                trusted: dev.is_trusted().await.unwrap_or(false),
                has_atvv: dev
                    .uuids()
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.contains(&ATVV_SERVICE))
                    .unwrap_or(false),
                modalias: dev.modalias().await.ok().flatten().map(|m| {
                    format!("{}:v{:04X}p{:04X}d{:04X}", m.source, m.vendor, m.product, m.device)
                }),
                battery: dev.battery_percentage().await.ok().flatten(),
            });
        }
        BtDiagnostics {
            adapter: Some(adapter.name().to_string()),
            powered,
            devices,
            error: None,
        }
    })
}

/// 本机是否有已配对的小米遥控器（主机状态栏用，结果缓存 5 秒）
pub fn paired_remote_known() -> Option<bool> {
    static CACHE: Mutex<Option<(Instant, Option<bool>)>> = Mutex::new(None);
    if let Some((t, v)) = *CACHE.lock() {
        if t.elapsed() < Duration::from_secs(5) {
            return v;
        }
    }
    let v = block_on(async { candidates().await.ok().map(|l| !l.is_empty()) });
    *CACHE.lock() = Some((Instant::now(), v));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(addr: &str, connected: bool, hw: bool) -> Candidate {
        Candidate {
            address: Address::from_str(addr).unwrap(),
            name: "MI RC".into(),
            connected,
            hardware_match: hw,
        }
    }

    #[test]
    fn choose_prefers_configured_then_connected() {
        let list = vec![cand("AA:BB:CC:DD:EE:01", false, true), cand("AA:BB:CC:DD:EE:02", true, true)];
        assert_eq!(
            choose(&list, Some("aa:bb:cc:dd:ee:01")).unwrap().address.to_string(),
            "AA:BB:CC:DD:EE:01"
        );
        assert_eq!(choose(&list, None).unwrap().address.to_string(), "AA:BB:CC:DD:EE:02");
        assert!(choose(&[], None).is_err());
    }

    #[test]
    fn names() {
        assert!(name_matches("MI RC"));
        assert!(name_matches("Xiaomi Bluetooth Remote 2 Pro"));
        assert!(!name_matches("WBT22"));
    }
}
