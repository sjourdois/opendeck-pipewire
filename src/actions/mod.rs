//! The plugin's Stream Deck actions — one submodule per action.

use serde::{Deserialize, Serialize};

/// Build an action UUID from the shared reverse-DNS prefix, so the prefix lives
/// in exactly one place: `action_uuid!("volume") == "fr.jourdois.pipewire.volume"`.
/// (`concat!` needs literals, hence a macro rather than a `const` prefix.)
macro_rules! action_uuid {
	($suffix:literal) => {
		concat!("fr.jourdois.pipewire.", $suffix)
	};
}
pub(crate) use action_uuid;

/// A configured volume step (clamped to 1–20%) as a 0.01–0.20 cubic fraction.
pub(crate) fn step_fraction(step: u8) -> f32 {
	step.clamp(1, 20) as f32 / 100.0
}

/// What a Keypad press does on a volume action. Encoders ignore it: the dial
/// rotates for volume and presses to mute. This folds the former standalone Mute
/// actions into every volume action.
#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum KeyMode {
	/// Nudge the volume up (the default).
	#[default]
	Up,
	/// Nudge the volume down.
	Down,
	/// Toggle mute.
	Mute,
}

pub mod app_volume;
pub mod device_volume;
pub mod input_volume;
pub mod mic_volume;
pub mod output;
pub mod push_to_talk;
pub mod switch_input;
pub mod volume;
