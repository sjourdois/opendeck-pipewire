//! Device Volume action: controls the volume of a specific output device (sink),
//! identified by its `node.name`. Mirrors the original plugin's "Output Volume".
//!
//! - Keypad: each press nudges that device's volume up/down by `step`.
//! - Encoder: each tick changes it by `step`; pressing the dial mutes the device.
//!
//! The key/encoder shows a custom label and a live level bar.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::{PwHandle, SinkDesc};
use crate::refresh::Refresher;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct DeviceVolumeSettings {
	/// `node.name` of the target sink (== original plugin's `outputName`).
	pub sink: Option<String>,
	/// Custom label shown on the key (== original `customName`).
	pub name: String,
	/// Step per press / tick, in percent (== original `volumeStep`).
	pub step: u8,
	/// For a Keypad button: direction. `true` = up, `false` = down.
	pub up: bool,
}

impl Default for DeviceVolumeSettings {
	fn default() -> Self {
		Self {
			sink: None,
			name: String::new(),
			step: 5,
			up: true,
		}
	}
}

#[derive(Serialize)]
struct SinksMessage {
	event: &'static str,
	sinks: Vec<SinkDesc>,
}

/// The key label: the custom name, else the sink's `node.name`, else "Device".
pub fn label(settings: &DeviceVolumeSettings) -> String {
	if settings.name.is_empty() {
		settings.sink.clone().unwrap_or_else(|| "Device".to_owned())
	} else {
		settings.name.clone()
	}
}

pub struct DeviceVolumeAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
}

#[async_trait]
impl Action for DeviceVolumeAction {
	const UUID: &'static str = "fr.jourdois.pipewire.devicevolume";
	type Settings = DeviceVolumeSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		let delta = if settings.up { step } else { -step };
		self.adjust(instance, settings, delta).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		self.adjust(instance, settings, ticks as f32 * step).await
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		if let Some(sink) = settings.sink.as_deref().filter(|s| !s.is_empty()) {
			self.pw.toggle_sink_mute(sink);
			let (vol, mute) = self.pw.sink_state(sink).unwrap_or((0.0, false));
			return self.render(instance, settings, vol, !mute).await;
		}
		Ok(())
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.set_device(&instance.instance_id, settings);
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn will_disappear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.forget_device(&instance.instance_id);
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.set_device(&instance.instance_id, settings);
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn property_inspector_did_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		instance
			.send_to_property_inspector(SinksMessage {
				event: "sinks",
				sinks: self.pw.sinks(),
			})
			.await
	}
}

impl DeviceVolumeAction {
	fn state(&self, settings: &DeviceVolumeSettings) -> (f32, bool) {
		settings
			.sink
			.as_deref()
			.and_then(|s| self.pw.sink_state(s))
			.unwrap_or((0.0, false))
	}

	async fn adjust(
		&self,
		instance: &Instance,
		settings: &DeviceVolumeSettings,
		delta_cubic: f32,
	) -> OpenActionResult<()> {
		let Some(sink) = settings.sink.as_deref().filter(|s| !s.is_empty()) else {
			return Ok(());
		};
		self.pw.adjust_sink_volume(sink, delta_cubic);
		// Optimistic render so the bar updates instantly (the real Props event
		// will reconcile a moment later).
		let (cur, mute) = self.pw.sink_state(sink).unwrap_or((0.0, false));
		let next = (cur + delta_cubic).clamp(0.0, 1.5);
		self.render(instance, settings, next, mute).await
	}

	async fn render(
		&self,
		instance: &Instance,
		settings: &DeviceVolumeSettings,
		volume_cubic: f32,
		mute: bool,
	) -> OpenActionResult<()> {
		instance
			.set_image(
				Some(crate::render::device_key(
					&label(settings),
					volume_cubic,
					mute,
				)),
				None,
			)
			.await
	}
}
