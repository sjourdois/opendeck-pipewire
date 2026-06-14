//! Mic Volume action: adjusts the default input (source) volume.
//!
//! - Keypad: each press nudges the mic volume up or down by `step`.
//! - Encoder: each tick changes it by `step`; pressing the dial mutes the mic.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::PwHandle;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MicVolumeSettings {
	/// Step per press / tick, in percent (clamped to 1–20).
	pub step: u8,
	/// For a Keypad button: direction. `true` = up, `false` = down.
	pub up: bool,
}

impl Default for MicVolumeSettings {
	fn default() -> Self {
		Self { step: 5, up: true }
	}
}

pub struct MicVolumeAction {
	pub pw: PwHandle,
}

#[async_trait]
impl Action for MicVolumeAction {
	const UUID: &'static str = "fr.jourdois.pipewire.micvolume";
	type Settings = MicVolumeSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		self.pw
			.adjust_default_source_volume(if settings.up { step } else { -step });
		self.refresh(instance).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		self.pw.adjust_default_source_volume(ticks as f32 * step);
		self.refresh(instance).await
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.pw.toggle_default_source_mute();
		self.refresh(instance).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresh(instance).await
	}
}

impl MicVolumeAction {
	async fn refresh(&self, instance: &Instance) -> OpenActionResult<()> {
		let snap = self.pw.default_source_snapshot();
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
