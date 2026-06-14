//! Output action: switches the system default output device (sink).
//!
//! The target sink is chosen in the property inspector, which is populated with
//! the live sink list pushed from the plugin.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::{PwHandle, SinkDesc};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct OutputSettings {
	/// `node.name` of the sink to switch to.
	pub sink: Option<String>,
}

#[derive(Serialize)]
struct SinksMessage {
	event: &'static str,
	sinks: Vec<SinkDesc>,
}

pub struct OutputAction {
	pub pw: PwHandle,
}

#[async_trait]
impl Action for OutputAction {
	const UUID: &'static str = "fr.jourdois.pipewire.output";
	type Settings = OutputSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		if let Some(sink) = settings.sink.as_deref().filter(|s| !s.is_empty()) {
			self.pw.set_default_sink(sink);
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
		// Give the property inspector the current list of sinks to choose from.
		instance
			.send_to_property_inspector(SinksMessage {
				event: "sinks",
				sinks: self.pw.sinks(),
			})
			.await
	}
}

impl OutputAction {
	async fn refresh_title(
		&self,
		instance: &Instance,
		settings: &OutputSettings,
	) -> OpenActionResult<()> {
		let title = settings
			.sink
			.as_deref()
			.and_then(|name| self.pw.sinks().into_iter().find(|s| s.name == name))
			.map(|s| s.description)
			.unwrap_or_else(|| "Output".to_owned());
		instance.set_title(Some(title), None).await
	}
}
