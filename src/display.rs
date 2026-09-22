//! Surface updates, dispatched by controller type.
//!
//! A **Keypad** button is plugin-drawn (an SVG key image, or a 2-state icon) —
//! except the Output Device key, whose image is OpenDeck's own until the settings
//! ask for an icon the plugin has to draw (see [`crate::actions::output`]), since
//! a pushed image overwrites the user's for good.
//! On an **Encoder** (Stream Deck+ dial) the volume actions push the
//! value + level bar of the `$B1` layout via `setFeedback` (see [`crate::render`]);
//! a user-configured icon is sent as the touchstrip `icon`, otherwise the dial
//! keeps its own OpenDeck icon. The resolved label is mirrored into the dial's
//! state so the OpenDeck window shows a recognizable preview, together with the
//! icon when one is configured — and only then, since clearing a dial's image
//! destroys OpenDeck's own as surely as overwriting it. Mute is a `set_state`,
//! and the device pickers set a default title. See the `encoder-feedback-model`
//! notes.
//!
//! Actions and the out-of-band [`crate::refresh`] task share one path per action.

use openaction::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::color::BarColors;
use crate::render;
use crate::ui::VolumeUi;

fn is_encoder(instance: &Instance) -> bool {
	instance.controller == "Encoder"
}

/// The `$B1`-based touchstrip layout with a larger title font, shipped in
/// `assets/layouts/`. Applied at runtime (the manifest keeps `$B1` as a
/// fallback for older OpenDeck releases), so existing dials pick it up without
/// being re-added.
const VOLUME_LAYOUT: &str = "layouts/volume.json";

/// Switch an encoder to the volume layout. No-op on keypads. Call on
/// `will_appear`; the layout persists for the instance afterwards.
pub async fn encoder_layout(instance: &Instance) -> OpenActionResult<()> {
	if is_encoder(instance) {
		instance
			.set_feedback_layout(VOLUME_LAYOUT.to_owned())
			.await?;
	}
	Ok(())
}

/// Everything [`bar`] needs to draw a volume-style surface.
struct BarSurface<'a> {
	title: &'a str,
	known: bool,
	volume_cubic: f32,
	muted: bool,
	colors: &'a BarColors,
	ui: &'a VolumeUi,
}

/// The dial preview pushed into an encoder's state: the icon (when the settings
/// provide one) and the label.
#[derive(PartialEq)]
struct Preview {
	icon: Option<String>,
	title: String,
}

/// Last [`Preview`] pushed per instance, so an unchanged one isn't re-sent:
/// every resend would refresh the UI and mark the profile stale in OpenDeck.
/// Dropped by [`forget_preview`] when the instance goes away.
static PREVIEW: LazyLock<Mutex<HashMap<String, Preview>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

/// Drop an instance's remembered dial preview, on `will_disappear`.
pub fn forget_preview(instance_id: &str) {
	PREVIEW.lock().unwrap().remove(instance_id);
}

/// Render a volume-style surface: the `$B1` value+bar on an encoder touchstrip, or
/// an SVG key image on a keypad (`keypad` builds it lazily, skipped on encoders).
/// `known == false` shows a yellow "n/a" over a yellow bar (the stream exists but
/// isn't currently available).
async fn bar(
	instance: &Instance,
	surface: &BarSurface<'_>,
	keypad: impl FnOnce() -> String,
) -> OpenActionResult<()> {
	if is_encoder(instance) {
		// The touchstrip shows the value + level bar and the `title` (the resolved
		// surface label — custom label or device name — mirroring what the keypad
		// draws in its image); a configured icon overrides the dial icon.
		instance
			.set_feedback(&render::bar_feedback(
				surface.title,
				surface.known,
				surface.volume_cubic,
				surface.muted,
				surface.colors,
				surface.ui.icon(),
			))
			.await?;
		// The OpenDeck window preview doesn't render feedback values, so mirror
		// the label — and an icon, if the settings provide one — into the dial's
		// state for a recognizable preview. The state text doubles as the strip
		// title (OpenDeck prefers it over the feedback title), so it stays a
		// single clean label: the value already has its own touchstrip slot, and
		// a newline would render as tofu there (Noto has no U+000A glyph).
		//
		// The image is only ever *set*, never cleared: `set_image(None)` resets
		// the dial to the action's default and takes OpenDeck's own picture with
		// it, for good (see [`crate::actions::output`]).
		let preview = Preview {
			icon: surface.ui.icon().map(str::to_owned),
			title: surface.title.to_owned(),
		};
		let changed = PREVIEW
			.lock()
			.unwrap()
			.get(&instance.instance_id)
			.is_none_or(|last| *last != preview);
		if changed {
			if let Some(icon) = preview.icon.as_deref() {
				instance.set_image(Some(icon), None).await?;
			}
			instance
				.set_title(Some(preview.title.clone()), None)
				.await?;
			PREVIEW
				.lock()
				.unwrap()
				.insert(instance.instance_id.clone(), preview);
		}
		Ok(())
	} else if !surface.known {
		instance
			.set_image(
				Some(render::unavailable_key(surface.colors, surface.ui)),
				None,
			)
			.await
	} else {
		instance.set_image(Some(keypad()), None).await
	}
}

/// Default-sink / master volume. `label` is the resolved surface text — the
/// user's custom title, else the current default output's name — shown as the
/// encoder title and preview. `known == false` when there is no default sink yet.
pub async fn volume(
	instance: &Instance,
	label: &str,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> OpenActionResult<()> {
	let surface = BarSurface {
		title: label,
		known,
		volume_cubic: vol,
		muted,
		colors,
		ui,
	};
	bar(instance, &surface, || {
		render::volume_key(vol, muted, colors, ui)
	})
	.await
}

/// Default-source / mic volume (same surface as [`volume`]).
pub async fn mic(
	instance: &Instance,
	label: &str,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> OpenActionResult<()> {
	let surface = BarSurface {
		title: label,
		known,
		volume_cubic: vol,
		muted,
		colors,
		ui,
	};
	bar(instance, &surface, || {
		render::volume_key(vol, muted, colors, ui)
	})
	.await
}

/// A specific output device's volume. `label` is the resolved surface text — the
/// user's custom label, else the device's friendly name — shown both as the keypad
/// image text and as the encoder title. `known == false` when no device is
/// configured or it isn't currently available.
pub async fn device(
	instance: &Instance,
	label: &str,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> OpenActionResult<()> {
	let surface = BarSurface {
		title: label,
		known,
		volume_cubic: vol,
		muted,
		colors,
		ui,
	};
	bar(instance, &surface, || {
		render::label_bar_key(label, vol, muted, colors, ui)
	})
	.await
}

/// A specific input device's volume. `label` is the resolved surface text — the
/// user's custom label, else the device's friendly name — shown both as the keypad
/// image text and as the encoder title. `known == false` when no input is
/// configured or it isn't currently available.
pub async fn input(
	instance: &Instance,
	label: &str,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> OpenActionResult<()> {
	let surface = BarSurface {
		title: label,
		known,
		volume_cubic: vol,
		muted,
		colors,
		ui,
	};
	bar(instance, &surface, || {
		render::label_bar_key(label, vol, muted, colors, ui)
	})
	.await
}

/// Set the plugin title, or clear the override when empty so the key falls back to
/// the user's own (usually blank) title rather than OpenDeck's default text.
async fn put_title(instance: &Instance, text: &str) -> OpenActionResult<()> {
	if text.is_empty() {
		instance.set_title(None::<String>, None).await
	} else {
		instance.set_title(Some(text), None).await
	}
}

/// A device picker (switch-input): set the plugin's default title to the selected
/// device name. The user can override the title, and on an encoder the icon is
/// theirs too.
pub async fn picker(instance: &Instance, name: &str) -> OpenActionResult<()> {
	put_title(instance, name).await
}

/// Output toggle surface: `title` (the current default output, or the user's
/// custom title), plus `image` — the icon resolved from the settings, or `None`
/// when the plugin doesn't draw this key (see [`crate::actions::output::Surface`]),
/// in which case the image OpenDeck holds for it is left untouched.
///
/// The key stays a single state so OpenDeck keeps the user's title font/position;
/// the greyed look is a swapped image rather than a second state. On an encoder the
/// title is the only surface (the dial keeps its own icon).
pub async fn output(instance: &Instance, title: &str, image: Option<&str>) -> OpenActionResult<()> {
	put_title(instance, title).await?;
	let Some(image) = image else { return Ok(()) };
	if is_encoder(instance) {
		return Ok(());
	}
	instance.set_image(Some(image), None).await
}

/// Set just the key title: a plain title on a keypad, or the `$B1` feedback title
/// on an encoder (merged, so it leaves the value/bar untouched). Used by the
/// default volume/mic actions to show their editable "Output"/"Input" label.
pub async fn title(instance: &Instance, text: &str) -> OpenActionResult<()> {
	if is_encoder(instance) {
		instance.set_feedback(&json!({ "title": text })).await
	} else {
		put_title(instance, text).await
	}
}

/// The App Volume surface: the resolved label (custom label, else the
/// application's name) with its live level bar (same surface as [`device`]).
/// No app chosen, or a chosen app that isn't currently playing, shows "n/a"
/// with a yellow bar.
pub async fn app(
	instance: &Instance,
	label: &str,
	known: bool,
	vol: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> OpenActionResult<()> {
	let surface = BarSurface {
		title: label,
		known,
		volume_cubic: vol,
		muted,
		colors,
		ui,
	};
	bar(instance, &surface, || {
		render::label_bar_key(label, vol, muted, colors, ui)
	})
	.await
}

#[cfg(test)]
mod tests {
	/// The shipped dial layout must stay `$B1`-shaped (same item keys, so the
	/// `setFeedback` payloads keep working) with a larger title that doesn't
	/// overlap the rows below it.
	#[test]
	fn volume_layout_matches_b1_shape_with_bigger_title() {
		let raw = include_str!("../assets/layouts/volume.json");
		let v: serde_json::Value = serde_json::from_str(raw).unwrap();
		let items = v["items"].as_array().unwrap();
		let keys: Vec<&str> = items.iter().map(|i| i["key"].as_str().unwrap()).collect();
		assert_eq!(keys, ["title", "icon", "value", "indicator"]);
		let title = &items[0];
		assert_eq!(title["font"]["size"], serde_json::json!(24));
		let rect = title["rect"].as_array().unwrap();
		let (x, y, w, h) = (
			rect[0].as_u64().unwrap(),
			rect[1].as_u64().unwrap(),
			rect[2].as_u64().unwrap(),
			rect[3].as_u64().unwrap(),
		);
		assert!(x + w <= 200 && y + h <= 100);
		assert!(y + h <= 40); // icon/value rows start at y=40
	}
}
