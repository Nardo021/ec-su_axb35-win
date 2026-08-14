# EVO-X2 Control for Windows

This fork replaces WinRing0 with PawnIO.

Secure Boot can remain enabled.

A Windows control and monitoring solution for the onboard Embedded Controller
(ITE IT5570E) on [Sixunited SU_AXB35](https://strixhalo-homelab.d7.wtf/Hardware/Boards/Sixunited-AXB35)
boards used by the GMKtec EVO-X2. It is a distant relative of
[ec-su_axb35-linux](https://github.com/cmetz/ec-su_axb35-linux) and consists of
a privileged Windows service, an optional GUI client, and `evox2ctl`.

This application does not require Secure Boot to be disabled.

See [docs/PAWNIO_MIGRATION.md](docs/PAWNIO_MIGRATION.md) for the driver change.

## Requirements

- GMKtec EVO-X2 / Sixunited AXB35 hardware
- EC firmware 1.04 or higher
- Windows 11 (Windows 10 is untested)
- Official [PawnIO](https://pawnio.eu/) (signed release)
- Administrator privileges to install the service

Secure Boot, Test Signing, and Memory Integrity/HVCI can stay in their default
secure configuration.

## Installation

1. Install the official PawnIO release from https://pawnio.eu/.
2. Run `ec-su_axb35-win-installer-2.0.0.exe` as Administrator, or copy the
   release binaries from `dist/`.
3. The installer registers the `ec-su_axb35-win` service
   (`ec-su_axb35-server.exe --service`).
4. Start the optional GUI client, or use `evox2ctl`.

Do not install WinRing0, inpoutx64, or any unsigned kernel driver.

## PawnIO

PawnIO is an external official dependency. This project:

- talks to `\\?\GLOBALROOT\Device\PawnIO`
- loads the official signed `LpcACPIEC` module
- uses only `ioctl_pio_read` / `ioctl_pio_write` on ports `0x62` and `0x66`

It does not bundle `PawnIO.sys`, does not download kernel binaries at runtime,
and does not use the unrestricted/test-signed PawnIO edition.

If PawnIO is missing, the service exits with a readable error and does not
crash the machine.

## Secure Boot

Secure Boot can remain **Enabled**. The application only reads the current
state (`Secure Boot: Enabled`) and never attempts to change it.

## GUI usage

The GUI client talks to `http://127.0.0.1:8395` and does not need direct EC
privileges once the service is running.

The APU block shows the current power mode and a selector:

```text
Current mode: Balanced

Power Mode
[ Quiet ] [ Balanced ] [ Performance ]
```

Fan blocks still support auto / fixed / curve, RPM, and temperature charts.

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

`diagnose` does not change hardware state.

## REST API

Default bind: `http://127.0.0.1:8395`

Do not expose the unauthenticated REST API directly to the Internet.

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/status` | EC firmware and runtime status |
| GET | `/metrics` | Power mode, temperature, fans |
| GET/POST | `/apu/power_mode` | `quiet` / `balanced` / `performance` |
| GET | `/apu/temp` | APU temperature |
| GET | `/fanX/rpm` | Fan RPM |
| GET/POST | `/fanX/mode` | `auto` / `fixed` / `curve` |
| GET/POST | `/fanX/level` | Fixed level 0-5 |
| GET/POST | `/fanX/rampup_curve` | Curve thresholds |
| GET/POST | `/fanX/rampdown_curve` | Curve thresholds |

There is no arbitrary `/ec/write` endpoint.

OpenAPI: [server/openapi.yaml](server/openapi.yaml).

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| `PawnIO is required for hardware access` | Install official PawnIO and reboot if needed |
| `Unsupported EC firmware` | Update EC firmware to 1.04+ |
| `Unsupported hardware` | Confirm the machine is AXB35 / EVO-X2 |
| `EC timeout waiting for input buffer to clear` | Another EC client may be holding the ports |
| GUI cannot connect | Confirm the service is running on `127.0.0.1:8395` |
| Service will not start | `%SYSTEMDRIVE%\ProgramData\ec-su_axb35-win\server.log` |

## Architecture

```text
Windows
   │
   ├── EVO-X2 Control Service  (Administrator / LocalSystem)
   │      ├── PawnIO + LpcACPIEC
   │      ├── EC controller
   │      └── localhost REST API
   │
   ├── GUI client              (no direct EC access)
   └── evox2ctl                (localhost API + diagnostics)
```

Configuration: `%SYSTEMDRIVE%\ProgramData\ec-su_axb35-win\config.json`

```json
{
  "host": "127.0.0.1",
  "port": 8395,
  "log_path": "C:\\ProgramData\\ec-su_axb35-win\\server.log"
}
```

## Security

- Treat EC control as privileged hardware access.
- The API has no authentication.
- Default bind is loopback only.
- The installer does not open Windows Firewall ports.
- Unknown machines do not receive speculative EC writes.
- Do not expose the unauthenticated REST API directly to the Internet.

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
