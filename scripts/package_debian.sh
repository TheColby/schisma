#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="${SCHISMA_DIST_DIR:-${repo_root}/dist}"

for command in awk cargo dpkg dpkg-deb du head install mkdir mktemp rm sed; do
    if ! command -v "${command}" >/dev/null 2>&1; then
        echo "Required command not found: ${command}" >&2
        exit 1
    fi
done

version="${SCHISMA_VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' "${repo_root}/Cargo.toml" | head -n 1)}"
architecture="${SCHISMA_DEB_ARCH:-$(dpkg --print-architecture)}"
if [[ -z "${version}" || -z "${architecture}" ]]; then
    echo "Could not determine package version or architecture" >&2
    exit 1
fi

package_root="$(mktemp -d "${TMPDIR:-/tmp}/schisma-deb.XXXXXX")"
cleanup() {
    case "${package_root}" in
        */schisma-deb.*) rm -rf "${package_root}" ;;
        *) echo "Refusing to remove unexpected temporary path: ${package_root}" >&2 ;;
    esac
}
trap cleanup EXIT

cargo build --release --locked \
    --manifest-path "${repo_root}/Cargo.toml" \
    -p schisma-app -p schisma-engine -p schisma-gpu

install -Dm755 "${repo_root}/target/release/schisma" "${package_root}/usr/bin/schisma"
install -Dm755 "${repo_root}/target/release/schisma-render" "${package_root}/usr/bin/schisma-render"
install -Dm755 "${repo_root}/target/release/schisma-live" "${package_root}/usr/bin/schisma-live"
install -Dm755 "${repo_root}/target/release/schisma-gpu-info" "${package_root}/usr/bin/schisma-gpu-info"
install -Dm644 "${repo_root}/packaging/linux/org.schisma.synth.desktop" \
    "${package_root}/usr/share/applications/org.schisma.synth.desktop"
install -Dm644 "${repo_root}/packaging/shared/org.schisma.synth.svg" \
    "${package_root}/usr/share/icons/hicolor/scalable/apps/org.schisma.synth.svg"
install -Dm644 "${repo_root}/README.md" "${package_root}/usr/share/doc/schisma/README.md"
install -Dm644 "${repo_root}/LICENSE" "${package_root}/usr/share/doc/schisma/copyright"

installed_size="$(du -sk "${package_root}/usr" | awk '{print $1}')"
mkdir -p "${package_root}/DEBIAN" "${dist_dir}"
sed \
    -e "s/@VERSION@/${version}/g" \
    -e "s/@ARCHITECTURE@/${architecture}/g" \
    -e "s/@INSTALLED_SIZE@/${installed_size}/g" \
    "${repo_root}/packaging/debian/control.in" > "${package_root}/DEBIAN/control"

output="${dist_dir}/schisma_${version}_${architecture}.deb"
dpkg-deb --root-owner-group --build "${package_root}" "${output}"
echo "Packaged ${output}"
