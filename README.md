# opendeck-pipewire

[![CI](https://github.com/sjourdois/opendeck-pipewire/actions/workflows/ci.yml/badge.svg)](https://github.com/sjourdois/opendeck-pipewire/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)

Native **Rust** PipeWire / WirePlumber audio control plugin for
[OpenDeck](https://github.com/nekename/OpenDeck) — **no Node.js**.

It is a plain Rust binary launched directly by OpenDeck, speaking the OpenAction /
Elgato Stream Deck WebSocket protocol through the
[`openaction`](https://crates.io/crates/openaction) crate, and driving audio
through the native [`pipewire`](https://crates.io/crates/pipewire) bindings.

Inspired by the Node.js plugin
[CronusAK/Pulse-Audio-Stream-Deck-Plugin](https://github.com/CronusAK/Pulse-Audio-Stream-Deck-Plugin),
reimplemented natively in Rust.

> This repository is the **source**. The installable plugin id is
> `fr.jourdois.pipewire.sdPlugin`, and the `.sdPlugin` bundle is a
> build artifact assembled from `assets/` + the compiled binaries.

## Features

| Action | Controllers | What it does |
|--------|-------------|--------------|
| **Output Volume** | Keypad · Encoder | Adjust the default output (sink) volume; live %-bar key image |
| **Output Device Volume** | Keypad · Encoder | Adjust the volume of a specific named output device |
| **Output App Volume** | Keypad · Encoder | Adjust the volume of a specific application (Firefox, Spotify, …) |
| **Output Device** | Keypad · Encoder | Switch the system default output; pick several devices to cycle through them, with a per-device icon, or toggle a subset and grey out when another device is active |
| **Input Volume** | Keypad · Encoder | Adjust the default input (mic/source) volume |
| **Input Device Volume** | Keypad · Encoder | Adjust the volume of a specific named input device |
| **Input Device** | Keypad · Encoder | Switch the system default input; pick several devices to cycle through them |
| **Push to Talk** | Keypad | Hold to talk (unmute the mic while held); optional push-to-mute |

Each **volume action** has an *On press* mode — **Volume up**, **Volume down**, or
**Toggle mute** — so a single action covers raising, lowering, and muting its target
(the default sink/source, a specific device, or an application). There is no separate
mute action: a mute button is just a volume action set to *Toggle mute*. On an
encoder the dial always rotates for volume and presses to mute, regardless of mode.

The **App Volume** key/dial shows a live level bar for the chosen application,
aggregated across its streams.

The Device and Volume keys **title** themselves with the current default device by
default, and take a custom title from the property inspector (blank to clear).

Every volume action's **bar colours** (unmuted and muted) are configurable in the
property inspector. When muted, a keypad key gets a bold diagonal slash in the mute
colour; on a Stream Deck+ touchstrip (which can't draw the slash) the value and bar
turn the mute colour instead.

On a **Stream Deck+**, the volume actions render an icon, a live percentage and a
level bar on the touchstrip (via `setFeedback`), and every encoder action responds
to dial rotation, dial press and touch.

Keys re-render **live** when a device's volume or mute changes outside the plugin
(wpctl, pavucontrol, media keys, another app…), reflected through a PipeWire watch
channel.

Volume changes are applied on the **device Route** for sinks with a hardware
mixer (USB headsets such as the Astro A50) and on the **node Props** for software
sinks — chosen automatically, including across live hardware-profile switches.

> **Linux only.** This plugin talks to PipeWire directly; there is no PulseAudio
> or Windows/macOS backend.

## Requirements

- A running **PipeWire** + **WirePlumber** session.
- **OpenDeck ≥ 2.13.1** — for the Stream Deck+ encoder touchstrip feedback. Earlier
  releases still run the keypad actions, but not the dial feedback.

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

> **Updating an installed plugin:** OpenDeck snapshots each action's definition
> into your profile when you place it, and re-reads it only when the plugin
> **version** changes. After updating the plugin, bump the version (this repo does
> so per release) and **re-create (delete and re-add) any already-placed buttons**
> so they pick up the new icons, states, and settings — a restart alone won't
> refresh existing buttons.

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

## License

GPL-3.0-or-later — see [`LICENSE`](LICENSE). (OpenDeck is GPL-3.0-or-later; the
`openaction` SDK crate is MIT, so this choice is by convention, not obligation.)

## Credits

- Plugin SDK: [`openaction`](https://crates.io/crates/openaction) / the
  [OpenAction API](https://openaction.amankhanna.me/) and
  [OpenDeck](https://github.com/nekename/OpenDeck) by Aman Khanna (nekename).
- Inspiration: [CronusAK/Pulse-Audio-Stream-Deck-Plugin](https://github.com/CronusAK/Pulse-Audio-Stream-Deck-Plugin).
