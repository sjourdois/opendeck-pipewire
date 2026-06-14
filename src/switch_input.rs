//! Switch Input Device action: sets the system default input device (source).

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::{PwHandle, SinkDesc};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct SwitchInputSettings {
	/// `node.name` of the source to switch to.
	pub source: Option<String>,
}

#[derive(Serialize)]
struct SourcesMessage {
	event: &'static str,
	sources: Vec<SinkDesc>,
}

pub struct SwitchInputAction {
	pub pw: PwHandle,
}

#[async_trait]
impl Action for SwitchInputAction {
	const UUID: &'static str = "fr.jourdois.pipewire.switchinput";
	type Settings = SwitchInputSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		if let Some(source) = settings.source.as_deref().filter(|s| !s.is_empty()) {
			self.pw.set_default_source(source);
		}
		self.refresh_title(instance, settings).await
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.key_down(instance, settings).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresh_title(instance, settings).await
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresh_title(instance, settings).await
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

impl SwitchInputAction {
	async fn refresh_title(
		&self,
		instance: &Instance,
		settings: &SwitchInputSettings,
	) -> OpenActionResult<()> {
		let title = settings
			.source
			.as_deref()
			.and_then(|name| self.pw.sources().into_iter().find(|s| s.name == name))
			.map(|s| s.description)
			.unwrap_or_else(|| "Input".to_owned());
		instance.set_title(Some(title), None).await
	}
}
