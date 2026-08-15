# EVO-X2 Control for Windows

This fork replaces WinRing0 with PawnIO.

Secure Boot can remain enabled.

A Windows control and monitoring solution for the onboard Embedded Controller
(ITE IT5570E) on [Sixunited SU_AXB35](https://strixhalo-homelab.d7.wtf/Hardware/Boards/Sixunited-AXB35)
boards used by the GMKtec EVO-X2. It is a distant relative of
[ec-su_axb35-linux](https://github.com/cmetz/ec-su_axb35-linux). One program,
`evox2-control.exe`, is the window, the tray icon, and the CLI (`evox2ctl`).

This application does not require Secure Boot to be disabled.

See [docs/PAWNIO_MIGRATION.md](docs/PAWNIO_MIGRATION.md) for the driver change.

## Requirements

- GMKtec EVO-X2 / Sixunited AXB35 hardware
- EC firmware 1.04 or higher
- Windows 11 (Windows 10 is untested)
- Official [PawnIO](https://pawnio.eu/) (signed release)
- Administrator privileges to run the program

Secure Boot, Test Signing, and Memory Integrity/HVCI can stay in their default
secure configuration.

## Installation

1. Install the official PawnIO release from https://pawnio.eu/.
2. Download the latest [GitHub Release](https://github.com/Nardo021/ec-su_axb35-win/releases),
   run `ec-su_axb35-win-installer-2.0.0.exe` as Administrator, or copy the
   binaries from `dist/`.
3. Double-click `evox2-control.exe`. That is the only program you need.
4. Closing the window hides it to the tray by default. Curve monitoring keeps
   running. Use the tray menu or Settings to quit. Power mode and fan settings
   are written immediately and restored the next time the app starts.

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

The APU block shows temperature, the current power mode, and a selector:

```text
Temperature: 48°C  GPU driver (Task Manager)
Current mode: Balanced

Power Mode
[ Quiet ] [ Balanced ] [ Performance ]
```

Fan blocks still support auto / fixed / curve, RPM, and temperature charts.
Curve mode uses the same temperature as the APU block.

### Temperature

The APU temperature is **not** the EC `0x70` byte. On current EVO-X2 firmware
that register can sit near 97°C while Task Manager shows a GPU temperature in
the 40s. Display, charts, and fan curves use one source, in this order:

1. AMD GPU driver via WDDM (the same sensor Task Manager uses)
2. AMD ADL, if the driver exposes it
3. EC register `0x70`, only if both driver paths fail

The GUI labels the source next to the number. `evox2ctl status` prints it in
parentheses. `evox2ctl diagnose` also prints the raw EC `0x70` value so the
two sensors can be compared.

### Tray

Left-click the tray icon to show the window.

Right-click for Quiet / Balanced / Performance, Show window, and Exit.

### Settings

On the same window:

- Close window: minimize to tray (default) or quit the program
- Start with Windows: off by default; writes `HKCU\...\Run` as `EVO-X2 Control`
- Language: 中文 (default) or English

There is no Windows service and no REST API.

## CLI usage

```powershell
evox2ctl mode
evox2ctl mode quiet
evox2ctl mode balanced
evox2ctl mode performance
evox2ctl mode performance --dry-run
evox2ctl status
evox2ctl diagnose
```

Shortcuts can point at:

```powershell
evox2ctl.exe mode quiet
evox2ctl.exe mode balanced
evox2ctl.exe mode performance
```

`status` includes the APU temperature and its source. `diagnose` does not
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
| APU temperature near 97°C while Task Manager GPU is ~45°C | Use a build that reads the GPU driver; `diagnose` should show the driver value and a separate EC `0x70` raw value |
| Temperature source says `EC 0x70` | AMD driver temperature was unavailable; confirm the AMD GPU driver is installed |
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
  "language": "zh"
}
```

`host` and `port` may remain in the file from older builds. They are not used.

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
git tag v2.0.1
git push origin v2.0.1
```

The tag must look like `v2.0.1` and match Cargo. The Release includes
`evox2-control.exe`, `evox2ctl.exe`, and a zip of both. PawnIO is not bundled.

## Building from source

```powershell
cargo test --workspace
cargo build --workspace --release
.\scripts\package.ps1
```

Optional installer: compile `installer.nsi` with NSIS after the release
binaries exist.

CI runs `cargo fmt --check`, `clippy`, `test`, and a release build. Hardware
tests are not run in CI.

## Help

Issues: https://github.com/deseven/ec-su_axb35-win/issues/new

Strix Halo HomeLab Discord: https://discord.gg/pnPRyucNrG
