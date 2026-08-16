# Changelog

All notable changes to ec-su_axb35-win are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.5.1] - 2026-08-16

Local privilege-escalation hardening. Fan and curve behavior is unchanged.

### Security

- `config.json` `log_path` is ignored. The log is always
  `%SYSTEMDRIVE%\ProgramData\ec-su_axb35-win\server.log`. Invalid fan or
  power-mode values in the file are reset instead of being written to the EC.
- That ProgramData folder is re-ACL'd to SYSTEM and Administrators only
  (no inherited Users write).
- `schtasks.exe` is taken from `GetSystemDirectoryW`, not `%WINDIR%`.
- Start with Windows only registers an ONLOGON / Highest task when the
  program lives under Program Files. A portable or `dist\` copy refuses
  the setting and deletes an existing unsafe `EVO-X2 Control` task.
- AMD ADL / `gdi32` load from System32 only. The process also restricts
  the default DLL search path to System32.
- The installer removes the logon task on install and uninstall.

### Changed

- Portable zip builds can no longer enable Start with Windows. Use the
  installer if you want the logon task.

## [2.5.0] - 2026-08-16

Curve mode in the window is a five-row ramp-up table. Ramp-down is
derived automatically (about 8°C below) and shown read-only.

### Changed

- Fan cards no longer use comma-separated temperature fields or a
  staircase chart. Edit five ramp-up thresholds (0→1 … 4→5). Ramp-down
  is derived automatically (8°C lower, still below each ramp-up point)
  and written on pointer-up. Until a fan is edited, the stored
  ramp-down from `config.json` is shown in the read-only column.
- CLI `evox2ctl fan … curve` still takes two 5-value lists.
- Curve mode can restore the stock ramp-up and ramp-down for that fan.

## [2.4.0] - 2026-08-16

Curve mode no longer leaves fans stuck on a software level after the
app exits, and fan curves average recent temperatures before changing
level.

### Added

- Exiting, signing out, or shutting down writes curve fans back to EC
  AUTO. Saved curve settings stay in `config.json` and are restored the
  next time the window starts. Fixed and auto fans are left alone.
- Settings slider for curve smoothing window (1–20 samples, default 8).
  Fan curves use the average of the last N readings before changing
  level. Export/import include this value.
- Processor card title uses the Windows host name and can be renamed.

### Changed

- `evox2ctl fan … curve` is refused unless `evox2-control` is already
  running and monitoring. `--dry-run` still works without the window.

## [2.3.1] - 2026-08-16

Idle CPU no longer stays near 3% of the machine after the window is
hidden to the tray.

### Fixed

- Closing to the tray no longer busy-spins the eframe event loop on
  Windows. The GUI skips painting while hidden and only wakes about once
  a second for tray events and alerts.

## [2.3.0] - 2026-08-16

Selectable temperature source, custom fan names, and the original
`ec-su_axb35-win` product title. Still no network API and no Windows service.

### Added

- Settings temperature source: GPU (default), CPU, SoC, GPU hotspot, or EC
  `0x70`. Fan curves and alerts follow that choice, with GPU then EC fallback.
- Rename fans from each card, with restore-default names.
- Settings footer shows version and author. About still has the full details.

### Changed

- Window title and product name are `ec-su_axb35-win`.
- The processor card replaces the APU label.
- Default language is English when `language` is missing.

### Fixed

- Turning off Start with Windows failed when the logon task was already
  missing (Chinese `schtasks` text was decoded as UTF-8).

## [2.2.0] - 2026-08-15

Daily-use, installer, and CLI completeness on top of 2.1.0. Still no network
API and no Windows service.

### Added

- About and Diagnostics pages (copy report, open log / log folder).
- Temperature tray balloon when the APU stays at or above a configurable
  threshold (default 90°C, 10 minute cooldown).
- Native configuration import/export with validation before any EC write.
- `evox2ctl fan` with cpu/secondary/system aliases, `--json`, and stable
  exit codes (0/1/2/3/4/5/6).
- LICENSE, installer license page (English + Simplified Chinese), optional
  Authenticode signing script, and NSIS installer in CI/release.

### Changed

- Fan cards use CPU / secondary CPU-APU / system names.
- Logs append and rotate to `server.log.1` around 2 MB.
- Debug and release app binaries request Administrator; DPI awareness is
  embedded. Publisher and project links point at this fork.
- Installer no longer treats a Windows service as the product; leftover
  `ec-su_axb35-win` services are still removed on upgrade/uninstall.

## [2.1.0] - 2026-08-15

Windows desktop release after 2.0.0. User-visible UI and autostart behavior
changed enough to warrant a minor bump rather than 2.0.1.

### Added

- Follow the Windows app light/dark setting and DWM accent color.
- Settings is a separate page, opened from the header gear and closed with
  Back or Esc.
- The main window scrolls so APU plus every fan stays reachable.
- Start with Windows uses a Task Scheduler logon task (`EVO-X2 Control`)
  running at highest privileges, which HKCU Run cannot do for this app.

### Changed

- Default window size is smaller; the window can be resized.
- Power mode, fan mode, level, and curve controls stay on the cards. There
  is no separate edit/apply layer.
- Segoe UI, Microsoft YaHei, Consolas, and Segoe Fluent / MDL2 icons when
  those fonts are present.

### Fixed

- Start with Windows could not register or unregister correctly.
- Tray left-click did not show the window; tray power-mode items did not
  apply.
- Choosing Exit on the tray left the process running.

## [2.0.0] - 2026-08-14

First PawnIO release: one window plus tray, no Windows service, Secure Boot
can stay enabled. See the GitHub Release for that tag.

[2.5.1]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.5.0...v2.5.1
[2.5.0]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.4.0...v2.5.0
[2.4.0]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.3.1...v2.4.0
[2.3.1]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.3.0...v2.3.1
[2.3.0]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.2.0...v2.3.0
[2.2.0]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/Nardo021/ec-su_axb35-win/releases/tag/v2.0.0
