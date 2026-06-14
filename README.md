# opendeck-pipewire

[![CI](https://github.com/sjourdois/opendeck-pipewire/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/opendeck-pipewire/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)

Native **Rust** PipeWire / WirePlumber audio control plugin for
[OpenDeck](https://github.com/nekename/OpenDeck) — **no Node.js, no Wine**.

It is a plain Rust binary launched directly by OpenDeck, speaking the OpenAction /
Elgato Stream Deck WebSocket protocol through the
[`openaction`](https://crates.io/crates/openaction) crate, and driving audio
through the native [`pipewire`](https://crates.io/crates/pipewire) bindings.

Inspired by the Node.js plugin
[CronusAK/Pulse-Audio-Stream-Deck-Plugin](https://github.com/CronusAK/Pulse-Audio-Stream-Deck-Plugin),
reimplemented natively in Rust.

> This repository is the **source**. The installable plugin id is
> `fr.jourdois.pipewire.sdPlugin` (reverse-DNS, the Stream Deck convention) — the
> repo name and the plugin id are independent, and the `.sdPlugin` bundle is a
> build artifact assembled from `assets/` + the compiled binaries.

## Features

| Action | Controllers | What it does |
|--------|-------------|--------------|
| **Volume** | Keypad · Encoder | Adjust the default output (sink) volume; dial press = mute; live %-bar key image |
| **Mute** | Keypad · Encoder | Toggle the default output mute (2-state icon) |
| **Output Device** | Keypad · Encoder | Switch the system default output device |
| **App Volume** | Keypad · Encoder | Adjust the volume of a specific application (Firefox, Spotify, …) |
| **Device Volume** | Keypad · Encoder | Adjust the volume of a specific named output device |

Volume changes are applied on the **device Route** for sinks with a hardware
mixer (USB headsets such as the Astro A50) and on the **node Props** for software
sinks — chosen automatically, including across live hardware-profile switches.

> **Linux only.** This plugin talks to PipeWire directly; there is no PulseAudio
> or Windows/macOS backend.

## Requirements

- A running **PipeWire** + **WirePlumber** session.
- **OpenDeck** ≥ the version that supports native plugins.

## Install

**Manual install** (until published to the OpenDeck plugin store):

1. Build the binary (see below).
2. Assemble the plugin directory `fr.jourdois.pipewire.sdPlugin/`:
   ```
   fr.jourdois.pipewire.sdPlugin/
   ├── manifest.json            (from assets/)
   ├── icons/                   (from assets/)
   ├── propertyInspector/       (from assets/)
   └── <target-triple>/bin/opendeck-pipewire
   ```
   e.g. the binary at `x86_64-unknown-linux-gnu/bin/opendeck-pipewire`.
3. Copy that directory into `~/.config/opendeck/plugins/` and restart OpenDeck.

## Build from source

The `pipewire` crate generates bindings with `bindgen`, so it needs the system
headers **and** libclang. On Debian/Ubuntu:

```bash
sudo apt install libpipewire-0.3-dev libspa-0.2-dev clang libclang-dev pkg-config
```

Then:

```bash
# Quick dev/release build for the host target:
cargo build --release      # -> target/release/opendeck-pipewire

# Multi-target packaging (mirrors OpenDeck's built-in plugins) — needs deno:
deno run -A build.ts <outDir> x86_64-unknown-linux-gnu
```

CI builds the release `.sdPlugin` bundle (x86_64 + aarch64 Linux) automatically —
see `.github/workflows/release.yml`. Pushing a `v*` tag attaches the zipped bundle
to the GitHub release; `.github/workflows/ci.yml` runs fmt + clippy + build on every
push.

Runtime PipeWire tools (`wpctl`, `pw-dump`, `pactl`) are **not** used by the
plugin — audio control is fully native — but are handy for debugging.

## How it works

- `src/pw.rs` runs a dedicated PipeWire main-loop thread (the loop is not `Send`),
  tracking sink nodes, application streams, the `default` metadata, and audio
  `Device` objects with their routes. Actions talk to it through a
  `pipewire::channel` (commands) and shared state (live volumes / device lists).
- Volume is exposed on the cubic/perceptual scale (like `wpctl`); the mapping to
  PipeWire's linear `channelVolumes` is `linear = cubic³`.
- Live key images (volume bars, app labels) are generated as SVG data URIs and
  sent via `set_image`.

## Parity with the plugin it replaces

`com.sfgrimes.pipewire-audio` (the Node.js plugin) exposes 8 actions. **Full
functional parity is reached** (plus a dedicated Mute action as a bonus):

| Original action | Status here |
|-----------------|-------------|
| Master Volume | ✅ Volume |
| App Volume | ✅ App Volume |
| Output Volume | ✅ Device Volume |
| Switch Output Device | ✅ Output Device |
| Mic Volume | ✅ Mic Volume |
| Input Volume | ✅ Input Volume |
| Switch Input Device | ✅ Switch Input Device |
| Push to Talk | ✅ Push to Talk |
| — | ➕ Mute (default sink) |

## Roadmap to a publishable 1.0

- [x] **Source/input actions** for functional parity (Mic Volume, Input Volume,
      Switch Input Device, Push to Talk).
- [x] **`LICENSE`** (GPL-3.0-or-later).
- [x] **CI / packaging** workflow producing the `.sdPlugin` bundle for `x86_64` and
      `aarch64` Linux.
- [ ] Designed **icons** (current ones are simple built-in SVGs).
- [ ] Live volume bar on the **App Volume** key (per-stream volume read-back).
- [ ] **Distribution**: submit to the OpenDeck plugin store / OpenAction plugins
      registry, or host a release for manual install.
- [ ] Testing across more hardware (different cards, multi-channel routes).

## License

GPL-3.0-or-later — see [`LICENSE`](LICENSE). (OpenDeck is GPL-3.0-or-later; the
`openaction` SDK crate is MIT, so this choice is by convention, not obligation.)

## Credits

- Plugin SDK: [`openaction`](https://crates.io/crates/openaction) / the
  [OpenAction API](https://openaction.amankhanna.me/) and
  [OpenDeck](https://github.com/nekename/OpenDeck) by Aman Khanna (nekename).
- Inspiration: [CronusAK/Pulse-Audio-Stream-Deck-Plugin](https://github.com/CronusAK/Pulse-Audio-Stream-Deck-Plugin).
