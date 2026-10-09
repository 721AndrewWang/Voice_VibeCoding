#!/usr/bin/env python3
"""Voice VibeCoding — Windows / Linux 双系统共用蓝牙配对密钥

蓝牙设备对同一台电脑通常只保存一套配对密钥：在一个系统里重新配对，另一个系统就连不上。
本脚本在两边之间同步某个设备的 LE 配对密钥（LTK / EDIV / Rand / IRK）。

  sudo python3 bt-dualboot.py inspect        [设备MAC]   # 只读：对比两边的密钥
  sudo python3 bt-dualboot.py import-windows [设备MAC]   # Windows → Linux（先备份）

脚本位置：装了 .deb 的在 /usr/share/voice-vibecoding/bt-dualboot.py，源码里是 linux/bt-dualboot.py。

不写设备MAC时，自动找 Linux 里已配对、名字含 “MI RC” 的设备（小米蓝牙遥控器 2 Pro）；
找不到或有多个时，请写上 MAC。
Windows 分区只以只读方式挂载，不会改动 Windows；Linux 侧改动前自动备份到
/var/lib/bluetooth/<适配器>/<设备>/info.bak-<时间>。
"""

import configparser
import datetime
import os
import re
import shutil
import subprocess
import sys
import tempfile

DEVICE_NAME_HINT = "MI RC"  # 小米蓝牙遥控器 2 Pro 广播的名字
BT_DIR = "/var/lib/bluetooth"
REPORT = None  # inspect 摘要另存一份给当前用户看


def die(msg: str, code: int = 1):
    print(f"!!! {msg}", file=sys.stderr)
    sys.exit(code)


def run(cmd, check=True, **kw):
    return subprocess.run(cmd, check=check, text=True, capture_output=True, **kw)


def log(msg: str = ""):
    print(msg)
    if REPORT is not None:
        REPORT.append(msg)


# ---------------------------------------------------------------------------
# Windows 分区与注册表
# ---------------------------------------------------------------------------

def ntfs_partitions():
    out = run(["lsblk", "-rno", "PATH,FSTYPE,LABEL"]).stdout
    parts = []
    for line in out.splitlines():
        cols = line.split(" ")
        if len(cols) >= 2 and cols[1] == "ntfs":
            parts.append((cols[0], cols[2] if len(cols) > 2 else ""))
    return parts


def probe_state(dev: str) -> str:
    """ntfs-3g.probe --readwrite 的返回码说明 Windows 是否正常关机"""
    r = subprocess.run(["ntfs-3g.probe", "--readwrite", dev], capture_output=True, text=True)
    return {
        0: "正常关机（可安全读写）",
        14: "Windows 处于休眠 / 快速启动状态（只能只读访问）",
        15: "上次没有正常卸载（只能只读访问）",
    }.get(r.returncode, f"探测返回码 {r.returncode}：{(r.stderr or r.stdout).strip()[:120]}")


class WindowsMount:
    """只读挂载，找到含 Windows\\System32\\config\\SYSTEM 的那个分区"""

    def __init__(self):
        self.dir = None
        self.dev = None
        self.label = ""

    def __enter__(self):
        for dev, label in ntfs_partitions():
            d = tempfile.mkdtemp(prefix="vvc-win-", dir="/run")
            ok = subprocess.run(["mount", "-t", "ntfs-3g", "-o", "ro", dev, d],
                                capture_output=True).returncode == 0
            if not ok:
                ok = subprocess.run(["mount", "-t", "ntfs3", "-o", "ro", dev, d],
                                    capture_output=True).returncode == 0
            if ok and os.path.isfile(self._hive(d)):
                self.dir, self.dev, self.label = d, dev, label
                return self
            if ok:
                subprocess.run(["umount", d], capture_output=True)
            os.rmdir(d)
        die("没有找到含 Windows 注册表的 NTFS 分区（BitLocker 加密的分区无法读取）")

    @staticmethod
    def _hive(root: str) -> str:
        for cand in ("Windows/System32/config/SYSTEM", "WINDOWS/system32/config/SYSTEM"):
            p = os.path.join(root, cand)
            if os.path.isfile(p):
                return p
        return os.path.join(root, "Windows/System32/config/SYSTEM")

    @property
    def hive(self) -> str:
        return self._hive(self.dir)

    def __exit__(self, *exc):
        if self.dir:
            subprocess.run(["umount", self.dir], capture_output=True)
            try:
                os.rmdir(self.dir)
            except OSError:
                pass


def hivexsh(hive: str, script: str) -> str:
    r = subprocess.run(["hivexsh", hive], input=script, capture_output=True, text=True)
    if r.returncode != 0 and not r.stdout:
        raise RuntimeError(r.stderr.strip())
    return r.stdout


def current_control_set(hive: str) -> str:
    out = run(["hivexget", hive, "\\Select", "Current"]).stdout.strip()
    kind, n = parse_reg_value(out)
    if kind == "string":
        n = int(out, 0)
    return f"ControlSet{int(n):03d}"


def parse_reg_value(raw: str):
    """解析 hivexsh lsval 的值：dword:xxxxxxxx / hex(N):aa,bb（N 为十进制类型号）/ "str" """
    raw = raw.strip()
    if raw.startswith("dword:"):
        return "dword", int(raw[6:], 16)
    m = re.match(r"hex\((\d+)\):(.*)", raw, re.S)
    if m:
        kind = int(m.group(1))
        body = re.sub(r"[\s\\]", "", m.group(2))
        data = bytes(int(b, 16) for b in body.split(",") if b)
        if kind == 11:  # REG_QWORD
            return "qword", int.from_bytes(data, "little")
        if kind == 4:  # REG_DWORD
            return "dword", int.from_bytes(data, "little")
        return "binary", data
    if raw.startswith("hex:"):
        body = re.sub(r"[\s\\]", "", raw[4:])
        return "binary", bytes(int(b, 16) for b in body.split(",") if b)
    return "string", raw.strip('"')


def windows_device_names(hive: str, cs: str) -> dict:
    """Parameters\\Devices\\<mac>\\Name（UTF-8，以 0 结尾）"""
    base = f"\\{cs}\\Services\\BTHPORT\\Parameters\\Devices"
    names = {}
    macs = [l.strip() for l in hivexsh(hive, f"cd {base}\nls\n").splitlines()
            if re.fullmatch(r"[0-9a-f]{12}", l.strip())]
    for mac in macs:
        try:
            out = hivexsh(hive, f"cd {base}\\{mac}\nlsval Name\n").strip()
        except RuntimeError:
            out = ""
        kind, v = parse_reg_value(out) if out else ("string", "")
        if kind == "binary":
            v = v.split(b"\0", 1)[0].decode("utf-8", "replace")
        names[mac] = v
    return names


def windows_inventory(hive: str, adapter: str):
    """本适配器下所有配对设备：LE（子键）与经典蓝牙（值）"""
    cs = current_control_set(hive)
    base = f"\\{cs}\\Services\\BTHPORT\\Parameters\\Keys\\{adapter.replace(':', '').lower()}"
    out = hivexsh(hive, f"cd {base}\nls\nlsval\n")
    le, classic = [], []
    for line in out.splitlines():
        line = line.strip()
        if re.fullmatch(r"[0-9a-f]{12}", line):
            le.append(line)
        m = re.match(r'^"([0-9a-f]{12})"=', line)
        if m:
            classic.append(m.group(1))
    names = windows_device_names(hive, cs)
    return le, classic, names


def fmt_mac(m: str) -> str:
    return ":".join(m[i:i + 2] for i in range(0, 12, 2)).upper()


def windows_keys(hive: str, adapter: str, device: str):
    cs = current_control_set(hive)
    base = f"\\{cs}\\Services\\BTHPORT\\Parameters\\Keys"
    adapters = [l.strip() for l in hivexsh(hive, f"cd {base}\nls\n").splitlines() if l.strip()]
    a = adapter.replace(":", "").lower()
    d = device.replace(":", "").lower()
    devices = []
    values = None
    if a in adapters:
        listing = hivexsh(hive, f"cd {base}\\{a}\nls\nlsval\n")
        for line in listing.splitlines():
            line = line.strip()
            if re.fullmatch(r"[0-9a-f]{12}", line):
                devices.append(line)
        if d in devices:
            out = hivexsh(hive, f"cd {base}\\{a}\\{d}\nlsval\n")
            values = {}
            for m in re.finditer(r'^"([^"]+)"=(.*?)(?=^"|\Z)', out, re.M | re.S):
                values[m.group(1)] = parse_reg_value(m.group(2))
    return cs, adapters, devices, values


# ---------------------------------------------------------------------------
# BlueZ
# ---------------------------------------------------------------------------

def bluez_info_path(adapter: str, device: str) -> str:
    return os.path.join(BT_DIR, adapter.upper(), device.upper(), "info")


def read_bluez(adapter: str, device: str):
    path = bluez_info_path(adapter, device)
    if not os.path.isfile(path):
        return path, None
    cp = configparser.ConfigParser(interpolation=None, strict=False)
    cp.optionxform = str
    cp.read(path, encoding="utf-8")
    return path, cp


def hexkey(b: bytes) -> str:
    return b.hex().upper()


def describe_windows(values) -> list:
    rows = []
    for k, (kind, v) in sorted(values.items()):
        if kind == "binary":
            rows.append(f"{k}: {len(v)} 字节")
        elif kind in ("dword", "qword"):
            shown = v if k in ("KeyLength", "AddressType", "AuthReq") else ("0" if v == 0 else "非 0")
            rows.append(f"{k}: {kind} = {shown}")
        else:
            rows.append(f"{k}: {kind}")
    return rows


def compare(label: str, win: bytes, lin_hex: str):
    if not win or not lin_hex:
        return f"{label}: 缺失（Windows {'有' if win else '无'} / Linux {'有' if lin_hex else '无'}）"
    lin = bytes.fromhex(lin_hex)
    if win == lin:
        return f"{label}: 两边相同（字节顺序一致）"
    if win == lin[::-1]:
        return f"{label}: 两边相同（字节顺序相反）"
    return f"{label}: 不同"


# ---------------------------------------------------------------------------
# 子命令
# ---------------------------------------------------------------------------

def adapter_mac() -> str:
    out = run(["bluetoothctl", "show"], check=False).stdout
    m = re.search(r"Controller ([0-9A-F:]{17})", out)
    if not m:
        entries = [e for e in os.listdir(BT_DIR) if re.fullmatch(r"[0-9A-F:]{17}", e)]
        if len(entries) == 1:
            return entries[0]
        die("无法确定本机蓝牙适配器地址")
    return m.group(1)


def cmd_inspect(device: str):
    adapter = adapter_mac()
    log(f"本机蓝牙适配器：{adapter}    目标设备：{device}")
    with WindowsMount() as win:
        log(f"Windows 分区：{win.dev}（{win.label or '无卷标'}）  状态：{probe_state(win.dev)}")
        cs, adapters, devices, values = windows_keys(win.hive, adapter, device)
        try:
            le, classic, names = windows_inventory(win.hive, adapter)
        except Exception as e:  # noqa: BLE001
            le, classic, names = [], [], {}
            log(f"（读取设备清单失败：{e}）")
    log(f"Windows 注册表：{cs}\\Services\\BTHPORT\\Parameters\\Keys")
    for mac in le:
        log(f"  LE 设备     {fmt_mac(mac)}  {names.get(mac, '') or '（无名称）'}")
    for mac in classic:
        log(f"  经典蓝牙设备 {fmt_mac(mac)}  {names.get(mac, '') or '（无名称）'}")
    others = sorted(set(names) - set(le) - set(classic))
    if others:
        log(f"  另有 {len(others)} 个设备只有名称记录（已删除配对或未完成配对）：")
        for mac in others:
            log(f"    {fmt_mac(mac)}  {names.get(mac) or '（无名称）'}")
    log(f"  适配器条目：{', '.join(adapters) or '（无）'}")
    log(f"  本适配器下的设备：{len(devices)} 个{'（含目标设备）' if values is not None else '（不含目标设备）'}")
    if values is not None:
        for row in describe_windows(values):
            log(f"    {row}")

    path, cp = read_bluez(adapter, device)
    log(f"Linux（BlueZ）：{path}  {'存在' if cp else '不存在'}")
    if cp:
        for sec in cp.sections():
            keys = ", ".join(k for k in cp[sec].keys() if k != "Key")
            has_key = "Key" in cp[sec]
            log(f"    [{sec}]{'（含 Key）' if has_key else ''} {keys}")

    if values is not None and cp:
        w_ltk = values.get("LTK", (None, b""))[1]
        w_irk = values.get("IRK", (None, b""))[1]
        l_sec = linux_ltk_section(cp)
        l_ltk = cp.get(l_sec, "Key", fallback="") if l_sec else ""
        l_irk = cp.get("IdentityResolvingKey", "Key", fallback="")
        log("对比：")
        log("  " + compare("LTK（链路加密密钥）", w_ltk, l_ltk))
        log("  " + compare("IRK（遥控器身份密钥）", w_irk, l_irk))
        w_sc = values.get("EDIV", ("dword", 0))[1] == 0 and values.get("ERand", ("qword", 0))[1] == 0
        l_sc = bool(l_sec) and cp.get(l_sec, "EDiv", fallback="0") == "0" and cp.get(l_sec, "Rand", fallback="0") == "0"
        log(f"  配对方式：Windows {'LE Secure Connections' if w_sc else '传统配对'} / "
            f"Linux {'LE Secure Connections' if l_sc else '传统配对'}")


LTK_SECTIONS_SC = ("PeripheralLongTermKey", "SlaveLongTermKey")


def linux_ltk_section(cp: configparser.ConfigParser):
    """BlueZ 存 LTK 的小节：传统配对在 [LongTermKey]，LE Secure Connections 在 [PeripheralLongTermKey]"""
    for sec in ("LongTermKey",) + LTK_SECTIONS_SC:
        if cp.has_option(sec, "Key"):
            return sec
    return None


def apply_windows_keys(cp: configparser.ConfigParser, values: dict) -> list:
    """把 Windows 注册表里的 LE 密钥写进 BlueZ 的 info（原地修改 cp），返回说明。

    - LE Secure Connections（EDIV=0 且 ERand=0）：BlueZ 把唯一的 LTK 存在
      [PeripheralLongTermKey]（旧名 [SlaveLongTermKey]），EDiv/Rand 为 0。
    - 传统配对：遥控器分发的 LTK 连同 EDIV/Rand 存在 [LongTermKey]。
    - LTK 两边字节顺序相同，直接拷贝；ERand 是小端 64 位整数，BlueZ 写十进制。
    - Authenticated 沿用 Linux 这边同一设备配对时的值（配对方式相同）。
    - IRK 是遥控器自己的身份密钥，与配对无关，保持 Linux 里已有的值。
    """
    notes = []
    ltk = values.get("LTK", (None, b""))[1]
    if not isinstance(ltk, (bytes, bytearray)) or len(ltk) != 16:
        raise ValueError("Windows 记录里没有 16 字节的 LTK（不是 LE 设备？）")
    ediv = int(values.get("EDIV", ("dword", 0))[1])
    rand = int(values.get("ERand", ("qword", 0))[1])
    enc = int(values.get("KeyLength", ("dword", 16))[1] or 16)
    sc = ediv == 0 and rand == 0

    def existing_auth(default: str) -> str:
        for sec in LTK_SECTIONS_SC + ("LongTermKey",):
            if cp.has_option(sec, "Authenticated"):
                return cp.get(sec, "Authenticated")
        return default

    if sc:
        auth = existing_auth("2")
        for sec in LTK_SECTIONS_SC:
            if sec not in cp:
                cp.add_section(sec)
            cp[sec]["Key"] = hexkey(ltk)
            cp[sec]["Authenticated"] = auth
            cp[sec]["EncSize"] = str(enc)
            cp[sec]["EDiv"] = "0"
            cp[sec]["Rand"] = "0"
        if cp.has_section("LongTermKey"):
            cp.remove_section("LongTermKey")
            notes.append("删除了旧配对遗留的 [LongTermKey]")
        notes.append(f"LE Secure Connections：LTK 写入 {' / '.join(LTK_SECTIONS_SC)}（Authenticated={auth}）")
    else:
        auth = existing_auth("0")
        if "LongTermKey" not in cp:
            cp.add_section("LongTermKey")
        lt = cp["LongTermKey"]
        lt["Key"] = hexkey(ltk)
        lt["Authenticated"] = auth
        lt["EncSize"] = str(enc)
        lt["EDiv"] = str(ediv)
        lt["Rand"] = str(rand)
        notes.append("传统配对：LTK/EDiv/Rand 写入 [LongTermKey]")

    w_irk = values.get("IRK", (None, b""))[1]
    l_irk = cp.get("IdentityResolvingKey", "Key", fallback="")
    if isinstance(w_irk, (bytes, bytearray)) and len(w_irk) == 16:
        notes.append(compare("IRK", bytes(w_irk), l_irk) + "（保持 Linux 现有值）")
    return notes


def build_minimal_info(values: dict, name: str) -> configparser.ConfigParser:
    """Linux 这边的配对记录不存在（从没配过，或配对失效后被 BlueZ 删掉）时，按 Windows 记录新建"""
    cp = configparser.ConfigParser(interpolation=None, strict=False)
    cp.optionxform = str
    cp.add_section("General")
    g = cp["General"]
    g["Name"] = name or "MI RC"
    g["AddressType"] = "static" if values.get("AddressType", ("dword", 0))[1] else "public"
    g["SupportedTechnologies"] = "LE;"
    g["Trusted"] = "true"
    g["Blocked"] = "false"
    g["WakeAllowed"] = "true"
    return cp


def cmd_import_windows(device: str):
    adapter = adapter_mac()
    with WindowsMount() as win:
        cs, _, _, values = windows_keys(win.hive, adapter, device)
        win_name = ""
        if values is not None:
            try:
                win_name = windows_device_names(win.hive, cs).get(device.replace(":", "").lower(), "")
            except Exception:  # noqa: BLE001
                pass
    if values is None:
        die(f"Windows 注册表里没有 {device} 的配对记录：请先在 Windows 里配对这个设备，再回到 Linux 运行本命令")
    path, cp = read_bluez(adapter, device)
    rebuilt = cp is None
    if rebuilt:
        cp = build_minimal_info(values, win_name.strip())

    try:
        notes = apply_windows_keys(cp, values)
    except ValueError as e:
        die(str(e))
    if rebuilt:
        notes.insert(0, "Linux 里没有这个设备的配对记录，已按 Windows 记录新建")

    backup = None
    if not rebuilt:
        stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        backup = f"{path}.bak-{stamp}"
        shutil.copy2(path, backup)
    os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)

    run(["systemctl", "stop", "bluetooth"])
    try:
        tmp = path + ".vvc-tmp"
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            cp.write(f, space_around_delimiters=False)
        os.replace(tmp, path)
    finally:
        run(["systemctl", "start", "bluetooth"])
    print(f">>> 已把 Windows（{cs}）里 {device} 的配对密钥导入 BlueZ")
    for n in notes:
        print(f"    - {n}")
    print(">>> 按一下遥控器任意键唤醒，应该能直接连上（Windows 那边也保持可用）。")
    if backup:
        print(f">>> 原文件备份：{backup}；若要还原：")
        print(f"    sudo systemctl stop bluetooth && sudo cp {backup} {path} && sudo systemctl start bluetooth")


def detect_device() -> str:
    """在 BlueZ 的配对记录里找名字含 DEVICE_NAME_HINT 的设备，只有一个时返回它的 MAC"""
    found = set()
    for adapter in sorted(os.listdir(BT_DIR)) if os.path.isdir(BT_DIR) else []:
        adir = os.path.join(BT_DIR, adapter)
        for mac in os.listdir(adir) if os.path.isdir(adir) else []:
            info = os.path.join(adir, mac, "info")
            if not re.fullmatch(r"[0-9A-F]{2}(:[0-9A-F]{2}){5}", mac) or not os.path.isfile(info):
                continue
            cp = configparser.ConfigParser(interpolation=None)
            cp.optionxform = str
            try:
                cp.read(info, encoding="utf-8")
            except configparser.Error:
                continue
            if DEVICE_NAME_HINT.lower() in cp.get("General", "Name", fallback="").lower():
                found.add(mac)
    if len(found) == 1:
        return found.pop()
    if not found:
        die(f"Linux 里没找到名字含 “{DEVICE_NAME_HINT}” 的已配对设备，请在命令后面写上遥控器的 MAC\n"
            "（Linux 里配对过：bluetoothctl devices；只在 Windows 里配对过：设备管理器 → 蓝牙 → 遥控器 → 属性 → 详细信息 → 蓝牙设备地址）")
    die(f"找到多个名字含 “{DEVICE_NAME_HINT}” 的设备（{', '.join(sorted(found))}），请在命令后面写上要用的 MAC")


def main():
    global REPORT
    if len(sys.argv) < 2 or sys.argv[1] not in ("inspect", "import-windows"):
        print(__doc__)
        sys.exit(2)
    if os.geteuid() != 0:
        os.execvp("sudo", ["sudo", sys.executable] + sys.argv)
    if not (shutil.which("hivexsh") and shutil.which("hivexget")):
        die("需要 hivex 工具读取 Windows 注册表，请先安装：sudo apt install libhivex-bin")
    device = (sys.argv[2] if len(sys.argv) > 2 else detect_device()).upper()
    if not re.fullmatch(r"[0-9A-F]{2}(:[0-9A-F]{2}){5}", device):
        die(f"设备地址格式不对：{device}")
    if sys.argv[1] == "inspect":
        REPORT = []
        cmd_inspect(device)
        user = os.environ.get("SUDO_USER")
        if user:
            home = os.path.expanduser(f"~{user}")
            out = os.path.join(home, ".cache", "voice-vibecoding", "bt-dualboot-inspect.txt")
            os.makedirs(os.path.dirname(out), exist_ok=True)
            with open(out, "w", encoding="utf-8") as f:
                f.write("\n".join(REPORT) + "\n")
            shutil.chown(out, user, user)
            shutil.chown(os.path.dirname(out), user, user)
            print(f"\n（摘要已保存到 {out}，不含密钥原文）")
    else:
        cmd_import_windows(device)


if __name__ == "__main__":
    main()
