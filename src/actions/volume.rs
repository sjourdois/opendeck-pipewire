//! Output Volume action: adjusts the default sink (output) volume.
//!
//! - Keypad: each press nudges the volume up or down by `step`.
//! - Encoder: each tick changes the volume by `step`; pressing the dial mutes.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::color::BarColors;
use crate::command::Command;
use crate::pw::PwHandle;
use crate::refresh::Refresher;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VolumeSettings {
	/// Step per key press / dial tick, in percent (clamped to 1–20). Mirrors the
	/// "Configurable Volume Step" slider of the original PulseAudio plugin.
	pub step: u8,
	/// What a Keypad press does: volume up, down, or toggle mute.
	pub mode: super::KeyMode,
	/// Custom key title. `None` = "Output".
	pub title: Option<String>,
	/// User-configurable level-bar colours (unmuted / muted).
	#[serde(flatten)]
	pub colors: BarColors,
}

impl Default for VolumeSettings {
	fn default() -> Self {
		Self {
			step: 5,
			mode: super::KeyMode::Up,
			title: None,
			colors: BarColors::default(),
		}
	}
}

/// The key title: the custom title if set, else the current default output
/// device's description (falling back to "Output" when there is no default).
pub fn resolve_title(custom: &Option<String>, pw: &PwHandle) -> String {
	match custom {
		Some(t) => t.clone(),
		None => match pw.default_sink_name() {
			Some(n) => pw
				.sinks()
				.into_iter()
				.find(|s| s.name == n)
				.map(|s| s.description)
				.unwrap_or(n),
			None => "Output".to_owned(),
		},
	}
}

pub struct VolumeAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
}

#[async_trait]
impl Action for VolumeAction {
	const UUID: &'static str = super::action_uuid!("volume");
	type Settings = VolumeSettings;

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
		// Pressing the encoder toggles mute on the default sink.
		self.toggle_mute(instance, settings).await
	}

	async fn touch_tap(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		_position: (u16, u16),
		_hold: bool,
	) -> OpenActionResult<()> {
		// Tapping the touchstrip mirrors the dial press: toggle mute.
		self.toggle_mute(instance, settings).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher
			.set_colors(&instance.instance_id, &settings.colors);
		self.refresher
			.set_default_title(&instance.instance_id, settings.title.clone());
		self.refresh(instance, settings).await?;
		crate::display::title(instance, &resolve_title(&settings.title, &self.pw)).await
	}

	async fn will_disappear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.forget_colors(&instance.instance_id);
		self.refresher.forget_default_title(&instance.instance_id);
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher
			.set_colors(&instance.instance_id, &settings.colors);
		self.refresher
			.set_default_title(&instance.instance_id, settings.title.clone());
		self.refresh(instance, settings).await?;
		crate::display::title(instance, &resolve_title(&settings.title, &self.pw)).await
	}
}

impl VolumeAction {
	/// Nudge the default sink's volume. Adjusting a muted device unmutes it.
	async fn adjust(
		&self,
		instance: &Instance,
		settings: &VolumeSettings,
		delta_cubic: f32,
	) -> OpenActionResult<()> {
		if self.pw.default_sink_snapshot().mute {
			self.pw.send(Command::SetMute(Some(false)));
		}
		self.pw.send(Command::AdjustVolume(delta_cubic));
		self.refresh(instance, settings).await
	}

	/// Toggle the default sink's mute, from the displayed state, rendered
	/// optimistically so the bar flips instantly (the Props event reconciles later).
	async fn toggle_mute(
		&self,
		instance: &Instance,
		settings: &VolumeSettings,
	) -> OpenActionResult<()> {
		let snap = self.pw.default_sink_snapshot();
		let target = !snap.mute;
		self.pw.send(Command::SetMute(Some(target)));
		crate::display::volume(
			instance,
			snap.known,
			snap.volume_cubic,
			target,
			&settings.colors,
		)
		.await
	}

	/// Render the current level onto the key or touchstrip.
	async fn refresh(
		&self,
		instance: &Instance,
		settings: &VolumeSettings,
	) -> OpenActionResult<()> {
		let snap = self.pw.default_sink_snapshot();
		crate::display::volume(
			instance,
			snap.known,
			snap.volume_cubic,
			snap.mute,
			&settings.colors,
		)
		.await
	}
}
