import { describe, expect, it } from "vitest";
import { EXACT, RULES, translateBackend as tb } from "./backend";

describe("translateBackend", () => {
  it("translates host status labels and values exactly", () => {
    expect(tb("语音识别")).toBe("Speech");
    expect(tb("待加载")).toBe("Not loaded");
    expect(tb("无权限")).toBe("No permission");
    expect(tb("ATVV 未连接")).toBe("ATVV not connected");
  });

  it("translates battery lines", () => {
    expect(tb("电量 100%")).toBe("Battery 100%");
    expect(tb("电量 7%")).toBe("Battery 7%");
    expect(tb("电量读取失败: timeout")).toBe("Battery read failed: timeout");
  });

  it("translates nested Chinese inside a wrapper sentence", () => {
    expect(tb("ATVV 语音通道不可用: 遥控器上没有找到 ATVV 语音服务")).toBe(
      "ATVV voice channel unavailable: No ATVV voice service found on the remote",
    );
    expect(tb("ATVV 语音通道不可用: 订阅 ATVV CONTROL 失败: le-connection-abort")).toBe(
      "ATVV voice channel unavailable: Failed to subscribe to ATVV CONTROL: le-connection-abort",
    );
  });

  it("translates connect errors with retry suffix", () => {
    expect(tb("蓝牙已关闭：请在系统「设置 → 蓝牙」里打开（将自动重试）")).toBe(
      "Bluetooth is off: turn it on in system Settings → Bluetooth (retrying automatically)",
    );
    expect(tb("Error|遥控器已断开")).toBe("Error: Remote disconnected");
  });

  it("translates the uinput detail built from two parts", () => {
    expect(
      tb("无法打开 /dev/uinput。请在终端运行 linux/setup-system.sh（安装 udev 规则后需重新登录或重新插拔）。"),
    ).toBe(
      "Cannot open /dev/uinput. Run linux/setup-system.sh in a terminal (log out and back in, or replug, after installing the udev rules).",
    );
  });

  it("translates path-bearing errors", () => {
    expect(tb("无权读取遥控器按键 /dev/input/event5：请先运行 linux/setup-system.sh 安装 udev 规则")).toBe(
      "No permission to read remote keys /dev/input/event5: run linux/setup-system.sh first to install the udev rules",
    );
    expect(tb("保存 model.onnx 失败: disk full")).toBe("Failed to save model.onnx: disk full");
    expect(tb("下载失败: HTTP 404 Not Found")).toBe("Download failed: HTTP 404 Not Found");
  });

  it("translates generic X失败 errors", () => {
    expect(tb("创建模型目录失败: Permission denied")).toBe("Failed to create model folder: Permission denied");
    expect(tb("hf-mirror.com: 已取消下载")).toBe("hf-mirror.com: Download cancelled");
  });

  it("translates device names inside sentences", () => {
    expect(tb("T1 遥控器 连接逻辑尚未接入")).toBe("T1 remote connection is not implemented yet");
  });

  it("passes unknown and empty strings through unchanged", () => {
    expect(tb("something else")).toBe("something else");
    expect(tb("完全没见过的中文")).toBe("完全没见过的中文");
    expect(tb("")).toBe("");
  });

  it("has no English values that still contain Chinese", () => {
    for (const v of Object.values(EXACT)) expect(v).not.toMatch(/[一-鿿]/);
    expect(RULES.length).toBeGreaterThan(50);
  });
});
