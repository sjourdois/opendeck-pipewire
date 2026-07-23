# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- CI actions bumped off the deprecated Node 20 runtime.

### Fixed

- Reconnect automatically after a PipeWire daemon restart, instead of going inert.

## [0.2.0]

### Added

- **Stream Deck+ encoder support**: the volume actions render a live percentage
  and a level bar on the touchstrip (the Stream Deck+ `$B1` layout, via
  `setFeedback`), keeping the dial's own icon and title, and every encoder action
  responds to touchstrip taps (`touchTap`). Requires OpenDeck ≥ 2.13.1.
- **App Volume live level bar**: the App Volume key/dial now shows the chosen
  application's volume, aggregated across its streams (works on the touchstrip too).
- **Device cycling**: the Output Device and Input Device actions take an ordered
  list of devices (add / reorder / remove in the property inspector) and each press
  cycles the system default to the next one (a single device keeps the old
  switch-to-it behaviour).
- **Output Device — per-device icons**: assign an icon to each configured output (and
  one for when another device is active); the key shows the active output's icon.
- **Output Device — disable when inactive**: optionally grey out and ignore presses
  when the default isn't one of the chosen sinks, instead of cycling into the list.
  The key also tracks out-of-band default-sink changes live.
- **Editable key titles**: the Output/Input Device and Output/Input Volume keys show
  the current default device, or a custom title from the property inspector (blank
  when cleared).
- **Configurable bar colours**: every volume action's unmuted and muted bar colour
  can be set in the property inspector. Muted keypad keys get a diagonal slash in
  the mute colour; the touchstrip turns its value and bar that colour instead.

### Changed

- Output Device switches the *configured* default (`default.configured.audio.sink`,
  like `wpctl set-default`), so the choice sticks.
- **Unified mute into the volume actions**: each volume action now has an *On press*
  mode — *Volume up*, *Volume down*, or *Toggle mute* — so muting any target (the
  default sink/source, a specific device, or an application) is just a volume action
  set to *Toggle mute*. On an encoder the dial still rotates for volume and presses
  to mute regardless of mode. Adjusting the volume of a muted target unmutes it.
- **Homogenised action names** into parallel Output/Input pairs and grouped them
  in the same order: *Output Volume* / *Input Volume*, *Output Device Volume* /
  *Input Device Volume*, *Output App Volume*, and *Output Device* / *Input Device*
  (the latter was *Switch Input Device*).
- Renamed the plugin to **"PipeWire Audio (Native)"** to avoid a display-name
  clash with the Node.js `com.sfgrimes.pipewire-audio` plugin (same old name).
- Bumped `openaction` to 2.7 (`setFeedback` / `touchTap`) and `pipewire` to 0.10.

### Removed

- The standalone **Mute** action — replaced by the volume actions' *Toggle mute*
  mode. Re-create any placed mute buttons as a volume action set to *Toggle mute*.

## [0.1.1]

### Fixed

- Keys now update live when a device's volume or mute changes outside the plugin
  (wpctl, pavucontrol, media keys, another app). Previously the bar reflected the
  real value only when the key was next used.
- Hardware-mixer devices (USB headsets etc.): out-of-band volume/mute changes are
  now read from the device `Route`, which the node `Props` don't reliably mirror.

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

[Unreleased]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/sjourdois/opendeck-pipewire/releases/tag/v0.1.0
