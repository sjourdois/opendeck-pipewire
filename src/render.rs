//! On-the-fly visual feedback.
//!
//! Keypad buttons get a full 128×128 SVG key image via `set_image`. Encoders
//! (Stream Deck+ dials) use the native `$B1` touchstrip layout, for which this
//! module builds the `setFeedback` value+bar payload (see [`bar_feedback`]); the
//! title stays the dial's own OpenDeck configuration unless the action resolves
//! a label, and a user-configured icon is sent as the touchstrip `icon`.

use std::collections::HashMap;
use std::io::Cursor;

use base64::Engine;
use serde_json::{Value, json};

use crate::color::BarColors;
use crate::ui::VolumeUi;

/// Build a `data:image/svg+xml;base64,...` URI suitable for `Instance::set_image`.
fn data_uri(svg: &str) -> String {
	let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
	format!("data:image/svg+xml;base64,{b64}")
}

/// The Output Device key icon (normal) as a data URI. Set explicitly (rather than
/// via a manifest state) so the picker can stay a single state — switching states
/// would reset the user's per-state title font/position.
pub fn output_icon() -> String {
	data_uri(include_str!("../assets/icons/output.svg"))
}

/// The greyed Output Device icon, shown when the key is inactive (the default
/// output isn't one of the chosen sinks and `when_inactive == Disable`).
pub fn output_disabled_icon() -> String {
	data_uri(include_str!("../assets/icons/outputDisabled.svg"))
}

/// A bold slash drawn corner-to-corner across the whole key to signal mute, in
/// the configured mute colour. Overlaid last (on top of the label/bar) by the
/// keypad renderers when muted; empty when unmuted.
fn mute_slash(muted: bool, color: &str) -> String {
	if muted {
		format!(
			r##"<line x1="18" y1="110" x2="110" y2="18" stroke="{color}" stroke-width="12" stroke-linecap="round"/>"##
		)
	} else {
		String::new()
	}
}

/// The user's custom icon (a data URI from the property inspector), drawn small
/// at the top of the key image. Empty when no icon is configured. Both `href`
/// (SVG2) and `xlink:href` (SVG1.1) are set so older renderers pick it up too;
/// `preserveAspectRatio` keeps non-square images undistorted (letterboxed).
fn icon_overlay(ui: &VolumeUi) -> String {
	match ui.icon() {
		Some(uri) => format!(
			r##"<image x="48" y="4" width="32" height="32" preserveAspectRatio="xMidYMid meet" href="{uri}" xlink:href="{uri}"/>"##,
			uri = escape(uri)
		),
		None => String::new(),
	}
}

/// The main text's vertical baseline, shifted down when the icon occupies the top.
fn text_y(with_icon: bool, plain: i32) -> i32 {
	if with_icon { plain + 10 } else { plain }
}

/// A volume key: a big percentage with a horizontal level bar underneath.
/// `volume_cubic` is on the 0..=1 (perceptual) scale; values above 1 are boost.
pub fn volume_key(volume_cubic: f32, muted: bool, colors: &BarColors, ui: &VolumeUi) -> String {
	let pct = (volume_cubic * 100.0).round() as i32;
	let fill_w = (volume_cubic.clamp(0.0, 1.0) * 100.0).round() as i32;
	let with_icon = ui.icon().is_some();
	let y = text_y(with_icon, 66);
	let (bar, label, label_color, size) = if muted {
		(colors.mute(), "muted".to_owned(), "#999999", 20)
	} else if volume_cubic > 1.001 {
		("#d8843d", format!("{pct}%"), "#ffffff", 20) // boosted (fixed warning colour)
	} else {
		(colors.active(), format!("{pct}%"), "#ffffff", 20)
	};

	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128">
{icon}<text x="64" y="{y}" font-family="sans-serif" font-size="{size}" font-weight="bold" fill="{label_color}" text-anchor="middle">{label}</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="{fill_w}" height="12" rx="6" fill="{bar}"/>
{slash}</svg>"##,
		icon = icon_overlay(ui),
		slash = mute_slash(muted, colors.mute())
	);
	data_uri(&svg)
}

/// A key shown when the audio stream is unavailable (no default device, a
/// configured device that's gone, an app that isn't playing…): "n/a" over a
/// full-width bar in the configured n/a colour (yellow by default).
pub fn unavailable_key(colors: &BarColors, ui: &VolumeUi) -> String {
	let with_icon = ui.icon().is_some();
	let y = text_y(with_icon, 66);
	let na = colors.unavailable();
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128">
{icon}<text x="64" y="{y}" font-family="sans-serif" font-size="20" font-weight="bold" fill="#ffffff" text-anchor="middle">n/a</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="100" height="12" rx="6" fill="{na}"/>
</svg>"##,
		icon = icon_overlay(ui),
	);
	data_uri(&svg)
}

/// A labelled level-bar key/encoder: a custom label with a level bar (no
/// percentage), shared by the device / input / app volume actions.
pub fn label_bar_key(
	label: &str,
	volume_cubic: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> String {
	let text = escape(&label.chars().take(10).collect::<String>());
	let fill_w = (volume_cubic.clamp(0.0, 1.0) * 100.0).round() as i32;
	let bar = colors.bar(muted);
	let y = text_y(ui.icon().is_some(), 64);
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128">
{icon}<text x="64" y="{y}" font-family="sans-serif" font-size="40" font-weight="bold" fill="#ffffff" text-anchor="middle">{text}</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="{fill_w}" height="12" rx="6" fill="{bar}"/>
{slash}</svg>"##,
		icon = icon_overlay(ui),
		slash = mute_slash(muted, colors.mute())
	);
	data_uri(&svg)
}

/// Minimal XML text escaping for untrusted strings (e.g. application names) and
/// attribute values (e.g. icon data URIs).
fn escape(s: &str) -> String {
	s.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
		.replace('"', "&quot;")
}

// --- Encoder touchstrip feedback (Stream Deck+) ------------------------------

/// The touchstrip `value` text for a level: "NN%", "muted", or "n/a" when the
/// stream is unavailable. Shared by [`bar_feedback`] and the UI preview text.
pub fn value_text(known: bool, volume_cubic: f32, muted: bool) -> String {
	if !known {
		"n/a".to_owned()
	} else if muted {
		"muted".to_owned()
	} else {
		format!("{}%", (volume_cubic * 100.0).round() as i64)
	}
}

/// Square side length of the normalized dial icon (see [`dial_icon`]).
const DIAL_ICON_SIDE: u32 = 128;

/// Normalized dial icons by original data URI, so the decode/resize/re-encode
/// in [`dial_icon`] only runs when the icon actually changes.
static DIAL_ICON_CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<String, String>>> =
	std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Normalize a user icon data URI for the encoder touchstrip: the strip draws
/// the icon into a square box, stretching whatever it gets, so a non-square
/// image is fitted into a square PNG padded with transparency (aspect
/// preserved). Square images and anything that isn't a decodable raster image
/// (SVG, unknown formats) pass through unchanged.
pub(crate) fn dial_icon(uri: &str) -> String {
	if let Some(hit) = DIAL_ICON_CACHE.lock().unwrap().get(uri) {
		return hit.clone();
	}
	let normalized = squared_png(uri).unwrap_or_else(|| uri.to_owned());
	DIAL_ICON_CACHE
		.lock()
		.unwrap()
		.insert(uri.to_owned(), normalized.clone());
	normalized
}

/// Fit the raster image in a `data:` URI onto a transparent [`DIAL_ICON_SIDE`]
/// square and re-encode it as PNG. `None` when the image is already square or
/// the URI isn't a decodable raster image.
fn squared_png(uri: &str) -> Option<String> {
	let rest = uri.strip_prefix("data:")?;
	let (mime, b64) = rest.split_once(";base64,")?;
	if !mime.starts_with("image/") || mime.contains("svg") {
		return None;
	}
	let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
	let img = image::load_from_memory(&bytes).ok()?;
	if img.width() == 0 || img.height() == 0 || img.width() == img.height() {
		return None;
	}
	let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
	let img = image::load_from_memory(&bytes).ok()?;
	if img.width() == 0 || img.height() == 0 {
		return None;
	}
	let fitted = img.thumbnail(DIAL_ICON_SIDE, DIAL_ICON_SIDE).to_rgba8();
	let mut canvas =
		image::RgbaImage::from_pixel(DIAL_ICON_SIDE, DIAL_ICON_SIDE, image::Rgba([0, 0, 0, 0]));
	image::imageops::overlay(
		&mut canvas,
		&fitted,
		((DIAL_ICON_SIDE - fitted.width()) / 2) as i64,
		((DIAL_ICON_SIDE - fitted.height()) / 2) as i64,
	);
	let mut buf = Vec::new();
	canvas
		.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
		.ok()?;
	Some(format!(
		"data:image/png;base64,{}",
		base64::engine::general_purpose::STANDARD.encode(&buf)
	))
}

/// `setFeedback` for the `$B1` volume layout: the "NN%"/"muted" `value` and the
/// level `indicator` bar. A non-empty `title` (the surface label — a device/app
/// name or the user's custom label) is sent too; otherwise the title is left to
/// OpenDeck. `known == false` renders an "n/a" with a full bar in the
/// configured n/a colour (yellow by default). A configured `icon` (data URI) is
/// sent as the touchstrip `icon` — normalized to a square (see [`dial_icon`])
/// so the strip doesn't stretch it; layouts without icon support ignore the
/// field.
///
/// The layout can't draw the keypad's mute slash, so mute turns the value text and
/// the bar to the mute colour instead — the strongest cue it allows. `setFeedback`
/// merges into the previous payload, so the colours are always sent explicitly
/// (else the mute colour would persist after unmuting).
pub fn bar_feedback(
	title: &str,
	known: bool,
	volume_cubic: f32,
	muted: bool,
	colors: &BarColors,
	icon: Option<&str>,
) -> Value {
	let mut fb = if !known {
		let na = colors.unavailable();
		json!({
			"value": { "value": value_text(false, volume_cubic, muted), "color": na },
			"indicator": { "value": 100, "bar_fill_c": na },
		})
	} else {
		let pct = (volume_cubic * 100.0).round() as i64;
		let bar = pct.clamp(0, 100);
		if muted {
			json!({
				"value": { "value": value_text(true, volume_cubic, true), "color": colors.mute() },
				"indicator": { "value": bar, "bar_fill_c": colors.mute() },
			})
		} else {
			json!({
				"value": { "value": value_text(true, volume_cubic, false), "color": "#ffffff" },
				"indicator": { "value": bar, "bar_fill_c": colors.active() },
			})
		}
	};
	if !title.is_empty() {
		fb["title"] = Value::String(title.to_owned());
	}
	if let Some(uri) = icon.filter(|s| !s.is_empty()) {
		fb["icon"] = Value::String(dial_icon(uri));
	}
	fb
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::color::UNAVAILABLE;

	/// Decode a `data:image/svg+xml;base64,...` URI back to its SVG source.
	fn svg_of(uri: &str) -> String {
		let b64 = uri.strip_prefix("data:image/svg+xml;base64,").unwrap();
		String::from_utf8(
			base64::engine::general_purpose::STANDARD
				.decode(b64)
				.unwrap(),
		)
		.unwrap()
	}

	fn ui_with(icon: Option<&str>) -> VolumeUi {
		VolumeUi {
			icon: icon.map(str::to_owned),
			..VolumeUi::default()
		}
	}

	#[test]
	fn label_key_renders_label() {
		let svg = svg_of(&label_bar_key(
			"Brave",
			0.5,
			false,
			&BarColors::default(),
			&ui_with(None),
		));
		assert!(svg.contains(r#"font-size="40""#));
		assert!(svg.contains(">Brave<"));
	}

	#[test]
	fn icon_overlay_embeds_data_uri_undistorted() {
		let uri = "data:image/png;base64,iVBORw0KGgo=";
		let svg = svg_of(&volume_key(
			0.5,
			false,
			&BarColors::default(),
			&ui_with(Some(uri)),
		));
		assert!(svg.contains(&format!(r#"href="{uri}""#)));
		assert!(svg.contains(&format!(r#"xlink:href="{uri}""#)));
		assert!(svg.contains(r#"preserveAspectRatio="xMidYMid meet""#));
	}

	#[test]
	fn unavailable_key_is_yellow_na_by_default() {
		let svg = svg_of(&unavailable_key(
			&BarColors::default(),
			&VolumeUi::default(),
		));
		assert!(svg.contains(">n/a<"));
		assert!(svg.contains(UNAVAILABLE));
	}

	#[test]
	fn unavailable_key_uses_configured_color() {
		let colors = BarColors {
			unavailable_color: Some("#123456".to_owned()),
			..BarColors::default()
		};
		let svg = svg_of(&unavailable_key(&colors, &VolumeUi::default()));
		assert!(svg.contains("#123456"));
		assert!(!svg.contains(UNAVAILABLE));
	}

	#[test]
	fn dial_icon_pads_to_square_png() {
		// A 4x2 red PNG must come back as a 128x128 PNG (aspect preserved via
		// padding, never stretched).
		let mut buf = Vec::new();
		image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]))
			.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
			.unwrap();
		let uri = format!(
			"data:image/png;base64,{}",
			base64::engine::general_purpose::STANDARD.encode(&buf)
		);
		let out = dial_icon(&uri);
		let rest = out
			.strip_prefix("data:image/png;base64,")
			.expect("re-encoded as PNG data URI");
		let raw = base64::engine::general_purpose::STANDARD
			.decode(rest)
			.unwrap();
		let img = image::load_from_memory(&raw).unwrap();
		assert_eq!((img.width(), img.height()), (128, 128));
	}

	#[test]
	fn dial_icon_passes_through_svg() {
		let uri = "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=";
		assert_eq!(dial_icon(uri), uri);
	}

	#[test]
	fn dial_icon_passes_through_square_png() {
		let mut buf = Vec::new();
		image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 255, 255]))
			.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
			.unwrap();
		let uri = format!(
			"data:image/png;base64,{}",
			base64::engine::general_purpose::STANDARD.encode(&buf)
		);
		assert_eq!(dial_icon(&uri), uri);
	}

	#[test]
	fn value_text_covers_states() {
		assert_eq!(value_text(false, 0.0, false), "n/a");
		assert_eq!(value_text(true, 0.5, true), "muted");
		assert_eq!(value_text(true, 0.723, false), "72%");
	}

	#[test]
	fn feedback_sends_na_and_icon() {
		let fb = bar_feedback("", false, 0.0, false, &BarColors::default(), None);
		assert_eq!(fb["value"]["value"], Value::String("n/a".to_owned()));
		let fb = bar_feedback(
			"Brave",
			true,
			0.5,
			false,
			&BarColors::default(),
			Some("data:image/png;base64,AAA"),
		);
		assert_eq!(fb["title"], Value::String("Brave".to_owned()));
		assert_eq!(
			fb["icon"],
			Value::String("data:image/png;base64,AAA".to_owned())
		);
	}
}
