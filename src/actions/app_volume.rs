//! Output App Volume action: controls the volume (and, on dial press, mute) of every
//! audio stream belonging to a chosen application.
//!
//! The key/encoder shows the app name and a live level bar aggregated across the
//! app's streams (mean volume; muted only when every stream is).

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::color::BarColors;
use crate::command::Command;
use crate::pw::{AppDesc, PwHandle};
use crate::refresh::Refresher;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppVolumeSettings {
	/// `application.name` to control (e.g. "Firefox", "spotify").
	pub app: Option<String>,
	/// Step per key press / dial tick, in percent (clamped to 1–20).
	pub step: u8,
	/// What a Keypad press does: volume up, down, or toggle mute.
	pub mode: super::KeyMode,
	/// User-configurable level-bar colours (unmuted / muted).
	#[serde(flatten)]
	pub colors: BarColors,
}

impl Default for AppVolumeSettings {
	fn default() -> Self {
		Self {
			app: None,
			step: 5,
			mode: super::KeyMode::Up,
			colors: BarColors::default(),
		}
	}
}

#[derive(Serialize)]
struct AppsMessage {
	event: &'static str,
	apps: Vec<AppDesc>,
}

pub struct AppVolumeAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
}

#[async_trait]
impl Action for AppVolumeAction {
	const UUID: &'static str = super::action_uuid!("appvolume");
	type Settings = AppVolumeSettings;

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
		self.refresher.set_app(&instance.instance_id, settings);
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn will_disappear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.forget_app(&instance.instance_id);
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.set_app(&instance.instance_id, settings);
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn property_inspector_did_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		instance
			.send_to_property_inspector(AppsMessage {
				event: "apps",
				apps: self.pw.apps(),
			})
			.await
	}
}

impl AppVolumeAction {
	/// The chosen app's name, if one is set and non-empty.
	fn app<'a>(&self, settings: &'a AppVolumeSettings) -> Option<&'a str> {
		settings.app.as_deref().filter(|s| !s.is_empty())
	}

	/// Live aggregate (volume_cubic, mute) of the chosen app's streams, or
	/// (0, false) when none is set or it isn't currently playing.
	fn state(&self, settings: &AppVolumeSettings) -> (f32, bool) {
		self.app(settings)
			.and_then(|a| self.pw.app_state(a))
			.unwrap_or((0.0, false))
	}

	async fn adjust(
		&self,
		instance: &Instance,
		settings: &AppVolumeSettings,
		delta_cubic: f32,
	) -> OpenActionResult<()> {
		let Some(app) = self.app(settings) else {
			return Ok(());
		};
		let (cur, mute) = self.pw.app_state(app).unwrap_or((0.0, false));
		// Adjusting the volume of a muted app unmutes it.
		if mute {
			self.pw
				.send(Command::SetAppMute(app.to_owned(), Some(false)));
		}
		self.pw
			.send(Command::AdjustAppVolume(app.to_owned(), delta_cubic));
		// Optimistic render so the bar moves instantly (the real Props event will
		// reconcile a moment later).
		let next = (cur + delta_cubic).clamp(0.0, 1.5);
		self.render(instance, settings, next, false).await
	}

	async fn toggle_mute(
		&self,
		instance: &Instance,
		settings: &AppVolumeSettings,
	) -> OpenActionResult<()> {
		let Some(app) = self.app(settings) else {
			return Ok(());
		};
		self.pw.send(Command::SetAppMute(app.to_owned(), None));
		// The command is applied asynchronously, so render the flipped state
		// optimistically (the Props event will reconcile a moment later).
		let (vol, mute) = self.pw.app_state(app).unwrap_or((0.0, false));
		self.render(instance, settings, vol, !mute).await
	}

	async fn render(
		&self,
		instance: &Instance,
		settings: &AppVolumeSettings,
		vol: f32,
		mute: bool,
	) -> OpenActionResult<()> {
		crate::display::app(instance, self.app(settings), vol, mute, &settings.colors).await
	}
}
