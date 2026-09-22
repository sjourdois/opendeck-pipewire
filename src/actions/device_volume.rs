//! Output Device Volume action: controls the volume of a specific output device (sink),
//! identified by its `node.name`. Mirrors the original plugin's "Output Volume".
//!
//! - Keypad: each press nudges that device's volume up/down by `step`.
//! - Encoder: each tick changes it by `step`; pressing the dial mutes the device.
//!
//! The key/encoder shows a custom label and a live level bar.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::color::BarColors;
use crate::command::Command;
use crate::pw::{PwHandle, SinkDesc};
use crate::refresh::Refresher;
use crate::ui::VolumeUi;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct DeviceVolumeSettings {
	/// `node.name` of the target sink (== original plugin's `outputName`).
	pub sink: Option<String>,
	/// Custom label shown on the key (== original `customName`).
	pub name: String,
	/// Step per press / tick, in percent (== original `volumeStep`).
	pub step: u8,
	/// What a Keypad press does: volume up, down, or toggle mute.
	pub mode: super::KeyMode,
	/// User-configurable level-bar colours (unmuted / muted).
	#[serde(flatten)]
	pub colors: BarColors,
	/// Custom icon and 100%-limit.
	#[serde(flatten)]
	pub ui: VolumeUi,
}

impl Default for DeviceVolumeSettings {
	fn default() -> Self {
		Self {
			sink: None,
			name: String::new(),
			step: 5,
			mode: super::KeyMode::Up,
			colors: BarColors::default(),
			ui: VolumeUi::default(),
		}
	}
}

#[derive(Serialize)]
struct SinksMessage {
	event: &'static str,
	sinks: Vec<SinkDesc>,
}

/// The key label: the custom name if set, else the sink's friendly description
/// (never the raw `node.name`), else "Device".
pub fn label(settings: &DeviceVolumeSettings, pw: &PwHandle) -> String {
	if !settings.name.is_empty() {
		return settings.name.clone();
	}
	match settings.sink.as_deref() {
		Some(name) => pw
			.sinks()
			.into_iter()
			.find(|s| s.name == name)
			.map(|s| s.description)
			.unwrap_or_else(|| name.to_owned()),
		None => "Device".to_owned(),
	}
}

pub struct DeviceVolumeAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
}

#[async_trait]
impl Action for DeviceVolumeAction {
	const UUID: &'static str = super::action_uuid!("devicevolume");
	type Settings = DeviceVolumeSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let step = super::step_fraction(settings.step);
		match settings.mode {
			super::KeyMode::Up => self.adjust(instance, settings, step).await,
			super::KeyMode::Down => self.adjust(instance, settings, -step).await,
			super::KeyMode::Mute => self.toggle_mute(instance, settings).await,
		}
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let step = super::step_fraction(settings.step);
		self.adjust(instance, settings, ticks as f32 * step).await
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.toggle_mute(instance, settings).await
	}

	async fn touch_tap(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		_position: (u16, u16),
		_hold: bool,
	) -> OpenActionResult<()> {
		self.toggle_mute(instance, settings).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.set_device(&instance.instance_id, settings);
		crate::display::encoder_layout(instance).await?;
		let (vol, mute) = self.state(settings).unwrap_or((0.0, false));
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
		let (vol, mute) = self.state(settings).unwrap_or((0.0, false));
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
	/// Live (volume_cubic, mute) of the configured sink, or `None` when no sink
	/// is configured or it isn't currently available.
	fn state(&self, settings: &DeviceVolumeSettings) -> Option<(f32, bool)> {
		settings
			.sink
			.as_deref()
			.filter(|s| !s.is_empty())
			.and_then(|s| self.pw.sink_state(s))
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
		let (cur, mute) = self.pw.sink_state(sink).unwrap_or((0.0, false));
		// Adjusting the volume of a muted device unmutes it.
		if mute {
			self.pw
				.send(Command::SetSinkMute(sink.to_owned(), Some(false)));
		}
		let delta = settings.ui.limit_delta(cur, delta_cubic);
		self.pw
			.send(Command::AdjustSinkVolume(sink.to_owned(), delta));
		// Optimistic render so the bar updates instantly (the real Props event
		// will reconcile a moment later).
		let next = (cur + delta).clamp(0.0, settings.ui.max_cubic());
		self.render(instance, settings, next, false).await
	}

	async fn toggle_mute(
		&self,
		instance: &Instance,
		settings: &DeviceVolumeSettings,
	) -> OpenActionResult<()> {
		if let Some(sink) = settings.sink.as_deref().filter(|s| !s.is_empty()) {
			self.pw.send(Command::SetSinkMute(sink.to_owned(), None));
			// The command is applied asynchronously, so render the flipped state
			// optimistically (the Props event will reconcile a moment later).
			let (vol, mute) = self.pw.sink_state(sink).unwrap_or((0.0, false));
			return self.render(instance, settings, vol, !mute).await;
		}
		Ok(())
	}

	async fn render(
		&self,
		instance: &Instance,
		settings: &DeviceVolumeSettings,
		volume_cubic: f32,
		mute: bool,
	) -> OpenActionResult<()> {
		// `known` re-resolves the live state so a device that vanished (or was
		// never configured) renders "n/a" instead of a stale level.
		let known = self.state(settings).is_some();
		crate::display::device(
			instance,
			&label(settings, &self.pw),
			known,
			volume_cubic,
			mute,
			&settings.colors,
			&settings.ui,
		)
		.await
	}
}
