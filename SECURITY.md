# LiScan security model

LiScan scans file metadata locally. It does not upload file names, paths,
sizes, or contents, and it does not include analytics.

## Administrator scans

The graphical application always runs as the signed-in user. For an
administrator scan it starts `/usr/libexec/liscan/liscan-admin-helper` through
Polkit. The helper:

- accepts one absolute local path over a versioned, length-bounded CBOR pipe;
- returns large metadata trees in independently bounded chunks and validates
  declared node counts, ordering, parent links, and transfer completion;
- only traverses metadata and never opens file contents;
- cannot perform delete, move, launch, terminal, or network actions;
- never follows symbolic links;
- deduplicates hard-linked files;
- traverses local filesystem boundaries, including Btrfs subvolumes, so
  protected data below the selected folder is not silently omitted;
- skips snapshot trees, repeated bind paths, and duplicate overlay mount views
  during broad scans while allowing an explicitly selected snapshot target;
- always excludes `/proc`, `/sys`, `/dev`, and `/run`;
- excludes remote mounts and reports every omission;
- exits when the scan completes or its controlling pipe closes.

Cleanup actions are handled by the unprivileged application. Trash is the
default; permanent deletion requires an explicit typed confirmation.

## Reporting a vulnerability

Please use the repository's private security-advisory channel. Include the
affected version, reproduction steps, and expected impact. Do not include
unredacted private file paths in a public issue.
