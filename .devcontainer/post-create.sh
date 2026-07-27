#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

pnpm install --frozen-lockfile
cargo fetch --manifest-path src-tauri/Cargo.toml --locked
cargo fetch --manifest-path src-tauri/admin-helper/Cargo.toml --locked

printf '\nLiScan development container is ready.\n'
printf 'Run "pnpm check" to verify the project.\n'
printf 'Run "pnpm dev:container" and open port 6080 to use the desktop UI.\n'
