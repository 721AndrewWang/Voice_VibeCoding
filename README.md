# Voice VibeCoding for Linux

[![CI](https://github.com/721AndrewWang/Voice_VibeCoding/actions/workflows/ci.yml/badge.svg?branch=linux-port)](https://github.com/721AndrewWang/Voice_VibeCoding/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/721AndrewWang/Voice_VibeCoding?include_prereleases)](https://github.com/721AndrewWang/Voice_VibeCoding/releases)

**English** | [中文 (original project README)](README.zh-CN.md) | [中文 Linux 文档](docs/LINUX.md)

Turn a Xiaomi Bluetooth Remote 2 Pro into a push-to-talk controller for coding on Linux. Hold the voice button and talk; when you let go, your speech is transcribed **offline on your machine** and pasted at the cursor. Mixed Chinese and English works, technical terms included. Every other button can be mapped to any keyboard shortcut.

This is the **Linux port** of [mwlt/Voice_VibeCoding](https://github.com/mwlt/Voice_VibeCoding) (Rust + Tauri 2 + Vue 3, MIT), which ships for Windows. The UI and the key-mapping config (`xiaomi.json`) are the same as the Windows version; the platform layer underneath was rewritten for Linux. Credit for the original app goes to [mwlt](https://github.com/mwlt) and its contributors. See [Credits](#credits).

> Tested on Ubuntu 24.04 (GNOME, X11 session) on x86-64.

## How it works on Linux

| Feature | Windows (upstream) | Linux (this port) |
|---|---|---|
| Bluetooth / ATVV voice channel | WinRT BLE | BlueZ over D-Bus |
| Reading remote buttons | HID Tap (Frida into WUDFHost) + low-level keyboard hook | evdev, exclusive grab of the remote's input nodes (`EVIOCGRAB`) |
| Injecting shortcuts | WinUHid virtual keyboard / SendInput | uinput virtual keyboard |
| Where voice goes | VB-CABLE virtual sound card → IME dictation | **Offline speech recognition, pasted at the cursor** (default), or a PipeWire virtual microphone |
| Recording shortcuts | Low-level keyboard hook | X11 keyboard grab (`XGrabKeyboard`) |
| Start at login | Registry Run key | `~/.config/autostart` |

Why offline recognition by default: Linux has no IME with a hold-to-talk dictation button like the ones the Windows version relies on, so the port runs speech recognition locally ([sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), CPU only) and pastes the text when you release the button. You can switch back to the Windows-style "virtual microphone + shortcut" mode.

### Speech models

| Model | Best for | Latency (5 s of speech) | RAM | Download |
|---|---|---|---|---|
| **Qwen3-ASR 0.6B** (default) | Mixed Chinese/English, dev jargon (Claude Code, cargo build, PR, tokio…) | ~1 s | ~1.9 GB | ~990 MB |
| SenseVoice Small | Chinese only, instant output, low memory | ~0.1 s | ~330 MB | ~240 MB |

On 16 dictated programming sentences mixing Chinese and English (with simulated remote-microphone audio quality), Qwen3-ASR had a 5.7% character error rate and got 86% of English technical terms right; SenseVoice had 14.3% and 64%.

Other details:
- **Idle memory release**: by default the Qwen3-ASR model is unloaded after 10 minutes without speech and reloaded in the background (~2–3 s) while you talk.
- **Post-recognition replacements**: one `wrong => right` rule per line to fix words that are always misheard (ships with `Cloud Code => Claude Code`).
- **Hotwords** (Qwen3-ASR only, optional).

## Install

Build and install as a .deb (installs to `/usr`, managed by apt, includes udev rules and hwdb):

```bash
# 1. One time: build dependencies (needs sudo)
sudo bash linux/setup-system.sh

# 2. Build and package (no sudo) → dist-linux/voice-vibe-coding_<version>_amd64.deb
bash linux/package-deb.sh

# 3. Install / upgrade (asks for your sudo password); restarts the app if it is running
bash linux/install-deb.sh
```

Building needs Rust (rustup) and Node.js 18+. A built .deb can be copied to other Ubuntu 24.04 machines and installed with `sudo apt install ./voice-vibe-coding_<version>_amd64.deb`. The install scripts use `apt --no-remove`, so they never uninstall packages you already have.

What the package installs:

| Path | Contents |
|---|---|
| `/usr/bin/voice-vibecoding` | The app (speech engine statically linked) |
| `/usr/share/applications/Voice VibeCoding.desktop` | App menu entry |
| `/usr/lib/udev/rules.d/72-voice-vibecoding.rules` | Lets the logged-in user access `/dev/uinput` and the remote's nodes; the remote's power button won't shut down the PC |
| `/usr/lib/udev/hwdb.d/72-voice-vibecoding.hwdb` | Neutralizes the remote's native voice/power key codes so they never leak to the desktop during reconnects |
| `/usr/share/voice-vibecoding/bt-dualboot.py` | Tool to share one Bluetooth pairing between Windows and Linux (see below) |

Uninstall with `sudo apt remove voice-vibe-coding`. Settings, models and logs live in `~/.local/share/com.remote-bridge-hub.app/` and are kept.

Download speech models from the app (Xiaomi page → "Voice output" card → "Download model"). Sources are hf-mirror.com / huggingface.co, downloaded in parallel and verified with sha256.

To move a working setup to another machine, `bash linux/make-migration-bundle.sh` builds a tarball with the .deb, models, settings and a one-step installer (`--no-models` for a 17 MB version).

## Pair the remote

1. Open system Settings → Bluetooth.
2. Hold **Home + Menu** on the remote for about 3 seconds until the light blinks.
3. Click "MI RC" in the list.
4. In Voice VibeCoding, the status bar should show the Bluetooth bridge as connected and the remote keys as grabbed.

The remote reconnects on any key press after sleeping; the voice channel comes back within 1–2 seconds.

### Dual boot: one pairing for Windows and Linux

A Bluetooth remote only remembers one set of pairing keys per computer, so pairing in one OS breaks the other. Pair **last in Windows**, then import the Windows keys into Linux:

1. Boot Windows and pair the remote there.
2. **Restart** (not shut down with Fast Startup, not hibernate) into Ubuntu.
3. Before pressing any key on the remote, run:
   ```bash
   sudo python3 /usr/share/voice-vibecoding/bt-dualboot.py import-windows
   ```
   It mounts the Windows partition read-only, reads the LE pairing keys from the registry and writes them into BlueZ (the original file is backed up). It finds the remote automatically if Linux has paired with it before; otherwise pass its address: `import-windows AA:BB:CC:DD:EE:FF`. Needs `sudo apt install libhivex-bin`.
4. Press any key on the remote. Both systems now work without re-pairing.

`bt-dualboot.py inspect` compares both sides read-only without printing the keys.

## Use

- **Buttons**: same as Windows. In "Key mapping", click a button on the remote picture and record a shortcut. Recording grabs the keyboard, so system shortcuts like Super won't fire.
- **Voice (offline mode)**: hold the voice button, speak, release. Text is pasted at the cursor (default Shift+Insert, which also works in terminals). The "Voice output" card sets the model, paste method, clipboard restore, trailing punctuation, idle release, hotwords and replacements.
- **Voice (virtual microphone mode)**: pick "Voice VibeCoding remote microphone" as the input device in your app; holding the voice button also holds the shortcut mapped to it.
- **Start at login**: Settings → start at login writes to `~/.config/autostart/`; combine with "minimize to tray on start".

## Troubleshooting

| Symptom | Fix |
|---|---|
| Virtual keyboard: no permission | udev rules aren't active yet: install the .deb (or run `sudo bash linux/setup-system.sh`), then log out and back in |
| Remote not paired | See "Pair the remote"; "Bluetooth diagnostics" in the Voice output card lists what BlueZ sees |
| ATVV not connected | Click "Fix ATVV connection" (soft-restarts the Bluetooth session and resubscribes the voice channel) |
| Buttons do nothing | Check the log (Xiaomi page → Log), or run `voice-vibecoding diag-bluetooth` |
| Text isn't pasted | Try another paste method (some terminals need Ctrl+Shift+V). Wayland restricts clipboard and key injection; use an X11 session |
| A word is always wrong | Add `wrong => right` to replacements; if that's not enough, add it as a hotword (Qwen3-ASR only) |
| Low on memory / want instant output | Switch the model to SenseVoice Small |

Log file: `~/.local/share/com.remote-bridge-hub.app/logs/app.log`

### Testing without the remote

```bash
# Fake an "MI RC" device sending keys (same evdev → mapping → uinput path as the real remote)
voice-vibecoding simulate-remote up down ok back

# Feed a 16 kHz mono WAV as if the voice button were held (ATVV codec → recognition → paste)
voice-vibecoding --simulate-voice /path/to/16k-mono.wav
```

## Code layout

All Linux backends are in [`src-tauri/src/linux/`](src-tauri/src/linux/) (BlueZ, evdev input, uinput keyboard, X11 shortcut recording, PipeWire virtual mic, speech recognition, paste, autostart, diagnostics, simulators). Packaging is in [`linux/`](linux/) and [`src-tauri/tauri.linux.conf.json`](src-tauri/tauri.linux.conf.json). The shared state machines (ATVV protocol, ADPCM decoding, key mapping, shortcut recording engine, config) are reused unchanged. Windows code paths are separated with `cfg` and their behavior is unchanged (not compiled or tested on Windows by this port).

## Roadmap

- Drive coding agents directly from the remote, e.g. send dictated text to Codex CLI or Claude Code and map buttons to approve/reject actions.
- Optional cloud speech-to-text backend next to the offline models.
- More voice devices: other BLE remotes and USB microphones, and the T1 / Hanvon V60 remotes the UI already reserves.
- Wayland support for shortcut recording and pasting.

## Credits

- [mwlt/Voice_VibeCoding](https://github.com/mwlt/Voice_VibeCoding): the Rust + Tauri Windows app this port is based on ([Gitee mirror](https://gitee.com/mwlt/remote-voice-vibe-coding)).
- [xxb26553663-star/remote-bridge-hub](https://github.com/xxb26553663-star/remote-bridge-hub): the original Python version.
- [nijez/open-voice-bridge](https://github.com/nijez/open-voice-bridge): the macOS version.
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), Qwen3-ASR and SenseVoice for offline speech recognition.

## License

[MIT](LICENSE), same as upstream.
