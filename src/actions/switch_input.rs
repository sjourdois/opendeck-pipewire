//! Input Device action: switches the system default input device (source).
//!
//! Holds a list of sources chosen in the property inspector; each press cycles the
//! system default to the next one in the list. With a single source it simply
//! switches to it. The key shows the user's custom title, or — when empty — the
//! current default input device's name.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::pw::{PwHandle, SinkDesc};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct SwitchInputSettings {
	/// `node.name`s of the sources to cycle through.
	pub sources: Vec<String>,
	/// Custom key title. `None` = show the current default input's name.
	pub title: Option<String>,
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
	const UUID: &'static str = super::action_uuid!("switchinput");
	type Settings = SwitchInputSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		match self.next(settings) {
			Some(next) => {
				self.pw.send(Command::SetDefaultSource(next.clone()));
				self.show(instance, settings, Some(next)).await
			}
			None => self.show(instance, settings, None).await,
		}
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.key_down(instance, settings).await
	}

	async fn touch_tap(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		_position: (u16, u16),
		_hold: bool,
	) -> OpenActionResult<()> {
		self.key_down(instance, settings).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.show(instance, settings, None).await
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.show(instance, settings, None).await
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

/// The configured sources, dropping empty entries.
fn chosen(settings: &SwitchInputSettings) -> Vec<&str> {
	settings
		.sources
		.iter()
		.map(String::as_str)
		.filter(|s| !s.is_empty())
		.collect()
}

impl SwitchInputAction {
	/// The next source to switch to: the one after the current default in the list,
	/// wrapping around; the first if the default isn't in the list (or unknown);
	/// `None` when the list is empty.
	fn next(&self, settings: &SwitchInputSettings) -> Option<String> {
		let list = chosen(settings);
		if list.is_empty() {
			return None;
		}
		let current = self.pw.default_source_name();
		let idx = current
			.as_deref()
			.and_then(|c| list.iter().position(|&s| s == c));
		Some(list[idx.map_or(0, |i| (i + 1) % list.len())].to_owned())
	}

	/// A source's friendly description, falling back to its `node.name`.
	fn describe(&self, name: &str) -> String {
		self.pw
			.sources()
			.into_iter()
			.find(|s| s.name == name)
			.map(|s| s.description)
			.unwrap_or_else(|| name.to_owned())
	}

	/// Set the key title: the custom title if set, else the given source (used
	/// optimistically right after a switch) or the current default source's
	/// description, else the "Input" placeholder.
	async fn show(
		&self,
		instance: &Instance,
		settings: &SwitchInputSettings,
		source: Option<String>,
	) -> OpenActionResult<()> {
		let title = match &settings.title {
			Some(t) => t.clone(),
			None => match source.or_else(|| self.pw.default_source_name()) {
				Some(n) => self.describe(&n),
				None => "Input".to_owned(),
			},
		};
		crate::display::picker(instance, &title).await
	}
}
