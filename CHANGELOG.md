# Changelog

## Unreleased

### Changed
- LiScan draws its own window bar: the toolbar (and the start screen's top row) is now the title bar, with minimise, maximise and close at its end, so the system title bar no longer sits above it. Drag its empty space to move the window, double-click it to maximise; the path stays selectable text. The thin scan-progress line now runs along the bar's bottom edge instead of under the status strip.

## [1.1.0] - 2026-11-13

### Added
- `liscan --json` mode for scripted scans.
- Scan history with a what-changed comparison. It is on by default, keeps the newest 10 scans per target under `~/.local/share/liscan/history/`, and can be turned off in settings. Administrator scans are never recorded.
- Reclaimable view.
- A quiet GitHub Sponsors link.

### Changed
- The administrator password is now asked on every administrator scan; authorization is no longer reused between scans.
- The administrator helper protocol is now version 3 (paths travel as raw bytes). The app and helper must be the same version, so reinstall the whole package; older helpers are not compatible.
- Exclusions now work on canonical paths and filesystem types; directory traversal is iterative and views are bounded.

### Fixed
- Scan results and events from different scans could be mixed up; scan events are now tagged by generation.
- Administrator scans failing in several cases.
- Non-UTF-8 file names in administrator scans.

### Security
- Polkit action uses `auth_admin` (re-authentication each time).
- Helper protocol validates raw-byte paths.

### Not tested
- Administrator scans, Polkit prompts and the installed DEB/RPM packages were not exercised on real hardware for this release; automated tests only.
