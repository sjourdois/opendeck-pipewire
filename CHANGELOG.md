# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

Initial release — a native Rust PipeWire/WirePlumber audio control plugin for
OpenDeck, with full functional parity with the Node.js plugin it replaces (plus a
dedicated Mute action), and **no Node.js or Wine** at runtime.

### Added

- **Output actions**: Volume, Mute, Output Device, App Volume, Device Volume.
- **Input actions**: Mic Volume, Input Volume, Switch Input Device, Push to Talk.
- Native volume/mute control via the `pipewire` crate: node `Props` for software
  sinks, and the owning device's active `Route` for hardware-mixer devices (USB
  headsets etc.), chosen automatically — including across live hardware-profile
  switches (e.g. Astro A50 game↔chat).
- Default sink/source switching via the `default` PipeWire metadata.
- Live key images (volume bars, device/app labels) rendered as SVG.
- GPL-3.0-or-later license, CI (fmt + clippy + build) and a release workflow that
  bundles the `.sdPlugin` for x86_64 and aarch64 Linux.

[Unreleased]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sjourdois/opendeck-pipewire/releases/tag/v0.1.0
