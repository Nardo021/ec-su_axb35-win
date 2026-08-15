# Changelog

All notable changes to EVO-X2 Control are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[2.1.0]: https://github.com/Nardo021/ec-su_axb35-win/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/Nardo021/ec-su_axb35-win/releases/tag/v2.0.0
