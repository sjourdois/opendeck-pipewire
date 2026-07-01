//! Input Device Volume action: controls the volume of a specific input device (source),
//! identified by its `node.name`. Mirrors the original plugin's "Input Volume".

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::color::BarColors;
use crate::command::Command;
use crate::pw::{PwHandle, SinkDesc};
use crate::refresh::Refresher;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct InputVolumeSettings {
	/// `node.name` of the target source.
	pub source: Option<String>,
	/// Custom label shown on the key.
	pub name: String,
	/// Step per press / tick, in percent.
	pub step: u8,
	/// What a Keypad press does: volume up, down, or toggle mute.
	pub mode: super::KeyMode,
	/// User-configurable level-bar colours (unmuted / muted).
	#[serde(flatten)]
	pub colors: BarColors,
}

impl Default for InputVolumeSettings {
	fn default() -> Self {
		Self {
			source: None,
			name: String::new(),
			step: 5,
			mode: super::KeyMode::Up,
			colors: BarColors::default(),
		}
	}
}

#[derive(Serialize)]
struct SourcesMessage {
	event: &'static str,
	sources: Vec<SinkDesc>,
}

/// The key label: the custom name if set, else the source's friendly description
/// (never the raw `node.name`), else "Input".
pub fn label(settings: &InputVolumeSettings, pw: &PwHandle) -> String {
	if !settings.name.is_empty() {
		return settings.name.clone();
	}
	match settings.source.as_deref() {
		Some(name) => pw
			.sources()
			.into_iter()
			.find(|s| s.name == name)
			.map(|s| s.description)
			.unwrap_or_else(|| name.to_owned()),
		None => "Input".to_owned(),
	}
}

pub struct InputVolumeAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
}

#[async_trait]
impl Action for InputVolumeAction {
	const UUID: &'static str = super::action_uuid!("inputvolume");
	type Settings = InputVolumeSettings;

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
		self.refresher.set_input(&instance.instance_id, settings);
		let (vol, mute) = self.state(settings);
		self.render(instance, settings, vol, mute).await
	}

	async fn will_disappear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.forget_input(&instance.instance_id);
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.set_input(&instance.instance_id, settings);
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
		let (cur, mute) = self.pw.source_state(source).unwrap_or((0.0, false));
		// Adjusting the volume of a muted device unmutes it.
		if mute {
			self.pw
				.send(Command::SetSourceMute(source.to_owned(), Some(false)));
		}
		self.pw
			.send(Command::AdjustSourceVolume(source.to_owned(), delta_cubic));
		let next = (cur + delta_cubic).clamp(0.0, 1.5);
		self.render(instance, settings, next, false).await
	}

	async fn toggle_mute(
		&self,
		instance: &Instance,
		settings: &InputVolumeSettings,
	) -> OpenActionResult<()> {
		if let Some(source) = settings.source.as_deref().filter(|s| !s.is_empty()) {
			self.pw
				.send(Command::SetSourceMute(source.to_owned(), None));
			// The command is applied asynchronously, so render the flipped state
			// optimistically (the Props event will reconcile a moment later).
			let (vol, mute) = self.pw.source_state(source).unwrap_or((0.0, false));
			return self.render(instance, settings, vol, !mute).await;
		}
		Ok(())
	}

	async fn render(
		&self,
		instance: &Instance,
		settings: &InputVolumeSettings,
		volume_cubic: f32,
		mute: bool,
	) -> OpenActionResult<()> {
		crate::display::input(
			instance,
			&label(settings, &self.pw),
			&settings.name,
			volume_cubic,
			mute,
			&settings.colors,
		)
		.await
	}
}
