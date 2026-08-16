# ec-su_axb35-win

This fork replaces WinRing0 with PawnIO.

Secure Boot can remain enabled.

A Windows control and monitoring solution for the onboard Embedded Controller
(ITE IT5570E) on [Sixunited SU_AXB35](https://strixhalo-homelab.d7.wtf/Hardware/Boards/Sixunited-AXB35)
boards used by the GMKtec EVO-X2. It is a distant relative of
[ec-su_axb35-linux](https://github.com/cmetz/ec-su_axb35-linux). One program,
`evox2-control.exe`, is the window, the tray icon, and the CLI (`evox2ctl`).

This application does not require Secure Boot to be disabled.

See [docs/PAWNIO_MIGRATION.md](docs/PAWNIO_MIGRATION.md) for the driver change.

**Download the latest release, currently [v2.4.0](https://github.com/Nardo021/ec-su_axb35-win/releases/latest).**
Most people should take `ec-su_axb35-win-installer-2.4.0.exe`. See [Download](#download).

## Requirements

- GMKtec EVO-X2 / Sixunited AXB35 hardware
- EC firmware 1.04 or higher
- Windows 11 (Windows 10 is untested)
- Official [PawnIO](https://pawnio.eu/) (signed release)
- Administrator privileges to run the program

Secure Boot, Test Signing, and Memory Integrity/HVCI can stay in their default
secure configuration.

## Download

Use the **latest** GitHub Release (currently **v2.4.0**):

https://github.com/Nardo021/ec-su_axb35-win/releases/latest

Do not use older tags on this repo. Do not use upstream
[deseven/ec-su_axb35-win](https://github.com/deseven/ec-su_axb35-win) builds;
those still rely on WinRing0.

| File on the release page | Who it is for |
| --- | --- |
| `ec-su_axb35-win-installer-2.4.0.exe` | Normal install. **Download this** unless you have a reason not to. |
| `evox2-control-v2.4.0.zip` | Portable copy: GUI and CLI in one folder. |
| `evox2-control.exe` | Window and tray only. Same program as the CLI binary. |
| `evox2ctl.exe` | Command line only. Same program as `evox2-control.exe`, renamed. |
| Source code (zip / tar.gz) | Source for building. **Not** a Windows app. Skip this. |

PawnIO is not in any of those files. Install it from https://pawnio.eu/ first.

## Installation

1. Install the official PawnIO release from https://pawnio.eu/.
2. Download the file named in [Download](#download). Run the installer as
   Administrator, or unpack the zip / copy the binaries from `dist/`. Windows
   may show a SmartScreen warning until the binaries are Authenticode-signed;
   this project does not ship a self-signed certificate. The GUI and
   `evox2ctl` both request Administrator rights (UAC) because EC access
   through PawnIO requires elevation.
3. Double-click `evox2-control.exe`. That is the only program you need.
4. Closing the window hides it to the tray by default. Curve monitoring keeps
   running. Use the tray menu or Settings to quit. Quitting (or Windows
   shutdown / sign-out) writes curve fans back to firmware AUTO. Power mode
   and fan settings are written immediately and restored the next time the
   app starts.

If an older build left the `ec-su_axb35-win` Windows service running, stop and
remove it so it does not keep a second copy in the background:

```powershell
sc.exe stop ec-su_axb35-win
sc.exe delete ec-su_axb35-win
```

Do not install WinRing0, inpoutx64, or any unsigned kernel driver.

## PawnIO

PawnIO is an external official dependency. This project:

- talks to `\\?\GLOBALROOT\Device\PawnIO`
- loads the official signed `LpcACPIEC` module
- uses only `ioctl_pio_read` / `ioctl_pio_write` on ports `0x62` and `0x66`

It does not bundle `PawnIO.sys`, does not download kernel binaries at runtime,
and does not use the unrestricted/test-signed PawnIO edition.

If PawnIO is missing, the window shows a message in the current language and
opens https://pawnio.eu/. It does not download a driver.

## Secure Boot

Secure Boot can remain **Enabled**. The application only reads the current
state (`Secure Boot: Enabled`) and never attempts to change it.

## GUI usage

Double-click `evox2-control.exe` (Administrator). The GUI talks to the EC
in-process. It does not need a separate server process.

A second double-click focuses the existing window instead of opening another
EC session.

The processor block is titled with this computer's host name (rename it
like a fan). It shows temperature, the current power mode, and a selector:

```text
Temperature: 48°C  GPU driver (Task Manager)
Current mode: Balanced

Power Mode
[ Quiet ] [ Balanced ] [ Performance ]
```

Fan blocks are labeled CPU, secondary CPU, and system. They still
support auto / fixed / curve, RPM, and temperature charts. Curve mode uses
the same temperature as the processor block.

### Temperature

The processor block shows the **selected temperature source** (GPU by default)
plus every sensor that is actually available:

1. GPU via WDDM (the same sensor Task Manager uses), else AMD ADL GFX/Edge
2. CPU via AMD ADL PMLOG, when the driver exposes it
3. SoC via AMD ADL PMLOG, when the driver exposes it
4. GPU hotspot via AMD ADL
5. EC register `0x70` (ACPI CPUT). This is **not** accurate on current
   EVO-X2 firmware (it can sit near 97°C)

Choose the source in Settings. Fan curves and temperature alerts follow
that choice. If the selected sensor is missing, the app falls back to GPU,
then to EC `0x70`. `evox2ctl diagnose` prints every sensor plus the raw EC
byte so they can be compared.

### Tray

Left-click the tray icon to show the window.

Right-click for Quiet / Balanced / Performance, Show window, and Exit.

### Settings

Open Settings from the gear in the main window header. Esc or the back
control returns to processor and fan controls.

- Close window: minimize to tray (default) or quit the program
- Start with Windows: off by default; creates an on-logon Task Scheduler
  entry named `EVO-X2 Control` with highest privileges (required for PawnIO)
- Language: English (default) or Chinese
- Temperature source: GPU (default), CPU, SoC, GPU hotspot, or EC 0x70.
  This is the processor reading used for fan curves and alerts
- Curve smoothing window: average of the last 1–20 temperature readings
  (default 8, about 2 seconds) before a curve fan changes level. 1 is
  the old immediate behavior
- Temperature alert: tray balloon when the processor temperature stays at or above
  the threshold (default 90°C, range 70–97). Alerts wait at least 10 minutes
  between notifications
- Export / import configuration (native file dialog). Import validates fan
  mode, level, and 5-point curves, then restores onto the EC. A bad file is
  rejected and does not write hardware
- Open log / log folder, About, and Diagnostics. About is read-only.
  Diagnostics can copy its report and open the log

There is no Windows service and no REST API.

## CLI usage

`evox2ctl` is the same elevated binary. UAC still applies.

```powershell
evox2ctl mode
evox2ctl mode quiet
evox2ctl mode balanced
evox2ctl mode performance
evox2ctl mode performance --dry-run
evox2ctl status
evox2ctl diagnose
evox2ctl fan
evox2ctl fan 1
evox2ctl fan cpu auto
evox2ctl fan 1 fixed 3
evox2ctl fan 3 curve 20,60,83,95,97 0,50,80,94,96
evox2ctl fan 1 auto --dry-run
```

`fan … curve` needs `evox2-control` already running so the window can keep
adjusting levels. Without the window the command exits with code 2 and
does not leave the EC in manual mode. `--dry-run` is allowed either way.

```powershell
evox2ctl --json status
evox2ctl --json diagnose
```

Fan identity: `1` / `cpu`, `2` / `secondary`, `3` / `system`.

With `--json`, successful `status`, `diagnose`, `mode`, and `fan` output is
JSON on stdout. Errors with `--json` are JSON on stderr:
`{"error":"...","code":N}`.

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Success, including `--dry-run` |
| 1 | Other runtime / EC error |
| 2 | Usage / invalid arguments, including `fan … curve` without the GUI |
| 3 | Not running as Administrator |
| 4 | PawnIO unavailable |
| 5 | Unsupported hardware (writes refused) |
| 6 | EC firmware too low |

```powershell
evox2ctl --json status
if ($LASTEXITCODE -ne 0) { throw "evox2ctl failed: $LASTEXITCODE" }
$status = evox2ctl --json status | ConvertFrom-Json
$status.temperature
evox2ctl --json fan cpu | ConvertFrom-Json | Select-Object id, mode, rpm
```

Shortcuts can point at:

```powershell
evox2ctl.exe mode quiet
evox2ctl.exe mode balanced
evox2ctl.exe mode performance
```

`status` includes the processor temperature and its source. `diagnose` does not
change hardware state; it also prints the raw EC `0x70` temperature. CLI text
follows the `language` value in `config.json`.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| `PawnIO is required for hardware access` | Install official PawnIO and reboot if needed |
| `Unsupported EC firmware` | Update EC firmware to 1.04+ |
| `Unsupported hardware` | Confirm the machine is AXB35 / EVO-X2 |
| `EC timeout waiting for input buffer to clear` | Another EC client may be holding the ports |
| GUI cannot start | Confirm PawnIO is installed, the app is elevated, and no leftover service is running |
| Processor temperature near 97°C while Task Manager GPU is ~45°C | Control temp should follow GPU/CPU/SoC from the AMD driver; `diagnose` still prints raw EC `0x70` |
| Temperature source says `EC 0x70` | AMD driver temperatures were unavailable; confirm the AMD GPU driver is installed |
| CPU or SoC missing in the processor card | The installed ADL/PMLOG path did not expose that sensor; GPU-only is still valid |
| Settings did not come back | `%SYSTEMDRIVE%\ProgramData\ec-su_axb35-win\config.json` |

## Architecture

```text
evox2-control.exe     one window + tray, in-process EC
   close window    → hide to tray (default) or exit
   next launch     → restore config.json onto the EC
   second launch   → show the existing window

evox2ctl.exe          same binary, one-shot CLI
```

Configuration: `%SYSTEMDRIVE%\ProgramData\ec-su_axb35-win\config.json`

```json
{
  "host": "127.0.0.1",
  "port": 8395,
  "log_path": "C:\\ProgramData\\ec-su_axb35-win\\server.log",
  "close_to_tray": true,
  "start_with_windows": false,
  "language": "en",
  "temp_alert_enabled": true,
  "temp_alert_celsius": 90,
  "smoothing_window": 8
}
```

`host` and `port` may remain in the file from older builds. They are not used.
Import/export copies power mode, fans, tray, autostart, language, alert
settings, and the smoothing window. The log is appended and rotated to
`server.log.1` around 2 MB.

## Security

- Treat EC control as privileged hardware access.
- There is no network API.
- The installer does not open Windows Firewall ports.
- Unknown machines do not receive speculative EC writes.

## Publishing a release

Push a version tag after `server/Cargo.toml` has the same version. GitHub
Actions builds the Windows binaries and publishes a Release:

```powershell
# bump version in server/Cargo.toml first, then:
git tag v2.4.0
git push origin v2.4.0
```

The tag must look like `v2.4.0` and match Cargo. The Release includes
`evox2-control.exe`, `evox2ctl.exe`, a zip of both, and the NSIS installer
when NSIS is available. Optional Authenticode signing runs only when
`SIGNING_PFX` / `SIGNING_PASSWORD` secrets exist. PawnIO is not bundled.

Release notes live in [CHANGELOG.md](CHANGELOG.md).

## Building from source

```powershell
cargo test --workspace
cargo build --workspace --release
.\scripts\package.ps1
```

Optional installer: compile `installer.nsi` with NSIS after the release
binaries exist.

CI runs `cargo fmt --check`, `clippy -D warnings`, `test`, a release build,
and `makensis installer.nsi`. Hardware tests are not run in CI.

## License

Application code is under [LICENSE](LICENSE) (deseven upstream and this fork).
The bundled `LpcACPIEC` module is LGPL-2.1-or-later from PawnIO.Modules.

## Help

Issues: https://github.com/Nardo021/ec-su_axb35-win/issues/new

Strix Halo HomeLab Discord: https://discord.gg/pnPRyucNrG
