#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

pnpm install --frozen-lockfile
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --locked
pnpm tauri build --bundles deb,rpm

printf 'Linux packages are in src-tauri/target/release/bundle\n'
