# LiScan AUR packages

Each directory mirrors one AUR package base:

- `liscan`: stable release built from source.
- `liscan-bin`: stable prebuilt GitHub release.
- `liscan-git`: latest `main` branch built from source.

Update the matching `PKGBUILD`, regenerate `.SRCINFO` with
`makepkg --printsrcinfo`, validate the package in a clean environment, and
push only those two files to the corresponding AUR Git repository.
