//! Device names chosen by the user.
//!
//! A name belongs to the device, not to a key: renaming the headset once shows
//! the new name on every key that names it. So the names live in the plugin's
//! global settings, keyed by `node.name`. The plugin is their only writer: a
//! property inspector sends a rename, the plugin stores it, saves the global
//! settings and redraws the keys.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::pw::{PwHandle, SinkDesc};
use crate::refresh::Refresher;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GlobalSettings {
	/// `node.name` -> the user's name for that device.
	pub names: HashMap<String, String>,
}

impl GlobalSettings {
	/// Give `device` the name `name`; a blank name gives it back its own.
	pub fn rename(&mut self, device: &str, name: &str) {
		let name = name.trim();
		if name.is_empty() {
			self.names.remove(device);
		} else {
			self.names.insert(device.to_owned(), name.to_owned());
		}
	}

	/// Drop blank names and trim the others, so a hand-edited settings file
	/// reads the same as one this plugin wrote.
	fn normalize(&mut self) {
		self.names = std::mem::take(&mut self.names)
			.into_iter()
			.map(|(device, name)| (device, name.trim().to_owned()))
			.filter(|(device, name)| !device.is_empty() && !name.is_empty())
			.collect();
	}
}

/// The global settings as last received from OpenDeck or set by this plugin.
static GLOBAL: LazyLock<Mutex<GlobalSettings>> =
	LazyLock::new(|| Mutex::new(GlobalSettings::default()));

/// Every name the user has given, for the property inspectors.
pub fn all() -> HashMap<String, String> {
	GLOBAL.lock().unwrap().names.clone()
}

pub fn store(mut settings: GlobalSettings) {
	settings.normalize();
	log::info!("device names: {:?}", settings.names);
	*GLOBAL.lock().unwrap() = settings;
}

/// What a property inspector sends the plugin about device names.
#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
enum InspectorMessage {
	/// Give `device` the name `name`; blank gives it back its own.
	SetName { device: String, name: String },
}

/// Handle a message from a device list's property inspector.
pub async fn inspector_message(payload: &serde_json::Value, pw: &PwHandle, refresher: &Refresher) {
	match serde_json::from_value(payload.clone()) {
		Ok(InspectorMessage::SetName { device, name }) => {
			rename(&device, &name, pw, refresher).await
		}
		Err(error) => log::warn!("unexpected message from the property inspector: {error}"),
	}
}

/// Rename `device`, save the global settings and redraw every key.
async fn rename(device: &str, name: &str, pw: &PwHandle, refresher: &Refresher) {
	let global = {
		let mut global = GLOBAL.lock().unwrap();
		global.rename(device, name);
		global.clone()
	};
	log::info!("device names now {:?}", global.names);
	if let Err(error) = openaction::set_global_settings(&global).await {
		log::warn!("could not save the device names: {error}");
	}
	crate::refresh::refresh_all(pw, refresher).await;
}

/// What a key calls a sink: the user's name for it, else its description, else
/// its `node.name`.
pub fn sink(pw: &PwHandle, device: &str) -> String {
	label(device, &pw.sinks())
}

/// What a key calls a source, as [`sink`] does a sink.
pub fn source(pw: &PwHandle, device: &str) -> String {
	label(device, &pw.sources())
}

fn label(device: &str, nodes: &[SinkDesc]) -> String {
	if let Some(name) = GLOBAL.lock().unwrap().names.get(device) {
		return name.clone();
	}
	nodes
		.iter()
		.find(|n| n.name == device)
		.map(|n| n.description.clone())
		.unwrap_or_else(|| device.to_owned())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rename_and_reset() {
		let mut g = GlobalSettings::default();
		g.rename("alsa_output.usb.a50", "  Casque ");
		assert_eq!(g.names["alsa_output.usb.a50"], "Casque");
		g.rename("alsa_output.usb.a50", "   ");
		assert!(g.names.is_empty());
	}

	#[test]
	fn global_payload() {
		let mut g: GlobalSettings = serde_json::from_str(
			r#"{"names":{"alsa_output.usb.a50":"Casque","alsa_input.usb.a50":" "}}"#,
		)
		.unwrap();
		g.normalize();
		assert_eq!(g.names.len(), 1);
		assert_eq!(g.names["alsa_output.usb.a50"], "Casque");
		let empty: GlobalSettings = serde_json::from_str("{}").unwrap();
		assert!(empty.names.is_empty());
	}
}
