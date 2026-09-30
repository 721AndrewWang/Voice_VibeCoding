import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface PlatformInfo {
  /** windows | linux | macos */
  os: string;
  /** Linux 桌面会话：x11 | wayland | … */
  sessionType: string | null;
  appVersion: string;
}

/** 启动时在 main.ts 里先加载，页面渲染时已确定平台（避免 Windows 专属按钮闪一下） */
export const platform = reactive<PlatformInfo>({
  os: "windows",
  sessionType: null,
  appVersion: "",
});

export const isLinux = computed(() => platform.os === "linux");
export const isWindows = computed(() => platform.os === "windows");

export async function initPlatform(): Promise<void> {
  try {
    const info = await invoke<PlatformInfo>("get_platform_info");
    Object.assign(platform, info);
  } catch {
    // 旧后端没有该命令：保持 Windows 默认
  }
  document.documentElement.dataset.platform = platform.os;
}
