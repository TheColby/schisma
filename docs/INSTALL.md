# Installation and packaging

## Homebrew on macOS

This repository doubles as an explicit-URL Homebrew tap. Because the project
does not yet have a tagged source release, `Formula/schisma.rb` is head-only:

```sh
brew tap TheColby/schisma https://github.com/TheColby/schisma
brew install --HEAD TheColby/schisma/schisma
```

The formula fetches locked Cargo dependencies in Homebrew's network-enabled
fetch phase, builds offline, installs `Schisma.app` under the formula prefix,
and exposes these commands:

- `schisma` — standalone GUI;
- `schisma-render` — deterministic float-WAV renderer;
- `schisma-live` — terminal live host;
- `schisma-gpu-info` — Metal/CUDA/CPU discovery and compute self-test.

Useful maintenance commands:

```sh
brew test TheColby/schisma/schisma
brew upgrade --fetch-HEAD TheColby/schisma/schisma
brew uninstall schisma
```

## Debian and Ubuntu

The [Packages GitHub Actions workflow](https://github.com/TheColby/schisma/actions/workflows/packages.yml)
publishes a `schisma-debian` artifact on every push to `main`. After extracting
the artifact, APT can install the local package and resolve its declared runtime
libraries:

```sh
sudo apt-get update
sudo apt-get install ./schisma_0.1.0_amd64.deb
```

The installed package contains the four commands listed above, a freedesktop
application entry, the scalable Schisma icon, the README, and the MIT license.

### Build a `.deb` locally

Install Rust 1.92 or newer and the native development libraries:

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config dpkg-dev \
  libasound2-dev libudev-dev libx11-dev libxi-dev libgl1-mesa-dev \
  libwayland-dev libxkbcommon-dev libvulkan-dev
```

Build and install the package:

```sh
./scripts/package_debian.sh
sudo apt-get install ./dist/schisma_0.1.0_$(dpkg --print-architecture).deb
```

`SCHISMA_DIST_DIR`, `SCHISMA_VERSION`, and `SCHISMA_DEB_ARCH` may be set when a
packaging environment needs explicit output, version, or architecture values.

## macOS app bundle

The repository can also produce an unsigned local app bundle without Homebrew:

```sh
./scripts/package_macos.sh
open dist/Schisma.app
```

Signing and notarization are separate release operations and are not performed
by the local packaging scripts.
