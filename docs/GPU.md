# GPU backends

Schisma v0.1 contains real Metal and CUDA compute backends for non-realtime
audio batches. Both run the same `f32` conditioning kernel and are checked
against the CPU reference implementation. The desktop app uses the selected
backend in its analysis worker. GPU work is never awaited by the audio callback.

## Runtime selection

`Auto` prefers Metal on macOS, CUDA on other platforms, then the CPU fallback.
Explicit Metal or CUDA requests also fall back to CPU when the requested API,
driver, compiler, or device is unavailable. The GUI and `schisma-gpu-info`
display both the requested and active backend plus the fallback reason.

```sh
cargo run -p schisma-gpu --bin schisma-gpu-info
```

## Build features

The default build enables Metal and the dynamically loaded CUDA 12 interface:

```sh
cargo build -p schisma-app
```

Build one CUDA generation explicitly with default features disabled:

```sh
cargo build -p schisma-gpu --no-default-features --features cuda,cuda-11
cargo build -p schisma-gpu --no-default-features --features cuda,cuda-12
cargo build -p schisma-gpu --no-default-features --features cuda,cuda-13
```

Metal-only and CPU-only builds are also supported:

```sh
cargo build -p schisma-gpu --no-default-features --features metal
cargo build -p schisma-gpu --no-default-features
```

## Platform requirements

- Metal requires macOS and a Metal-capable GPU. Schisma requests the native
  Metal adapter through `wgpu` and compiles its WGSL compute shader at runtime.
- CUDA requires an NVIDIA GPU, compatible driver, and NVRTC runtime. The CUDA
  driver and compiler libraries are loaded dynamically, so building Schisma
  does not require a CUDA toolkit on the build machine.
- NVIDIA does not provide a current CUDA runtime on macOS. CUDA requests on a
  Mac therefore report the limitation and use CPU; this is expected behavior.

CUDA 11/12/13 feature builds are exercised independently in CI. Hardware
execution still depends on the host providing a compatible NVIDIA runtime.
