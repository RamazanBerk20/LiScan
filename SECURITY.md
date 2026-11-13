# LiScan security model

LiScan scans file metadata locally. It does not upload file names, paths,
sizes, or contents, and it does not include analytics.

## Administrator scans

The graphical application always runs as the signed-in user. For an
administrator scan it starts `/usr/libexec/liscan/liscan-admin-helper` through
Polkit. The helper:

- requires authentication for every scan (`auth_admin`, no cached
  authorization that other programs of the same user could reuse);
- accepts one canonical absolute local path (no `..`, `.` or symbolic link)
  over a versioned, length-bounded CBOR pipe;
- returns large metadata trees in independently bounded chunks and validates
  declared node counts, ordering, parent links, and transfer completion;
- only traverses metadata and never opens file contents;
- cannot perform delete, move, launch, terminal, or network actions;
- never follows symbolic links (paths are checked with `lstat` and then read
  by name, so a user who owns a directory inside the scanned tree and swaps it
  for a link at the exact moment it is read can still redirect that one
  listing; the result only reaches the administrator who started the scan);
- deduplicates hard-linked files;
- traverses local filesystem boundaries, including Btrfs subvolumes, so
  protected data below the selected folder is not silently omitted;
- skips snapshot trees, repeated bind paths, and duplicate overlay mount views
  during broad scans while allowing an explicitly selected snapshot target;
- always excludes `/proc`, `/sys`, `/dev`, and `/run`, and skips every
  kernel virtual filesystem (procfs, sysfs, devtmpfs, cgroup, …) and autofs
  trigger by filesystem type wherever it is mounted;
- excludes remote mounts and reports every omission;
- exits when the scan completes or its controlling pipe closes.

Cleanup actions are handled by the unprivileged application. Trash is the
default; permanent deletion requires an explicit typed confirmation. Known
limit: Trash and delete act on the path recorded during the scan and do not
re-verify that it still names the scanned object. A user who owns a directory
above the target could swap it for a symbolic link after the scan, and a
permanent delete of a folder also removes the contents of any filesystem
mounted inside it, including mounts the scan skipped.

## Scan history and the Reclaimable lens

With "Keep scan history" on (the default), LiScan stores, per scanned target,
up to 10 snapshots under `$XDG_DATA_HOME/liscan/history/` (normally
`~/.local/share/liscan/history/<16 hex characters of the SHA-256 of the target
path>/<unix time>.json`). A snapshot holds the target path, the time, the total
size, and folder paths and sizes down to four levels, so it reveals folder names
to anyone who can read your home folder. Directories are created with mode
0700 and files with 0600, files are written atomically, nothing is uploaded, and
Settings can turn the feature off or delete it all. Administrator scans are never
written to history and cannot be compared (they list folders only root may see,
and the history is readable by any process of your user without authentication).
A stored remote target has any user name and password removed. The Reclaimable lens is
read-only: it matches paths against a fixed table and only displays a command
for you to copy; LiScan never executes those commands or deletes anything on
their behalf.

## Reporting a vulnerability

Please use the repository's private security-advisory channel. Include the
affected version, reproduction steps, and expected impact. Do not include
unredacted private file paths in a public issue.
