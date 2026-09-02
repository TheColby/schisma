#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bundle_dir="${1:-${repo_root}/dist/Schisma.app}"
contents_dir="${bundle_dir}/Contents"

cargo build --release -p schisma-app --bin schisma --manifest-path "${repo_root}/Cargo.toml"
mkdir -p "${contents_dir}/MacOS" "${contents_dir}/Resources"
cp "${repo_root}/target/release/schisma" "${contents_dir}/MacOS/schisma"
cp "${repo_root}/packaging/macos/Info.plist" "${contents_dir}/Info.plist"
chmod +x "${contents_dir}/MacOS/schisma"

echo "Packaged ${bundle_dir}"
