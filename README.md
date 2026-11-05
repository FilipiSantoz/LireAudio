<h1 align="center"> Lírë 𝄐 Audio </h1>
<h3 align="center">
<details>
    <summary>Showcase</summary>
<br>

![Showcase](./assets/showcase.webp)

</details>
</h3>

<br>
A real-time audio spectrum analyzer and Dsp effects processor.
Lírë Audio sits directly inside your Linux PipeWire audio graph, functioning as a virtual sink that captures,
enhances, and routes your system audio through the app.

## Features

### Pipewire Pipeline

- https://gitlab.freedesktop.org/pipewire/pipewire-rs

### Ringbuf

- https://github.com/agerasev/ringbuf

### FFT Spectrum Analyzer

- https://github.com/ejmahler/rustfft

### Frontends

- GUI: egui and Tauri
  - https://github.com/emilk/egui
  - https://github.com/tauri-apps/tauri
- TUI:
  I haven't really workerd on the tui frontend, so it's just a visualizer.

### Vinyl Effect

- A simple Vinyl Effect with:
  - Crackle
  - Hiss
  - Room/Rumble
  - Warmth

## Requirements

- Linux Only(temporarily)
- Wireplumber 0.5+
  - To prevent Audio feedback I used wireplumber Smart Filters
    and Smart Filter was implemented on wireplumber version 0.5
    ```bash
    # Run this command to check your version
    wireplumber --version
    ```

## Usage

- Clone this repo

```bash
git clone https://github.com/FilipiSantoz/LireAudio.git
cd LireAudio
```

### Nix only

- If you use direnv

```bash
direnv allow
# or if you not use
nix develop
```

### Other Linux Distro

- Install the dependencies

```bash
sudo apt install build-essential pkg-config libasound2-dev \
libpulse-dev libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev \
libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libayatana-appindicator3-dev \
librsvg2-dev libpipewire-0.3-dev libspa-0.2-dev pipewire-bin libxdo-dev
```

### Run

- Egui frontend

```bash
cargo run --release --bin gui
```

- Tui visualizer

```bash
cargo run --release --bin tui
```

- Tauri

- You will need Nodejs and Bun to run Tauri Frontend

Dev

```bash
./frontend/node_modules/.bin/tauri dev
```

Build

```bash
./frontend/node_modules/.bin/tauri build
# Run the binary created
./target/release/lire-audio
```

## Tests

```bash
cargo test
```
