//! Output Device action: switches the system default output device (sink).
//!
//! Holds a list of sinks chosen in the property inspector; each press cycles the
//! system default to the next one in the list. With a single sink it simply
//! switches to it (the original one-device behaviour).

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::pw::{PwHandle, SinkDesc};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct OutputSettings {
	/// `node.name`s of the sinks to cycle through.
	pub sinks: Vec<String>,
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
	const UUID: &'static str = super::action_uuid!("output");
	type Settings = OutputSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		let list = chosen(settings);
		let Some(next) = self.next(&list) else {
			return self.show(instance, None).await;
		};
		self.pw.send(Command::SetDefaultSink(next.to_owned()));
		self.show(instance, Some(next)).await
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

/// The configured sinks, dropping empty entries.
fn chosen(settings: &OutputSettings) -> Vec<&str> {
	settings
		.sinks
		.iter()
		.map(String::as_str)
		.filter(|s| !s.is_empty())
		.collect()
}

impl OutputAction {
	/// The next sink to switch to: the one after the current default in the list,
	/// wrapping around; the first if the default isn't in the list (or unknown).
	fn next<'a>(&self, list: &[&'a str]) -> Option<&'a str> {
		if list.is_empty() {
			return None;
		}
		let current = self.pw.default_sink_name();
		let idx = current
			.as_deref()
			.and_then(|c| list.iter().position(|&s| s == c));
		Some(list[idx.map_or(0, |i| (i + 1) % list.len())])
	}

	/// The `node.name` shown at rest: the current default if it is one of the
	/// chosen sinks, else the first chosen sink.
	async fn refresh_title(
		&self,
		instance: &Instance,
		settings: &OutputSettings,
	) -> OpenActionResult<()> {
		let list = chosen(settings);
		let current = self.pw.default_sink_name();
		let shown = current
			.as_deref()
			.filter(|c| list.contains(c))
			.or_else(|| list.first().copied());
		self.show(instance, shown).await
	}

	/// Set the key title to a sink's description (falls back to its name, then
	/// "Output" when nothing is chosen).
	async fn show(&self, instance: &Instance, name: Option<&str>) -> OpenActionResult<()> {
		let title = match name {
			Some(n) => self
				.pw
				.sinks()
				.into_iter()
				.find(|s| s.name == n)
				.map(|s| s.description)
				.unwrap_or_else(|| n.to_owned()),
			None => "Output".to_owned(),
		};
		crate::display::picker(instance, &title).await
	}
}
