//! Volume action: adjusts the default sink (output) volume.
//!
//! - Keypad: each press nudges the volume up or down by `step`.
//! - Encoder: each tick changes the volume by `step`; pressing the dial mutes.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::PwHandle;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VolumeSettings {
	/// Step per key press / dial tick, in percent (clamped to 1–20). Mirrors the
	/// "Configurable Volume Step" slider of the original PulseAudio plugin.
	pub step: u8,
	/// For a Keypad button: direction. `true` = up, `false` = down.
	pub up: bool,
}

impl Default for VolumeSettings {
	fn default() -> Self {
		Self { step: 5, up: true }
	}
}

pub struct VolumeAction {
	pub pw: PwHandle,
}

#[async_trait]
impl Action for VolumeAction {
	const UUID: &'static str = "fr.jourdois.pipewire.volume";
	type Settings = VolumeSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		let delta = if settings.up { step } else { -step };
		self.apply(instance, delta).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		self.apply(instance, ticks as f32 * step).await
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		// Pressing the encoder toggles mute on the default sink.
		self.pw.toggle_default_sink_mute();
		self.refresh_title(instance).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresh_title(instance).await
	}
}

impl VolumeAction {
	async fn apply(&self, instance: &Instance, delta_cubic: f32) -> OpenActionResult<()> {
		self.pw.adjust_default_sink_volume(delta_cubic);
		self.refresh_title(instance).await
	}

	/// Render the current level onto the key (percentage + level bar).
	async fn refresh_title(&self, instance: &Instance) -> OpenActionResult<()> {
		let snap = self.pw.default_sink_snapshot();
		if !snap.known {
			return instance.set_title(Some("—"), None).await;
		}
		instance
			.set_image(
				Some(crate::render::volume_key(snap.volume_cubic, snap.mute)),
				None,
			)
			.await
	}
}
