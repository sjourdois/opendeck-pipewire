//! On-the-fly key images (SVG data URIs) for live visual feedback.

use base64::Engine;

/// Build a `data:image/svg+xml;base64,...` URI suitable for `Instance::set_image`.
fn data_uri(svg: &str) -> String {
	let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
	format!("data:image/svg+xml;base64,{b64}")
}

/// A volume key: a big percentage with a horizontal level bar underneath.
/// `volume_cubic` is on the 0..=1 (perceptual) scale; values above 1 are boost.
pub fn volume_key(volume_cubic: f32, muted: bool) -> String {
	let pct = (volume_cubic * 100.0).round() as i32;
	let fill_w = (volume_cubic.clamp(0.0, 1.0) * 100.0).round() as i32;
	let (bar, label, label_color, size) = if muted {
		("#666666", "muted".to_owned(), "#999999", 26)
	} else if volume_cubic > 1.001 {
		("#d8843d", format!("{pct}%"), "#ffffff", 34) // boosted
	} else {
		("#3db36b", format!("{pct}%"), "#ffffff", 34)
	};

	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<text x="64" y="66" font-family="sans-serif" font-size="{size}" font-weight="bold" fill="{label_color}" text-anchor="middle">{label}</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="{fill_w}" height="12" rx="6" fill="{bar}"/>
</svg>"##
	);
	data_uri(&svg)
}

/// A device-volume key/encoder: a custom label with a level bar (no percentage),
/// mirroring the original plugin's output-volume faders.
pub fn device_key(label: &str, volume_cubic: f32, muted: bool) -> String {
	let text = escape(&label.chars().take(10).collect::<String>());
	let fill_w = (volume_cubic.clamp(0.0, 1.0) * 100.0).round() as i32;
	let bar = if muted { "#666666" } else { "#3db36b" };
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<text x="64" y="64" font-family="sans-serif" font-size="40" font-weight="bold" fill="#ffffff" text-anchor="middle">{text}</text>
<rect x="14" y="92" width="100" height="12" rx="6" fill="#3a3a3a"/>
<rect x="14" y="92" width="{fill_w}" height="12" rx="6" fill="{bar}"/>
</svg>"##
	);
	data_uri(&svg)
}

/// Minimal XML text escaping for untrusted strings (e.g. application names).
fn escape(s: &str) -> String {
	s.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
}

/// An application-volume key: the app name with a small speaker glyph.
/// (Per-app volume level is not yet tracked, so no live bar here.)
pub fn app_key(app: &str) -> String {
	let truncated: String = app.chars().take(14).collect();
	let name = escape(&truncated);
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<path d="M30 54 H46 L66 38 V90 L46 74 H30 Z" fill="#ffffff"/>
<path d="M76 52 Q90 64 76 76" fill="none" stroke="#ffffff" stroke-width="6" stroke-linecap="round"/>
<text x="64" y="116" font-family="sans-serif" font-size="20" font-weight="bold" fill="#ffffff" text-anchor="middle">{name}</text>
</svg>"##
	);
	data_uri(&svg)
}
