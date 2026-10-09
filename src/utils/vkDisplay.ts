import { lang, t } from "../i18n";

/** Linux 桌面把 Win 键叫 Super（GNOME/KDE 设置里都这么写） */
function winKeyLabel(side: "左" | "右"): string {
  const linux =
    typeof document !== "undefined" && document.documentElement.dataset.platform === "linux";
  const sideLabel = side === "左" ? t("左", "L") : t("右", "R");
  return `${sideLabel} ${linux ? "Super" : "Win"}`;
}

/** VK → 显示名（与 Rust `vk_to_label` / Python NAMED_KEYS 对齐） */
export function vkDisplayName(vk: number): string {
  const map: Record<number, string> = {
    0x08: "Backspace",
    0x09: "Tab",
    0x0d: "Enter",
    0x13: "Pause",
    0x14: "CapsLock",
    0x1b: "Esc",
    0x20: t("空格 Space", "Space"),
    0x21: "PageUp",
    0x22: "PageDown",
    0x23: "End",
    0x24: "Home",
    0x25: "←",
    0x26: "↑",
    0x27: "→",
    0x28: "↓",
    0x2c: "PrtSc",
    0x2d: "Insert",
    0x2e: "Delete",
    0x5d: "Menu",
    0x90: "NumLock",
    0x91: "ScrLk",
    0x6a: "Num*",
    0x6b: "Num+",
    0x6d: "Num-",
    0x6e: "Num.",
    0x6f: "Num/",
    0x10: t("左 Shift", "L Shift"),
    0xa0: t("左 Shift", "L Shift"),
    0xa1: t("右 Shift", "R Shift"),
    0x11: t("左 Ctrl", "L Ctrl"),
    0xa2: t("左 Ctrl", "L Ctrl"),
    0xa3: t("右 Ctrl", "R Ctrl"),
    0x12: t("左 Alt", "L Alt"),
    0xa4: t("左 Alt", "L Alt"),
    0xa5: t("右 Alt", "R Alt"),
    0x5b: winKeyLabel("左"),
    0x5c: winKeyLabel("右"),
    0xad: t("静音", "Mute"),
    0xae: t("音量-", "Vol-"),
    0xaf: t("音量+", "Vol+"),
    0xb0: t("下一曲", "Next"),
    0xb1: t("上一曲", "Prev"),
    0xb2: t("停止", "Stop"),
    0xb3: t("播放/暂停", "Play/Pause"),
    0xb7: t("计算器", "Calculator"),
    0xba: ";",
    0xbb: "=",
    0xbc: ",",
    0xbd: "-",
    0xbe: ".",
    0xbf: "/",
    0xc0: "`",
    0xdb: "[",
    0xdc: "\\",
    0xdd: "]",
    0xde: "'",
  };
  if (map[vk]) return map[vk];
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk);
  if (vk >= 0x30 && vk <= 0x39) return String(vk - 0x30);
  if (vk >= 0x60 && vk <= 0x69) return `Num${vk - 0x60}`;
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
  return `VK_0x${vk.toString(16).toUpperCase()}`;
}

/** 录入 UI 常驻媒体/系统键兜底 */
export const MEDIA_PICK_KEYS: { readonly vk: number; readonly label: string }[] = [
  { vk: 0xaf, get label() { return t("音量+", "Vol+"); } },
  { vk: 0xae, get label() { return t("音量-", "Vol-"); } },
  { vk: 0xad, get label() { return t("静音", "Mute"); } },
  { vk: 0xb7, get label() { return t("计算器", "Calculator"); } },
];

/** 中文键名 → 英文（后端 / 配置里存的是中文；只在显示时翻译） */
const KEY_LABEL_EN: Record<string, string> = {
  "空格 Space": "Space",
  空格: "Space",
  回车: "Enter",
  静音: "Mute",
  "音量-": "Vol-",
  "音量+": "Vol+",
  音量: "Volume",
  下一曲: "Next",
  上一曲: "Prev",
  停止: "Stop",
  "播放/暂停": "Play/Pause",
  计算器: "Calculator",
  电源: "Power",
  上: "Up",
  下: "Down",
  左: "Left",
  右: "Right",
  确定: "OK",
  返回: "Back",
  主页: "Home",
  菜单: "Menu",
  语音: "Voice",
  删除: "Delete",
  鼠标: "Mouse",
};

/**
 * 后端录键事件里的键名（如「左 Shift」「音量+」）按当前界面语言显示。
 * 中文模式原样返回。
 */
export function keyLabelDisplay(label: string): string {
  if (!label || lang.value === "zh") return label;
  if (KEY_LABEL_EN[label]) return KEY_LABEL_EN[label];
  const m = /^([左右])\s*(.+)$/.exec(label);
  if (m && /^[\x20-\x7e]+$/.test(m[2])) return `${m[1] === "左" ? "L" : "R"} ${m[2]}`;
  return label;
}

/** 遥控器按键名（配置 button_aliases 存中文别名）：只在显示时翻译 */
export function remoteButtonLabel(alias: string): string {
  return keyLabelDisplay(alias);
}
