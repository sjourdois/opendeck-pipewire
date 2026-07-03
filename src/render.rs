//! On-the-fly visual feedback.
//!
//! Keypad buttons get a full 128×128 SVG key image via `set_image`. Encoders
//! (Stream Deck+ dials) use the native `$B1` touchstrip layout, for which this
//! module builds the `setFeedback` value+bar payload (see [`bar_feedback`]); the
//! icon and title stay the dial's own OpenDeck configuration.

use base64::Engine;
use serde_json::{Value, json};

use crate::color::BarColors;

/// Build a `data:image/svg+xml;base64,...` URI suitable for `Instance::set_image`.
fn data_uri(svg: &str) -> String {
	let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
	format!("data:image/svg+xml;base64,{b64}")
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

/// A volume key: a big percentage with a horizontal level bar underneath.
/// `volume_cubic` is on the 0..=1 (perceptual) scale; values above 1 are boost.
pub fn volume_key(volume_cubic: f32, muted: bool, colors: &BarColors) -> String {
	let pct = (volume_cubic * 100.0).round() as i32;
	let fill_w = (volume_cubic.clamp(0.0, 1.0) * 100.0).round() as i32;
	let (bar, label, label_color, size) = if muted {
		(colors.mute(), "muted".to_owned(), "#999999", 26)
	} else if volume_cubic > 1.001 {
		("#d8843d", format!("{pct}%"), "#ffffff", 34) // boosted (fixed warning colour)
	} else {
		(colors.active(), format!("{pct}%"), "#ffffff", 34)
	};

	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<text x="64" y="66" font-family="sans-serif" font-size="{size}" font-weight="bold" fill="{label_color}" text-anchor="middle">{label}</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="{fill_w}" height="12" rx="6" fill="{bar}"/>
{slash}</svg>"##,
		slash = mute_slash(muted, colors.mute())
	);
	data_uri(&svg)
}

/// A labelled level-bar key/encoder: a custom label with a level bar (no
/// percentage), shared by the device / input / app volume actions.
pub fn label_bar_key(label: &str, volume_cubic: f32, muted: bool, colors: &BarColors) -> String {
	let text = escape(&label.chars().take(10).collect::<String>());
	let fill_w = (volume_cubic.clamp(0.0, 1.0) * 100.0).round() as i32;
	let bar = colors.bar(muted);
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<text x="64" y="64" font-family="sans-serif" font-size="40" font-weight="bold" fill="#ffffff" text-anchor="middle">{text}</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="{fill_w}" height="12" rx="6" fill="{bar}"/>
{slash}</svg>"##,
		slash = mute_slash(muted, colors.mute())
	);
	data_uri(&svg)
}

/// Minimal XML text escaping for untrusted strings (e.g. application names).
fn escape(s: &str) -> String {
	s.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
}

// --- Encoder touchstrip feedback (Stream Deck+) ------------------------------

/// `setFeedback` for the `$B1` volume layout: the "NN%"/"muted" `value` and the
/// level `indicator` bar. A non-empty `title` (the surface label — a device/app
/// name or the user's custom label) is sent too; otherwise the title — and the
/// icon — are left to OpenDeck. `known == false` renders a "—".
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
) -> Value {
	let mut fb = if !known {
		json!({ "value": "—", "indicator": 0 })
	} else {
		let pct = (volume_cubic * 100.0).round() as i64;
		let bar = pct.clamp(0, 100);
		if muted {
			json!({
				"value": { "value": "muted", "color": colors.mute() },
				"indicator": { "value": bar, "bar_fill_c": colors.mute() },
			})
		} else {
			json!({
				"value": { "value": format!("{pct}%"), "color": "#ffffff" },
				"indicator": { "value": bar, "bar_fill_c": colors.active() },
			})
		}
	};
	if !title.is_empty() {
		fb["title"] = Value::String(title.to_owned());
	}
	fb
}
