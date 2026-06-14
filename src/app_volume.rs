//! App Volume action: controls the volume (and, on dial press, mute) of every
//! audio stream belonging to a chosen application.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::{AppDesc, PwHandle};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppVolumeSettings {
	/// `application.name` to control (e.g. "Firefox", "spotify").
	pub app: Option<String>,
	/// Step per key press / dial tick, in percent (clamped to 1–20).
	pub step: u8,
	/// For a Keypad button: direction. `true` = up, `false` = down.
	pub up: bool,
}

impl Default for AppVolumeSettings {
	fn default() -> Self {
		Self {
			app: None,
			step: 5,
			up: true,
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
}

#[async_trait]
impl Action for AppVolumeAction {
	const UUID: &'static str = "fr.jourdois.pipewire.appvolume";
	type Settings = AppVolumeSettings;

	async fn key_down(
		&self,
		_instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		if let Some(app) = settings.app.as_deref().filter(|s| !s.is_empty()) {
			let step = settings.step.clamp(1, 20) as f32 / 100.0;
			let delta = if settings.up { step } else { -step };
			self.pw.adjust_app_volume(app, delta);
		}
		Ok(())
	}

	async fn dial_rotate(
		&self,
		_instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		if let Some(app) = settings.app.as_deref().filter(|s| !s.is_empty()) {
			let step = settings.step.clamp(1, 20) as f32 / 100.0;
			self.pw.adjust_app_volume(app, ticks as f32 * step);
		}
		Ok(())
	}

	async fn dial_down(
		&self,
		_instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		if let Some(app) = settings.app.as_deref().filter(|s| !s.is_empty()) {
			self.pw.toggle_app_mute(app);
		}
		Ok(())
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		match settings.app.as_deref().filter(|s| !s.is_empty()) {
			Some(app) => {
				instance
					.set_image(Some(crate::render::app_key(app)), None)
					.await
			}
			None => instance.set_title(Some("App"), None).await,
		}
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.will_appear(instance, settings).await
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
