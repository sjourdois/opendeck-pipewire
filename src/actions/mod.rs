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

#[cfg(test)]
mod tests {
	//! The `setSettings` payloads the property inspectors actually send, captured
	//! by driving each `.html` in a browser against a stand-in for OpenDeck's
	//! websocket. They pin the one contract nothing else checks: the field names
	//! and shapes the inspectors emit against what these structs deserialize,
	//! across the two `#[serde(flatten)]` groups (bar colours and appearance).

	use super::*;
	use crate::actions::app_volume::AppVolumeSettings;
	use crate::actions::device_volume::DeviceVolumeSettings;
	use crate::actions::input_volume::InputVolumeSettings;
	use crate::actions::mic_volume::MicVolumeSettings;
	use crate::actions::output::OutputSettings;
	use crate::actions::volume::VolumeSettings;

	#[test]
	fn volume_payload() {
		let s: VolumeSettings = serde_json::from_str(
			r##"{"step":7,"mode":"up","title":"Sortie","unmute_color":"#3db36b","mute_color":"#ff3b30","unavailable_color":"#abcdef","icon":"data:image/png;base64,iVBORw0K","limit_100":false}"##,
		)
		.unwrap();
		assert_eq!(s.step, 7);
		assert_eq!(s.title.as_deref(), Some("Sortie"));
		assert_eq!(s.colors.unavailable(), "#abcdef");
		assert_eq!(s.ui.icon(), Some("data:image/png;base64,iVBORw0K"));
		assert_eq!(s.ui.max_cubic(), 1.5);
	}

	#[test]
	fn mic_payload() {
		let s: MicVolumeSettings = serde_json::from_str(
			r##"{"step":5,"mode":"up","title":null,"unmute_color":"#3db36b","mute_color":"#ff3b30","unavailable_color":"#eab308","icon":"data:image/png;base64,iVBORw0K","limit_100":true}"##,
		)
		.unwrap();
		assert_eq!(s.title, None);
		assert_eq!(s.ui.max_cubic(), 1.0);
		assert!(s.ui.icon().is_some());
	}

	#[test]
	fn device_payload() {
		let s: DeviceVolumeSettings = serde_json::from_str(
			r##"{"sink":"alsa_output.usb.a50","name":"","step":5,"mode":"up","unmute_color":"#3db36b","mute_color":"#ff3b30","unavailable_color":"#eab308","icon":null,"limit_100":true}"##,
		)
		.unwrap();
		assert_eq!(s.sink.as_deref(), Some("alsa_output.usb.a50"));
		assert!(s.name.is_empty());
		// A cleared icon arrives as JSON null, which must read as "no icon".
		assert_eq!(s.ui.icon(), None);
		assert_eq!(s.ui.max_cubic(), 1.0);
	}

	#[test]
	fn input_payload() {
		let s: InputVolumeSettings = serde_json::from_str(
			r##"{"source":"alsa_input.usb.a50","name":"Micro","step":5,"mode":"mute","unmute_color":"#3db36b","mute_color":"#ff3b30","unavailable_color":"#112233","icon":null,"limit_100":false}"##,
		)
		.unwrap();
		assert_eq!(s.name, "Micro");
		assert!(s.mode == KeyMode::Mute);
		assert_eq!(s.colors.unavailable(), "#112233");
	}

	#[test]
	fn app_payload() {
		let s: AppVolumeSettings = serde_json::from_str(
			r##"{"app":"Brave","name":"Navigateur","step":5,"mode":"up","unmute_color":"#3db36b","mute_color":"#ff3b30","unavailable_color":"#00ff00","icon":null,"limit_100":false}"##,
		)
		.unwrap();
		assert_eq!(app_volume::label(&s), "Navigateur");
		assert_eq!(s.colors.unavailable(), "#00ff00");
	}

	#[test]
	fn output_payload() {
		let s: OutputSettings = serde_json::from_str(
			r##"{"sinks":["alsa_output.usb.a50","alsa_output.pci.hdmi"],"stream":null,"when_inactive":"cycle","title":null,"icons":{},"inactive_icon":null}"##,
		)
		.unwrap();
		assert_eq!(s.sinks.len(), 2);
		assert!(s.icons.is_empty());
		assert_eq!(s.inactive_icon, None);
	}
}
