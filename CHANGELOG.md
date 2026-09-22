# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Unavailable streams show "n/a" on a coloured bar**: when a volume target
  isn't available (no default device, a configured device that's gone, an app
  that isn't playing), the key and the encoder touchstrip show "n/a" with a
  full bar in a configurable colour (yellow by default) instead of the "—"
  placeholder / empty bar.
- **Custom icons for all volume actions**: pick an image in the property
  inspector; it is drawn at the top of the keypad key and sent as the encoder
  touchstrip icon. The picker fits it on a transparent 128×128 square, so
  nothing is stretched and the profile doesn't carry a full-resolution photo.
- **Limit to 100%**: an optional checkbox caps volume increases through the
  action at 100% instead of the +50% boost range.
- **Custom label for App Volume**: like the device volume actions, the app key
  can show a custom label instead of the application name.
- **Larger dial titles**: the volume actions switch their encoder to a shipped
  `$B1`-based layout with a larger title font at runtime (no re-adding of
  buttons needed).
- **Recognizable dial previews**: the OpenDeck window shows the label — and the
  custom icon, once one is set — for encoder instances, which the touchstrip
  feedback values it doesn't render never gave it. Setting a dial's icon hands
  its image to the plugin for good, as it does for the Output Device key, so
  the property inspector says so and the plugin never pushes an empty one.

### Changed

- **A dial's title is now the plugin's.** The preview writes the resolved label
  into the dial's state, which is the same field OpenDeck's dial editor writes
  to, and it is rewritten on every redraw — so 0.2.0's "keeping the dial's own
  icon and title" no longer holds for the title. The property inspector's
  Title / Label field decides it. The icon is untouched unless you set one
  there.
- The volume actions' icons are squared in the property inspector at pick time,
  on a 128×128 canvas, rather than in the plugin: the `image` crate and its 18
  transitive crates are gone, and an icon no longer crosses the websocket at
  full resolution on every volume tick.
- The property inspectors' dropdowns and radio buttons show their indicators
  again: the vendored stylesheet had pointed at `caret.svg` and `rcheck.svg`
  since the initial commit, and neither file was ever in the repo. A dropdown
  set `appearance: none` and then had nothing to draw its arrow with, and a
  selected radio was a blue square with no dot.
- The released bundle is assembled from all of `assets/`, the way a dev build
  already was, instead of naming each directory — which had left the dial
  layout out of it.

## [0.3.1]

### Fixed

- **Output Device: stop overwriting the key image you chose in OpenDeck**
  ([#2](https://github.com/sjourdois/opendeck-pipewire/issues/2)). OpenDeck keeps a
  plugin-pushed image in the very slot its own image picker writes to, so the key
  pushing its built-in icon on every redraw silently undid the picture you had just
  set. The key now leaves its image alone unless the property inspector asks for an
  icon only the plugin can pick — a per-device icon, an icon for when the key is
  inactive, or the greyed-out look of *Disable*.

## [0.3.0]

### Added

- **Output Device — route a stream**: set a stream's `node.name` in the property
  inspector and the key switches where that stream sends instead of the system
  default output; the apps feeding it are never moved, so nothing is paused.
  Targets can then be added by name, even when hidden from applications.
- Every command the backend receives is logged, with its parameters, so an
  unexpected mute or volume change can be traced back to the key or dial.

### Changed

- CI actions bumped off the deprecated Node 20 runtime.
- `base64` upgraded to 0.23.

### Fixed

- Reconnect automatically after a PipeWire daemon restart, instead of going inert.
- An `ENOENT` reported on the core (binding a node whose permission was just
  revoked) is no longer taken for a lost connection, which looped reconnecting.

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

[Unreleased]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/sjourdois/opendeck-pipewire/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/sjourdois/opendeck-pipewire/releases/tag/v0.1.0
