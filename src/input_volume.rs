//! Input Volume action: controls the volume of a specific input device (source),
//! identified by its `node.name`. Mirrors the original plugin's "Input Volume".

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::{PwHandle, SinkDesc};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct InputVolumeSettings {
	/// `node.name` of the target source.
	pub source: Option<String>,
	/// Custom label shown on the key.
	pub name: String,
	/// Step per press / tick, in percent.
	pub step: u8,
	/// For a Keypad button: direction. `true` = up, `false` = down.
	pub up: bool,
}

impl Default for InputVolumeSettings {
	fn default() -> Self {
		Self {
			source: None,
			name: String::new(),
			step: 5,
			up: true,
		}
	}
}

#[derive(Serialize)]
struct SourcesMessage {
	event: &'static str,
	sources: Vec<SinkDesc>,
}

pub struct InputVolumeAction {
	pub pw: PwHandle,
}

#[async_trait]
impl Action for InputVolumeAction {
	const UUID: &'static str = "fr.jourdois.pipewire.inputvolume";
	type Settings = InputVolumeSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let step = settings.step.clamp(1, 20) as f32 / 100.0;
		self.adjust(instance, settings, if settings.up { step } else { -step })
			.await
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
		if let Some(source) = settings.source.as_deref().filter(|s| !s.is_empty()) {
			self.pw.toggle_source_mute(source);
			let (vol, mute) = self.pw.source_state(source).unwrap_or((0.0, false));
			return self.render(instance, settings, vol, !mute).await;
		}
		Ok(())
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn property_inspector_did_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		instance
			.send_to_property_inspector(SourcesMessage {
				event: "sources",
				sources: self.pw.sources(),
			})
			.await
	}
}

impl InputVolumeAction {
	fn state(&self, settings: &InputVolumeSettings) -> (f32, bool) {
		settings
			.source
			.as_deref()
			.and_then(|s| self.pw.source_state(s))
			.unwrap_or((0.0, false))
	}

	async fn adjust(
		&self,
		instance: &Instance,
		settings: &InputVolumeSettings,
		delta_cubic: f32,
	) -> OpenActionResult<()> {
		let Some(source) = settings.source.as_deref().filter(|s| !s.is_empty()) else {
			return Ok(());
		};
		self.pw.adjust_source_volume(source, delta_cubic);
		let (cur, mute) = self.pw.source_state(source).unwrap_or((0.0, false));
		let next = (cur + delta_cubic).clamp(0.0, 1.5);
		self.render(instance, settings, next, mute).await
	}

	async fn render(
		&self,
		instance: &Instance,
		settings: &InputVolumeSettings,
		volume_cubic: f32,
		mute: bool,
	) -> OpenActionResult<()> {
		let label = if settings.name.is_empty() {
			settings
				.source
				.clone()
				.unwrap_or_else(|| "Input".to_owned())
		} else {
			settings.name.clone()
		};
		instance
			.set_image(
				Some(crate::render::device_key(&label, volume_cubic, mute)),
				None,
			)
			.await
	}
}
