# LiScan

LiScan is a local-first disk usage analyzer for Linux. It combines a radial
file map with a sortable folder list, clear coverage reporting, safe cleanup
actions, remote-location support through GIO, and a narrowly scoped
**Scan as administrator** workflow.

![LiScan application icon](src-tauri/icons/icon.svg)

## Features

- Scan a home folder, root filesystem, mounted volume, chosen folder, dropped
  folder, or GIO URI such as `sftp://` and `smb://`.
- See used, free, and total capacity plus the used/total percentage for the
  active local filesystem in both the overview and scan workspace.
- Explore a zoomable radial map with back, forward, up, recenter, adjustable
  depth, tooltips, keyboard navigation, and a text-list alternative.
- Sort children by name, allocated size, apparent size, item count, or
  percentage; inspect permissions, modification time, hard links, and paths.
- Open or reveal items, copy paths, open a terminal, move items to Trash, or
  use separately confirmed permanent deletion.
- Rescan, cancel in progress, choose filesystem-boundary behavior, include or
  exclude remote/removable mounts, group small files, and manage exclusions.
- See permission failures, changed files, I/O errors, boundaries, and policy
  exclusions in one coverage report.
- Administrator scans include local filesystems and Btrfs subvolumes below the
  selected folder. Runaway broad scans are avoided by skipping snapshot trees,
  bind-mount repeats, and duplicate container overlay views. A snapshot folder
  can still be scanned directly when its contents are the intended target.
- Follow the system light/dark theme, choose map colors and contrast, and
  display sizes with either IEC binary units (`KiB`, `MiB`, `GiB`, base 1024)
  or SI decimal units (`kB`, `MB`, `GB`, base 1000), and respect reduced-motion
  settings.
- Follow the system language by default or choose English, Turkish, Spanish,
  Italian, French, German, Russian, Arabic, Simplified Chinese, Japanese, or
  Korean. Numbers use the selected locale and Arabic uses a right-to-left
  layout.
- Start scans from a directory association, command line, drag-and-drop, or
  KDE file-manager context menu.

## Administrator scans

LiScan never elevates its graphical interface. It asks Polkit to start a fixed
helper installed at `/usr/libexec/liscan/liscan-admin-helper`, sends one
validated local path over a bounded CBOR protocol, and receives metadata-only
results in bounded chunks so large scans do not require one oversized protocol
frame. The helper never follows symlinks or reads file contents, disables new
privileges, excludes `/proc`, `/sys`, `/dev`, `/run`, and remote mounts, and
reports every skipped path. Broad scans also skip filesystem snapshots and
duplicate container mount views; selecting one of those folders directly scans
it intentionally. File changes remain unprivileged.

See [SECURITY.md](SECURITY.md) for the complete boundary.

## Build

Requirements:

- Rust 1.95 or newer
- Node.js 22 and pnpm 11
- GTK 3, GLib, WebKitGTK 4.1, librsvg, and Polkit development packages
- GIO/GVfs backends for remote scans

```bash
pnpm install --frozen-lockfile
pnpm check
pnpm tauri dev
```

## Dev Container

LiScan includes a Debian 12 development container with the pinned Rust, Node,
pnpm, Tauri system libraries, DEB/RPM packaging tools, and GIO backends needed
by the project. It also includes a lightweight virtual Linux desktop, so the
GUI works consistently in VS Code Dev Containers and GitHub Codespaces
without host-specific X11 or Wayland mounts.

1. Install Docker and a [Dev Container compatible
   tool](https://containers.dev/supporting.html).
2. Open this repository in the container. Dependencies are installed
   automatically the first time it is created.
3. Run the checks:

   ```bash
   pnpm check
   ```

4. Start LiScan:

   ```bash
   pnpm dev:container
   ```

5. Open the forwarded **LiScan desktop** port (`6080`), connect with the
   default password `vscode`, and use the app in the virtual desktop.

The container can exercise the administrator-helper protocol in debug mode,
but it intentionally cannot elevate into or scan the host filesystem. Validate
the installed Polkit integration with a native package on a Linux host.

For a development-only administrator scan, first build both binaries and use
the explicit bypass:

```bash
pnpm build
cargo build --manifest-path src-tauri/Cargo.toml --bin liscan
cargo build --manifest-path src-tauri/admin-helper/Cargo.toml --target-dir src-tauri/target
LISCAN_ADMIN_DIRECT=1 pnpm tauri dev
```

`LISCAN_ADMIN_DIRECT` is compiled for debug builds only. Release builds always
use `/usr/bin/pkexec` and the installed fixed helper path.

## Packages

`pnpm package:linux` runs tests and creates DEB and RPM bundles. It requires
the native `dpkg-deb` and `rpmbuild` tooling. Bundle output is written below
`src-tauri/target/release/bundle`.

Tagged releases publish ready-to-install DEB and RPM packages on
[GitHub Releases](https://github.com/RamazanBerk20/LiScan/releases). Arch Linux
users can choose one of three AUR variants:

```bash
yay -S liscan       # stable release, built from source
yay -S liscan-bin   # stable release, prebuilt
yay -S liscan-git   # latest main branch, built from source
```

The maintained AUR recipes are in `packaging/aur`. DEB, RPM, and AUR installs
include the Polkit policy, administrator helper, AppStream metadata, desktop
entry, scalable icon, and KDE service menu.

## Command line

```text
liscan [--admin] [--home] [--new-window] [PATH_OR_URI]
```

`--admin` accepts absolute local paths only. `--home` scans the current user's
home folder. Without a target, LiScan opens its overview.

## License

LiScan is licensed under GPL-3.0-or-later.
