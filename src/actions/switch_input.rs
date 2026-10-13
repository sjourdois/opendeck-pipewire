//! Input Device action: switches the system default input device (source).
//!
//! Holds a list of sources chosen in the property inspector; each press cycles the
//! system default to the next one in the list. With a single source it simply
//! switches to it. The key shows the user's custom title, or — when empty — the
//! current default input device's name, and the active source's custom icon
//! once one is set.

use std::collections::HashMap;

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::pw::{PwHandle, SinkDesc};
use crate::refresh::Refresher;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct SwitchInputSettings {
	/// `node.name`s of the sources to cycle through.
	pub sources: Vec<String>,
	/// Custom key title. `None` = show the current default input's name.
	pub title: Option<String>,
	/// Per-source custom icon (`node.name` -> data URI); the key shows the
	/// active source's icon. As on the Output Device key, the first icon set
	/// hands the key image over to the plugin (see [`crate::actions::output`]).
	pub icons: HashMap<String, String>,
}

#[derive(Serialize)]
struct SourcesMessage {
	event: &'static str,
	sources: Vec<SinkDesc>,
	names: HashMap<String, String>,
}

pub struct SwitchInputAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
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
		let _event = crate::trace::Event::start(instance, "key_down");
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
		let _event = crate::trace::Event::start(instance, "dial_down");
		self.key_down(instance, settings).await
	}

	async fn touch_tap(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		_position: (u16, u16),
		_hold: bool,
	) -> OpenActionResult<()> {
		let _event = crate::trace::Event::start(instance, "touch_tap");
		self.key_down(instance, settings).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher
			.set_switch_input(&instance.instance_id, settings);
		self.show(instance, settings, None).await
	}

	async fn will_disappear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.forget_switch_input(&instance.instance_id);
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher
			.set_switch_input(&instance.instance_id, settings);
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
				names: crate::names::all(),
			})
			.await
	}

	async fn send_to_plugin(
		&self,
		_instance: &Instance,
		_settings: &Self::Settings,
		payload: &serde_json::Value,
	) -> OpenActionResult<()> {
		crate::names::inspector_message(payload, &self.pw, &self.refresher).await;
		Ok(())
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

/// What the key should show: its title, and its image — `None` while no icon is
/// set, leaving OpenDeck's own.
pub struct Surface {
	pub title: String,
	pub image: Option<String>,
}

/// Resolve the key surface for `source` (the source just switched to, used
/// optimistically), else the current default source. Shared by the action's own
/// redraws and the background [`Refresher`].
pub fn surface(settings: &SwitchInputSettings, pw: &PwHandle, source: Option<String>) -> Surface {
	let current = source.or_else(|| pw.default_source_name());
	let title = match &settings.title {
		Some(t) => t.clone(),
		None => match &current {
			Some(n) => crate::names::source(pw, n),
			None => "Input".to_owned(),
		},
	};
	let image = (!settings.icons.is_empty()).then(|| {
		current
			.as_deref()
			.and_then(|c| settings.icons.get(c).cloned())
			.unwrap_or_else(crate::render::switch_input_icon)
	});
	Surface { title, image }
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

	/// Draw the key for `source` (see [`surface`]).
	async fn show(
		&self,
		instance: &Instance,
		settings: &SwitchInputSettings,
		source: Option<String>,
	) -> OpenActionResult<()> {
		let s = surface(settings, &self.pw, source);
		crate::display::picker(instance, &s.title, s.image.as_deref()).await
	}
}
