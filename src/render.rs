//! On-the-fly visual feedback.
//!
//! Keypad buttons get a full 128×128 SVG key image via `set_image`. Encoders
//! (Stream Deck+ dials) use the `$B1`-shaped touchstrip layout the plugin ships
//! (see [`crate::display::encoder_layout`]), for which this module builds the
//! `setFeedback` value+bar payload (see [`bar_feedback`]): the resolved label as
//! the `title`, and a user-configured icon as the touchstrip `icon`.

use base64::Engine;
use serde_json::{Value, json};

use crate::color::{BOOST, BarColors, OVERDRIVE};
use crate::ui::VolumeUi;

/// The key's margin, top and bottom: the level bar sits this far above the
/// bottom edge, and the icon (or a label tall enough to reach) this far below
/// the top.
const MARGIN: i32 = 12;
/// The level bar's height, and its top edge.
const BAR_H: i32 = 12;
const BAR_Y: i32 = 128 - MARGIN - BAR_H;
/// The icon's size, drawn at the top of the key.
const ICON: i32 = 32;
/// The level bar's left edge and full width.
const BAR_X: f32 = 14.0;
const BAR_W: f32 = 100.0;
/// The level a boosted bar spans end to end (the most a volume action allows)…
const BOOST_MAX: f32 = 1.5;
/// … and the level past which its boost turns from orange to magenta.
const BOOST_WARN: f32 = 1.3;

/// Whether a level is boosted past 100% (with slack for float noise).
fn boosted(volume_cubic: f32) -> bool {
	volume_cubic > 1.001
}

/// The colour of a boosted bar's topmost segment: orange, or magenta past
/// [`BOOST_WARN`].
fn boost_color(volume_cubic: f32) -> &'static str {
	if volume_cubic > BOOST_WARN + 0.001 {
		OVERDRIVE
	} else {
		BOOST
	}
}

/// How much of the bar a level fills, 0..=1: the level itself up to 100%, so a
/// full bar is 100%; past that, the bar spans 0..[`BOOST_MAX`], compressing the
/// unboosted part to make room for the boost.
fn bar_fraction(volume_cubic: f32) -> f32 {
	if boosted(volume_cubic) {
		volume_cubic.min(BOOST_MAX) / BOOST_MAX
	} else {
		volume_cubic.clamp(0.0, 1.0)
	}
}

/// The level bar: a dark track filled to the level. Up to 100% the fill is one
/// colour (`fill`: the active or mute colour). Past it the bar compresses (see
/// [`bar_fraction`]) and, unless muted, the boost shows in segments after the
/// green: orange up to [`BOOST_WARN`], magenta beyond. The segments are clipped to
/// the fill's rounded outline, so only the bar's ends are rounded.
fn level_bar(volume_cubic: f32, muted: bool, colors: &BarColors) -> String {
	let fill = colors.bar(muted);
	let fill_w = BAR_W * bar_fraction(volume_cubic);
	let track = format!(
		r##"<rect x="{BAR_X}" y="{BAR_Y}" width="{BAR_W}" height="{BAR_H}" rx="6" fill="#3a3a3a"/>"##
	);
	if muted || !boosted(volume_cubic) {
		return format!(
			r##"{track}
<rect x="{BAR_X}" y="{BAR_Y}" width="{fill_w:.1}" height="{BAR_H}" rx="6" fill="{fill}"/>"##
		);
	}
	let at = |level: f32| BAR_X + BAR_W * level.min(BOOST_MAX) / BOOST_MAX;
	let segment = |from: f32, to: f32, color: &str| {
		format!(
			r##"<rect x="{x:.1}" y="{BAR_Y}" width="{w:.1}" height="{BAR_H}" fill="{color}"/>"##,
			x = at(from),
			w = at(to) - at(from)
		)
	};
	let mut segments = segment(0.0, 1.0, fill) + &segment(1.0, volume_cubic.min(BOOST_WARN), BOOST);
	if volume_cubic > BOOST_WARN {
		segments += &segment(BOOST_WARN, volume_cubic, OVERDRIVE);
	}
	format!(
		r##"{track}
<clipPath id="level"><rect x="{BAR_X}" y="{BAR_Y}" width="{fill_w:.1}" height="{BAR_H}" rx="6"/></clipPath>
<g clip-path="url(#level)">{segments}</g>"##
	)
}

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

/// A custom icon (a data URI) drawn at reduced opacity over the key's black
/// background, for an Output Device key whose device isn't the active one.
pub fn dimmed(icon: &str) -> String {
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128"><image width="128" height="128" opacity="0.35" preserveAspectRatio="xMidYMid meet" href="{uri}" xlink:href="{uri}"/></svg>"##,
		uri = escape(icon)
	))
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
/// (SVG2) and `xlink:href` (SVG1.1) are set so older renderers pick it up too.
/// The picker squares new icons itself (see `propertyInspector/icon.js`), so
/// `preserveAspectRatio` is what keeps an icon stored by an earlier version
/// undistorted rather than stretched.
fn icon_overlay(ui: &VolumeUi) -> String {
	match ui.icon() {
		Some(uri) => format!(
			r##"<image x="48" y="{MARGIN}" width="{ICON}" height="{ICON}" preserveAspectRatio="xMidYMid meet" href="{uri}" xlink:href="{uri}"/>"##,
			uri = escape(uri)
		),
		None => String::new(),
	}
}

/// The main text's vertical baseline, shifted down when the icon occupies the top.
fn text_y(with_icon: bool, plain: i32) -> i32 {
	if with_icon { plain + 12 } else { plain }
}

/// The main text's size: shrunk to leave the icon room, `plain` otherwise.
fn text_size(with_icon: bool, plain: i32) -> i32 {
	if with_icon { 20 } else { plain }
}

/// A volume key: a big percentage with a horizontal level bar underneath.
/// `volume_cubic` is on the 0..=1 (perceptual) scale; values above 1 are boost.
pub fn volume_key(volume_cubic: f32, muted: bool, colors: &BarColors, ui: &VolumeUi) -> String {
	let pct = (volume_cubic * 100.0).round() as i32;
	let with_icon = ui.icon().is_some();
	let y = text_y(with_icon, 68);
	let (label, label_color, plain_size) = if muted {
		("muted".to_owned(), "#999999", 26)
	} else {
		(format!("{pct}%"), "#ffffff", 34)
	};
	let size = text_size(with_icon, plain_size);

	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128">
{icon}<text x="64" y="{y}" font-family="sans-serif" font-size="{size}" font-weight="bold" fill="{label_color}" text-anchor="middle">{label}</text>
{bar}
{slash}</svg>"##,
		icon = icon_overlay(ui),
		bar = level_bar(volume_cubic, muted, colors),
		slash = mute_slash(muted, colors.mute())
	);
	data_uri(&svg)
}

/// A key shown when the audio stream is unavailable (no default device, a
/// configured device that's gone, an app that isn't playing…): "n/a" over a
/// full-width bar in the configured n/a colour (yellow by default).
pub fn unavailable_key(colors: &BarColors, ui: &VolumeUi) -> String {
	let with_icon = ui.icon().is_some();
	let y = text_y(with_icon, 68);
	let size = text_size(with_icon, 34);
	let na = colors.unavailable();
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128">
{icon}<text x="64" y="{y}" font-family="sans-serif" font-size="{size}" font-weight="bold" fill="#ffffff" text-anchor="middle">n/a</text>
<rect x="14" y="{BAR_Y}" width="100" height="{BAR_H}" rx="6" fill="#3a3a3a"/>
<rect x="14" y="{BAR_Y}" width="100" height="{BAR_H}" rx="6" fill="{na}"/>
</svg>"##,
		icon = icon_overlay(ui),
	);
	data_uri(&svg)
}

/// Usable label width on a key, in SVG units (the key is 128 wide).
const LABEL_WIDTH: f32 = 116.0;
/// Rough advance of a bold sans-serif glyph, as a fraction of the font size:
/// measured on DejaVu Sans Bold, mixed case. The renderer's font isn't known
/// here, so this errs wide: a fitted label may come out a little short of the
/// edges, and only a run of capitals like "WW" reaches past them.
const GLYPH_EM: f32 = 0.72;
/// Height of a capital above the baseline, as a fraction of the font size…
const CAP_EM: f32 = 0.75;
/// … and of a descender below it.
const DESC_EM: f32 = 0.22;
/// Baseline-to-baseline distance of a multiline label, as a fraction of the
/// font size.
const LINE_EM: f32 = 1.15;
/// The auto-fitted label never grows past the size labels always had…
const LABEL_MAX_AUTO: i32 = 40;
/// … nor shrinks below what stays legible on a key; past that it is cut instead.
const LABEL_MIN: i32 = 14;
/// The largest size a user can pick (the property inspector slider spans
/// `LABEL_MIN..=LABEL_MAX`).
const LABEL_MAX: i32 = 56;
/// The gap kept between the label's descenders and the bar, and under the icon.
const LABEL_GAP: i32 = 6;

/// The band a label is centred in, top and bottom: from the top margin (or just
/// under the icon, when one occupies the top) down to just above the bar.
fn label_band(with_icon: bool) -> (f32, f32) {
	let top = if with_icon {
		MARGIN + ICON + LABEL_GAP
	} else {
		MARGIN
	};
	(top as f32, (BAR_Y - LABEL_GAP) as f32)
}

/// The height of `lines` lines of a label at font size `s`, from the first
/// line's capitals to the last line's descenders.
fn label_height(lines: usize, s: f32) -> f32 {
	(CAP_EM + DESC_EM + (lines - 1) as f32 * LINE_EM) * s
}

/// The label's lines (one per line break in it; blank lines dropped) and the font
/// size to draw them at: `size` if the user picked one, else the largest that
/// fits every line whole, both across and stacked up the key (within the auto
/// range). Lines still too long at that size are cut to the characters that fit,
/// and lines that don't fit above the bar are dropped, keeping at least the first;
/// either way the cut shows as an ellipsis (see [`ellipsize`]).
fn fit_label(label: &str, size: Option<u8>, with_icon: bool) -> (Vec<String>, i32) {
	let mut lines: Vec<&str> = label
		.lines()
		.map(str::trim)
		.filter(|l| !l.is_empty())
		.collect();
	if lines.is_empty() {
		lines.push("");
	}
	let (top, bottom) = label_band(with_icon);
	let room = bottom - top;
	let size = match size {
		Some(s) => i32::from(s).clamp(LABEL_MIN, LABEL_MAX),
		None => {
			let chars = lines
				.iter()
				.map(|l| l.chars().count())
				.max()
				.unwrap_or(0)
				.max(1) as f32;
			let across = LABEL_WIDTH / (GLYPH_EM * chars);
			let stacked = room / label_height(lines.len(), 1.0);
			(across.min(stacked) as i32).clamp(LABEL_MIN, LABEL_MAX_AUTO)
		}
	};
	let s = size as f32;
	let rows = (((room - label_height(1, s)) / (LINE_EM * s)).max(0.0) as usize) + 1;
	let fits = ((LABEL_WIDTH / (GLYPH_EM * s)) as usize).max(1);
	let dropped = lines.len() > rows;
	let lines = lines
		.into_iter()
		.take(rows)
		.enumerate()
		.map(|(i, l)| ellipsize(l, fits, dropped && i + 1 == rows))
		.collect();
	(lines, size)
}

/// `line` as is if it fits in `fits` characters, else cut to end in an ellipsis
/// within them; `more` (lines dropped after this one) forces the ellipsis.
fn ellipsize(line: &str, fits: usize, more: bool) -> String {
	if !more && line.chars().count() <= fits {
		return line.to_owned();
	}
	let kept: String = line.chars().take(fits - 1).collect();
	format!("{}…", kept.trim_end())
}

/// A labelled level-bar key/encoder: a custom label with a level bar (no
/// percentage), shared by the device / input / app volume actions. The label is
/// sized per [`fit_label`] and centred between the top margin (or the icon) and
/// the bar, so a label that fills the height is as far from the top as the bar
/// is from the bottom.
pub fn label_bar_key(
	label: &str,
	volume_cubic: f32,
	muted: bool,
	colors: &BarColors,
	ui: &VolumeUi,
) -> String {
	let with_icon = ui.icon().is_some();
	let (lines, size) = fit_label(label, ui.label_size, with_icon);
	let s = size as f32;
	let (top, bottom) = label_band(with_icon);
	let slack = (bottom - top - label_height(lines.len(), s)).max(0.0);
	let first = (top + slack / 2.0 + CAP_EM * s).round() as i32;
	let step = (s * LINE_EM).round() as i32;
	let text: String = lines
		.iter()
		.enumerate()
		.map(|(i, line)| {
			format!(
				r#"<tspan x="64" y="{}">{}</tspan>"#,
				first + step * i as i32,
				escape(line)
			)
		})
		.collect();
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 128 128">
{icon}<text font-family="sans-serif" font-size="{size}" font-weight="bold" fill="#ffffff" text-anchor="middle">{text}</text>
{bar}
{slash}</svg>"##,
		icon = icon_overlay(ui),
		bar = level_bar(volume_cubic, muted, colors),
		slash = mute_slash(muted, colors.mute())
	);
	data_uri(&svg)
}

/// A label as one line, for surfaces that can't break one: its lines joined
/// by spaces.
pub fn one_line(label: &str) -> String {
	label
		.lines()
		.map(str::trim)
		.filter(|l| !l.is_empty())
		.collect::<Vec<_>>()
		.join(" ")
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

/// `setFeedback` for the volume touchstrip layout: the "NN%"/"muted" `value` and the
/// level `indicator` bar. A non-empty `title` (the surface label — a device/app
/// name or the user's custom label) is sent too; otherwise the title is left to
/// OpenDeck. `known == false` renders an "n/a" with a full bar in the
/// configured n/a colour (yellow by default). A configured `icon` (data URI) is
/// sent as the touchstrip `icon`; layouts without icon support ignore the field.
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
		// The touchstrip bar takes one colour, so a boosted level fills it in
		// the colour of the highest segment the keypad bar would draw.
		let bar = (bar_fraction(volume_cubic) * 100.0).round() as i64;
		let fill = if boosted(volume_cubic) {
			boost_color(volume_cubic)
		} else {
			colors.active()
		};
		if muted {
			json!({
				"value": { "value": value_text(true, volume_cubic, true), "color": colors.mute() },
				"indicator": { "value": bar, "bar_fill_c": colors.mute() },
			})
		} else {
			json!({
				"value": { "value": value_text(true, volume_cubic, false), "color": "#ffffff" },
				"indicator": { "value": bar, "bar_fill_c": fill },
			})
		}
	};
	if !title.is_empty() {
		fb["title"] = Value::String(title.to_owned());
	}
	if let Some(uri) = icon.filter(|s| !s.is_empty()) {
		fb["icon"] = Value::String(uri.to_owned());
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
		assert!(svg.contains(r#"font-size="32""#));
		assert!(svg.contains(">Brave<"));
	}

	#[test]
	fn auto_label_shrinks_to_fit() {
		// Short labels stay at the classic size, longer ones shrink to fit whole.
		assert_eq!(fit_label("Mic", None, false), (vec!["Mic".to_owned()], 40));
		let (lines, size) = fit_label("Headphones", None, false);
		assert_eq!(lines, ["Headphones"]);
		assert!((LABEL_MIN..40).contains(&size));
		// Past the legible minimum the label is cut, not shrunk further.
		let (lines, size) = fit_label("Built-in Audio Analog Stereo", None, false);
		assert_eq!(size, LABEL_MIN);
		assert_eq!(lines, ["Built-in A…"]);
	}

	#[test]
	fn ellipsis_marks_a_cut() {
		assert_eq!(ellipsize("Speakers", 8, false), "Speakers");
		assert_eq!(ellipsize("Speakers", 5, false), "Spea…");
		// No space left dangling before the ellipsis.
		assert_eq!(ellipsize("Desk Speakers", 6, false), "Desk…");
		// A whole line followed by dropped ones still ends in one.
		assert_eq!(ellipsize("Game", 8, true), "Game…");
		assert_eq!(ellipsize("Speakers", 1, false), "…");
	}

	#[test]
	fn picked_label_size_is_kept_and_cut_to_fit() {
		let (lines, size) = fit_label("Headphones", Some(50), false);
		assert_eq!(size, 50);
		assert_eq!(lines, ["He…"]);
		// Out-of-range sizes (a hand-edited profile) are clamped.
		assert_eq!(fit_label("A", Some(200), false).1, LABEL_MAX);
		assert_eq!(fit_label("A", Some(1), false).1, LABEL_MIN);
		// A non-ASCII label is cut on a character boundary.
		assert_eq!(fit_label("Écouteurs sans fil", Some(56), false).0, ["É…"]);
	}

	#[test]
	fn multiline_label_fits_every_line() {
		// Sized to the longest line, and shrunk further to stack them all.
		let (lines, size) = fit_label("Desk\nSpeakers", None, false);
		assert_eq!(lines, ["Desk", "Speakers"]);
		assert_eq!(size, 20);
		// Blank lines and stray whitespace (a CRLF paste) are dropped.
		let (lines, _) = fit_label("\r\nGame \r\n\r\nChat\r\n", None, false);
		assert_eq!(lines, ["Game", "Chat"]);
		// A short pair fills the height; under the icon there's less of it.
		assert_eq!(fit_label("Web\n+", None, false).1, 40);
		assert!(fit_label("Web\n+", None, true).1 < 40);
	}

	#[test]
	fn lines_past_the_top_are_dropped() {
		// At a picked size only what fits above the bar is kept…
		let (lines, _) = fit_label("A\nB\nC", Some(56), false);
		assert_eq!(lines, ["A…"]);
		// … and at the legible minimum, the excess of a long list.
		let (lines, size) = fit_label("1\n2\n3\n4\n5\n6\n7\n8", None, false);
		assert_eq!(size, LABEL_MIN);
		assert!((2..8).contains(&lines.len()) && lines[0] == "1");
		assert!(lines.last().unwrap().ends_with('…'));
	}

	#[test]
	fn tall_label_sits_a_margin_below_the_top() {
		let svg = svg_of(&label_bar_key(
			"Web\n+",
			0.5,
			false,
			&BarColors::default(),
			&ui_with(None),
		));
		// Capitals 30 high at size 40: "Web" tops out at 13, by the 12 margin.
		assert!(svg.contains(r#"font-size="40""#));
		assert!(svg.contains(r#"<tspan x="64" y="43">Web</tspan>"#));
		assert!(svg.contains(r#"<tspan x="64" y="89">+</tspan>"#));
		assert!(svg.contains(r#"y="104""#));
	}

	#[test]
	fn bar_is_full_at_100_and_compresses_past_it() {
		assert_eq!(bar_fraction(0.5), 0.5);
		assert_eq!(bar_fraction(1.0), 1.0);
		assert!((bar_fraction(1.2) - 0.8).abs() < 1e-6);
		assert_eq!(bar_fraction(1.5), 1.0);
		// A level set past the actions' limit (by another app) still fits.
		assert_eq!(bar_fraction(2.0), 1.0);
	}

	#[test]
	fn boost_draws_green_then_orange_then_magenta() {
		let c = BarColors::default();
		let plain = level_bar(1.0, false, &c);
		assert!(plain.contains(r#"width="100.0""#) && !plain.contains(BOOST));
		// 120%: green to 100% (two thirds of the bar), orange after.
		let warm = level_bar(1.2, false, &c);
		assert!(warm.contains(r#"<clipPath id="level"><rect x="14" y="104" width="80.0""#));
		assert!(warm.contains(r##"x="14.0" y="104" width="66.7" height="12" fill="#3db36b""##));
		assert!(warm.contains(BOOST) && !warm.contains(OVERDRIVE));
		// 140%: orange stops at 130%, magenta takes over.
		let hot = level_bar(1.4, false, &c);
		assert!(hot.contains(r##"x="80.7" y="104" width="20.0" height="12" fill="#f08c2e""##));
		assert!(hot.contains(r##"x="100.7" y="104" width="6.7" height="12" fill="#d946ef""##));
		// Muted, a boosted bar is compressed but drawn in the mute colour alone.
		let muted = level_bar(1.4, true, &c);
		assert!(!muted.contains(BOOST) && muted.contains(r#"width="93.3""#));
	}

	#[test]
	fn feedback_bar_compresses_and_warns_when_boosted() {
		let c = BarColors::default();
		let fb = bar_feedback("", true, 1.2, false, &c, None);
		assert_eq!(fb["indicator"]["value"], 80);
		assert_eq!(fb["indicator"]["bar_fill_c"], BOOST);
		let fb = bar_feedback("", true, 1.4, false, &c, None);
		assert_eq!(fb["indicator"]["bar_fill_c"], OVERDRIVE);
		let fb = bar_feedback("", true, 1.0, false, &c, None);
		assert_eq!(fb["indicator"]["value"], 100);
		assert_eq!(fb["indicator"]["bar_fill_c"], c.active());
	}

	#[test]
	fn one_line_joins_lines() {
		assert_eq!(one_line("Desk\r\n\nSpeakers "), "Desk Speakers");
		assert_eq!(one_line("Brave"), "Brave");
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
	fn percentage_only_shrinks_for_the_icon() {
		let plain = svg_of(&volume_key(
			0.5,
			false,
			&BarColors::default(),
			&ui_with(None),
		));
		assert!(plain.contains(r#"font-size="34""#));
		let muted = svg_of(&volume_key(
			0.5,
			true,
			&BarColors::default(),
			&ui_with(None),
		));
		assert!(muted.contains(r#"font-size="26""#));
		let with_icon = svg_of(&volume_key(
			0.5,
			false,
			&BarColors::default(),
			&ui_with(Some("data:image/png;base64,iVBORw0KGgo=")),
		));
		assert!(with_icon.contains(r#"font-size="20""#));
	}

	#[test]
	fn unavailable_key_matches_the_percentage_size() {
		let plain = svg_of(&unavailable_key(&BarColors::default(), &ui_with(None)));
		assert!(plain.contains(r#"font-size="34""#));
		let with_icon = svg_of(&unavailable_key(
			&BarColors::default(),
			&ui_with(Some("data:image/png;base64,iVBORw0KGgo=")),
		));
		assert!(with_icon.contains(r#"font-size="20""#));
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
