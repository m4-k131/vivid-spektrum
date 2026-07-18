# Building vividspektrum

## Toolchain

Install [Rust via rustup](https://rustup.rs/) (stable is fine). You need a C toolchain and `pkg-config` so crates that link system libraries can run their build scripts.

**Fedora example:**

```bash
sudo dnf install gcc gcc-c++ make cmake pkgconf-pkg-config
```

**Arch Linux example:**

```bash
sudo pacman -S --needed base-devel pkgconf
```

## System libraries

The **`pipewire`** Rust crate links against **libpipewire** using `pkg-config`. Install the development files for PipeWire so `libpipewire-0.3.pc` is available (paths vary by distro).

**Fedora:**

```bash
sudo dnf install pipewire-devel
```

**Arch Linux:**

```bash
sudo pacman -S pipewire
```

If the build still cannot find `libpipewire-0.3.pc`, install the package that owns that file on your distribution, or set `PKG_CONFIG_PATH` to the directory containing the `.pc` file (only needed in unusual prefixes or containers).

### Clang (`bindgen`)

Some transitive dependencies use **[bindgen](https://github.com/rust-lang/rust-bindgen)**, which loads **`libclang`** at build time. Without it you get errors like “Unable to find libclang”.

**Fedora:**

```bash
sudo dnf install clang-devel
```

**Arch Linux:**

```bash
sudo pacman -S clang
```

If `bindgen` still cannot find the library, set **`LIBCLANG_PATH`** to the directory containing `libclang.so` (Fedora often places it under `/usr/lib64/llvm*`).

Graphics: the UI uses **`wgpu`**. Ensure you have a working **Vulkan** (or appropriate) stack for your GPU (e.g. Mesa on typical Linux desktops).

## Build

From the repository root:

```bash
cargo build
```

Release binary:

```bash
cargo build --release
```

The `vividspektrum` binary is under `target/release/vividspektrum` (or `target/debug/vividspektrum`).

With **`cargo run`**, pass binary flags **after `--`** so Cargo does not consume them. The package default binary is **`vividspektrum`** (use **`--bin sine_preview`** for the tone preview):

```bash
cargo run -p spektrum -- --fft 32768 --hop 16384
```

### Sine preview (no PipeWire, no Hyprland config)

To tune the spectrogram shader and DSP with a synthetic tone, run:

```bash
cargo run -p spektrum --bin sine_preview
```

Optional flags match the main app where relevant, e.g. `--freq-hz 880 --width 900 --height 240`. No changes to `hyprland.conf` are required.

## Optional checks

Format:

```bash
cargo fmt
```

Lint:

```bash
cargo clippy --workspace --all-targets
```

## Platform note

Only **Linux** is supported for the full application. `spektrum-core` may compile parts of the tree on other targets, but the `vividspektrum` binary expects Linux, Wayland, and PipeWire at runtime.
