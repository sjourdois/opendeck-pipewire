//! User-configurable level-bar colours, shared by the volume actions.
//!
//! Flattened into each volume action's settings (`unmute_color` / `mute_color`).
//! Empty or malformed values fall back to the defaults; values are validated to
//! be `#rgb` / `#rrggbb` before use, since they are spliced into SVG attributes
//! and the encoder `setFeedback` JSON.

use serde::{Deserialize, Serialize};

/// Level-bar colour when active (unmuted).
pub const ACTIVE: &str = "#3db36b";
/// Level-bar / value colour when muted.
pub const MUTE: &str = "#ff3b30";

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct BarColors {
	/// Level-bar colour when unmuted (default green, [`ACTIVE`]).
	pub unmute_color: Option<String>,
	/// Level-bar (and keypad mute-slash) colour when muted (default red, [`MUTE`]).
	pub mute_color: Option<String>,
}

impl BarColors {
	/// The unmuted bar colour, validated (falls back to [`ACTIVE`]).
	pub fn active(&self) -> &str {
		pick(&self.unmute_color, ACTIVE)
	}

	/// The muted bar / slash colour, validated (falls back to [`MUTE`]).
	pub fn mute(&self) -> &str {
		pick(&self.mute_color, MUTE)
	}

	/// The bar colour for the current mute state.
	pub fn bar(&self, muted: bool) -> &str {
		if muted { self.mute() } else { self.active() }
	}
}

/// The configured colour if it is a valid hex colour, else the default.
fn pick<'a>(configured: &'a Option<String>, default: &'a str) -> &'a str {
	match configured.as_deref() {
		Some(c) if is_hex_color(c) => c,
		_ => default,
	}
}

/// `#rgb` or `#rrggbb` with hex digits only — safe to splice into SVG/JSON.
fn is_hex_color(s: &str) -> bool {
	match s.strip_prefix('#') {
		Some(hex) => {
			(hex.len() == 3 || hex.len() == 6) && hex.bytes().all(|b| b.is_ascii_hexdigit())
		}
		None => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A representative action settings struct: flatten + a container default, the
	/// exact shape the property inspectors deserialize into.
	#[derive(Deserialize, Default)]
	#[serde(default)]
	struct Settings {
		step: u8,
		#[serde(flatten)]
		colors: BarColors,
	}

	#[test]
	fn partial_json_uses_defaults() {
		// The PI may send settings without the colour fields at all.
		let s: Settings = serde_json::from_str(r#"{"step":7}"#).unwrap();
		assert_eq!(s.step, 7);
		assert_eq!(s.colors.active(), ACTIVE);
		assert_eq!(s.colors.mute(), MUTE);
	}

	#[test]
	fn custom_and_invalid_colors() {
		let s: Settings =
			serde_json::from_str(r##"{"unmute_color":"#123abc","mute_color":"oops"}"##).unwrap();
		assert_eq!(s.colors.active(), "#123abc"); // valid hex kept
		assert_eq!(s.colors.mute(), MUTE); // malformed falls back
	}
}
