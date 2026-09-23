//! Output Device action: switches the system default output device (sink).
//!
//! Holds a list of sinks chosen in the property inspector; each press cycles the
//! system default to the next one in the list. With a single sink it simply
//! switches to it (the original one-device behaviour).
//!
//! When the *current* default is not one of the chosen sinks, the behaviour is
//! configurable (`when_inactive`): either **cycle** into the list on the next
//! press (the default), or **disable** — the key greys out and a press does
//! nothing until an external change (another app, wpctl, media keys…) brings
//! the default back into the list. The key reflects the live default either way.

use std::collections::HashMap;

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::pw::{PwHandle, SinkDesc};
use crate::refresh::Refresher;

/// What the key does when the current default sink is not one of the chosen ones.
#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WhenInactive {
	/// Cycle into the list on the next press (original behaviour).
	#[default]
	Cycle,
	/// Grey the key out; a press does nothing.
	Disable,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct OutputSettings {
	/// `node.name`s of the sinks to cycle through.
	pub sinks: Vec<String>,
	/// When set, the key cycles the `target.object` of this stream (by `node.name`)
	/// instead of the system default, so the apps feeding a virtual sink are never
	/// moved and never corked. Its targets may be sinks hidden from clients.
	pub stream: Option<String>,
	/// Behaviour when the current default isn't one of `sinks`.
	pub when_inactive: WhenInactive,
	/// Custom key title. `None` = show the current default output's name.
	pub title: Option<String>,
	/// Per-sink custom icon (`node.name` -> data URI) chosen in the property
	/// inspector; the key shows the active sink's icon.
	pub icons: HashMap<String, String>,
	/// Icon shown when the current default isn't one of the chosen sinks.
	pub inactive_icon: Option<String>,
}

#[derive(Serialize)]
struct SinksMessage {
	event: &'static str,
	sinks: Vec<SinkDesc>,
}

pub struct OutputAction {
	pub pw: PwHandle,
	pub refresher: Refresher,
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
		let _event = crate::trace::Event::start(instance, "key_down");
		if let Some(next) = self.next(settings) {
			match &settings.stream {
				Some(stream) => self.pw.send(Command::SetStreamTarget {
					stream: stream.clone(),
					target: next,
				}),
				None => self.pw.send(Command::SetDefaultSink(next)),
			}
		}
		// Reflect the resulting (or unchanged, when disabled) state.
		self.render(instance, settings).await
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
		self.refresher.set_output(&instance.instance_id, settings);
		self.render(instance, settings).await
	}

	async fn will_disappear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.forget_output(&instance.instance_id);
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresher.set_output(&instance.instance_id, settings);
		self.render(instance, settings).await
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

/// What the key reflects and cycles from: the stream's current target when the
/// key routes a stream, else the system default sink. While a stream still sits
/// on the target it was created with — which no metadata records — this falls
/// back to the first configured entry, and self-corrects on the first press.
fn current(settings: &OutputSettings, pw: &PwHandle) -> Option<String> {
	match &settings.stream {
		Some(stream) => pw
			.stream_target(stream)
			.or_else(|| chosen(settings).first().map(|s| (*s).to_owned())),
		None => pw.default_sink_name(),
	}
}

/// Whether the key applies right now, which is what `when_inactive` acts on.
///
/// Routing a stream, that is a different question from what the key cycles: the
/// targets are always the stream's own, so the key would never fall inactive if
/// they decided it. What matters is whether the sink feeding that stream is the
/// current system output — for a headset loopback, whether the headset is worn.
fn is_active(settings: &OutputSettings, pw: &PwHandle) -> bool {
	match &settings.stream {
		Some(stream) => match (pw.stream_sink(stream), pw.default_sink_name()) {
			(Some(sink), Some(default)) => sink == default,
			_ => false,
		},
		None => current(settings, pw)
			.as_deref()
			.is_some_and(|c| chosen(settings).contains(&c)),
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

/// Whether the plugin owns this key's image, rather than OpenDeck: true as soon
/// as the settings configure an icon, or ask for the greyed-out `Disable` look.
///
/// It is all or nothing, because OpenDeck keeps a plugin-pushed image in the very
/// slot its own image picker writes to, and gives no way to read it back: the
/// first image the plugin pushes replaces the user's choice for good. So the key
/// is left alone unless the settings ask for something only the plugin can draw.
fn draws_image(settings: &OutputSettings) -> bool {
	!settings.icons.is_empty()
		|| settings.inactive_icon.is_some()
		|| settings.when_inactive == WhenInactive::Disable
}

/// What the key should show, resolved from the live default and the settings.
pub struct Surface {
	/// The key title: the user's custom title, else the current default output's
	/// name (what is actually active), else the "Output" placeholder.
	pub title: String,
	/// The key image (data URI): the active sink's custom icon, the inactive icon,
	/// or the built-in output / greyed-output icon — and `None` when the plugin
	/// doesn't draw the key at all (see [`draws_image`]), leaving whatever image
	/// OpenDeck holds for it, the action's own or one the user picked.
	pub image: Option<String>,
}

/// Resolve the key surface for a set of output settings against the current
/// default. Shared by the action's own redraws and the background [`Refresher`].
pub fn surface(settings: &OutputSettings, pw: &PwHandle) -> Surface {
	let current = current(settings, pw);
	let title = match &settings.title {
		Some(t) => t.clone(),
		None => current
			.as_deref()
			.map(|c| describe(pw, c))
			.unwrap_or_else(|| "Output".to_owned()),
	};
	let active = is_active(settings, pw) || settings.when_inactive == WhenInactive::Cycle;
	let image = if !draws_image(settings) {
		None
	} else if active {
		Some(
			current
				.as_deref()
				.and_then(|c| settings.icons.get(c).cloned())
				.unwrap_or_else(crate::render::output_icon),
		)
	} else {
		Some(
			settings
				.inactive_icon
				.clone()
				.unwrap_or_else(crate::render::output_disabled_icon),
		)
	};
	Surface { title, image }
}

/// A sink's friendly description, falling back to its `node.name`.
fn describe(pw: &PwHandle, name: &str) -> String {
	pw.sinks()
		.into_iter()
		.find(|s| s.name == name)
		.map(|s| s.description)
		.unwrap_or_else(|| name.to_owned())
}

impl OutputAction {
	/// The next sink to switch to, or `None` when a press should do nothing:
	/// the one after the current default in the list (wrapping); the first when the
	/// default isn't in the list and `when_inactive == Cycle`; `None` when the list
	/// is empty or the default is outside it and `when_inactive == Disable`.
	fn next(&self, settings: &OutputSettings) -> Option<String> {
		let list = chosen(settings);
		if list.is_empty() {
			return None;
		}
		if settings.when_inactive == WhenInactive::Disable && !is_active(settings, &self.pw) {
			return None;
		}
		let current = current(settings, &self.pw);
		match current
			.as_deref()
			.and_then(|c| list.iter().position(|&s| s == c))
		{
			Some(i) => Some(list[(i + 1) % list.len()].to_owned()),
			None if settings.when_inactive == WhenInactive::Disable => None,
			None => Some(list[0].to_owned()),
		}
	}

	/// Draw the key from the current default + settings.
	async fn render(&self, instance: &Instance, settings: &OutputSettings) -> OpenActionResult<()> {
		let s = surface(settings, &self.pw);
		crate::display::output(instance, &s.title, s.image.as_deref()).await
	}
}
