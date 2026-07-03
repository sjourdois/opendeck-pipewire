//! Surface updates, dispatched by controller type.
//!
//! A **Keypad** button is fully plugin-drawn (an SVG key image, or a 2-state
//! icon). On an **Encoder** (Stream Deck+ dial) the volume actions push the
//! value + level bar of the `$B1` layout via `setFeedback` (see [`crate::render`]);
//! the icon stays the dial's own OpenDeck configuration. The device/app volume
//! actions also push their resolved label as the title (so the dial names its
//! target, as the keypad image does); the default sink/mic actions leave the
//! title to OpenDeck. Mute is a `set_state`, and the device pickers set a default
//! title. See the `encoder-feedback-model` notes.
//!
//! Actions and the out-of-band [`crate::refresh`] task share one path per action.

use openaction::*;

use crate::color::BarColors;
use crate::render;

fn is_encoder(instance: &Instance) -> bool {
	instance.controller == "Encoder"
}

/// Render a volume-style surface: the `$B1` value+bar on an encoder touchstrip, or
/// an SVG key image on a keypad (`keypad` builds it lazily, skipped on encoders).
/// `known == false` shows a "—" placeholder on the keypad.
async fn bar(
	instance: &Instance,
	title: &str,
	known: bool,
	volume_cubic: f32,
	muted: bool,
	colors: &BarColors,
	keypad: impl FnOnce() -> String,
) -> OpenActionResult<()> {
	if is_encoder(instance) {
		// The touchstrip shows the dial's own OpenDeck icon; we push the live
		// value + level bar and the `title` (the resolved surface label — custom
		// label or device name — mirroring what the keypad draws in its image).
		instance
			.set_feedback(&render::bar_feedback(
				title,
				known,
				volume_cubic,
				muted,
				colors,
			))
			.await
	} else if !known {
		instance.set_title(Some("—"), None).await
	} else {
		instance.set_image(Some(keypad()), None).await
	}
}

/// Default-sink / master volume. `known == false` when there is no default sink yet.
pub async fn volume(
	instance: &Instance,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
) -> OpenActionResult<()> {
	bar(instance, "", known, vol, muted, colors, || {
		render::volume_key(vol, muted, colors)
	})
	.await
}

/// Default-source / mic volume (same surface as [`volume`]).
pub async fn mic(
	instance: &Instance,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
) -> OpenActionResult<()> {
	bar(instance, "", known, vol, muted, colors, || {
		render::volume_key(vol, muted, colors)
	})
	.await
}

/// A specific output device's volume. `label` is the resolved surface text — the
/// user's custom label, else the device's friendly name — shown both as the keypad
/// image text and as the encoder title.
pub async fn device(
	instance: &Instance,
	label: &str,
	vol: f32,
	muted: bool,
	colors: &BarColors,
) -> OpenActionResult<()> {
	bar(instance, label, true, vol, muted, colors, || {
		render::label_bar_key(label, vol, muted, colors)
	})
	.await
}

/// A specific input device's volume. `label` is the resolved surface text — the
/// user's custom label, else the device's friendly name — shown both as the keypad
/// image text and as the encoder title.
pub async fn input(
	instance: &Instance,
	label: &str,
	vol: f32,
	muted: bool,
	colors: &BarColors,
) -> OpenActionResult<()> {
	bar(instance, label, true, vol, muted, colors, || {
		render::label_bar_key(label, vol, muted, colors)
	})
	.await
}

/// A device picker (output / switch-input): set the plugin's default title to the
/// selected device name. The user can override the title, and on an encoder the
/// icon is theirs too.
pub async fn picker(instance: &Instance, name: &str) -> OpenActionResult<()> {
	instance.set_title(Some(name), None).await
}

/// The App Volume surface: the chosen application's name with its live level bar
/// (same surface as [`device`]). `None` = no app chosen yet, shown as a
/// placeholder; a chosen app that isn't currently playing shows an empty bar.
pub async fn app(
	instance: &Instance,
	app: Option<&str>,
	vol: f32,
	muted: bool,
	colors: &BarColors,
) -> OpenActionResult<()> {
	let label = app.unwrap_or("App");
	bar(instance, label, app.is_some(), vol, muted, colors, || {
		render::label_bar_key(label, vol, muted, colors)
	})
	.await
}
