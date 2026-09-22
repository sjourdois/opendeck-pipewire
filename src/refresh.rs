//! Live key re-rendering for out-of-band PipeWire changes.
//!
//! Actions normally redraw only on user interaction or `willAppear`. When the
//! volume/mute of a sink or source changes by another means (wpctl, pavucontrol,
//! media keys, another app…), the PipeWire thread bumps a watch channel (see
//! [`crate::pw::PwHandle::subscribe`]); a background task then calls
//! [`refresh_all`] to redraw every visible instance from the latest state.
//!
//! The openaction runtime does not expose an instance's stored settings, so the
//! settings-bearing actions (device/input volume) register theirs into a shared
//! [`Refresher`] on appear/change and drop them on disappear.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openaction::*;

use crate::actions::app_volume::{self, AppVolumeSettings};
use crate::actions::device_volume::{self, DeviceVolumeSettings};
use crate::actions::input_volume::{self, InputVolumeSettings};
use crate::actions::mic_volume::{self, MicVolumeSettings};
use crate::actions::output::{self, OutputSettings};
use crate::actions::volume::{self, VolumeSettings};
use crate::pw::PwHandle;

/// Shared, cheaply-cloneable store of the per-instance settings needed to redraw
/// the settings-bearing actions from a background task.
#[derive(Clone, Default)]
pub struct Refresher {
	device: Arc<Mutex<HashMap<String, DeviceVolumeSettings>>>,
	input: Arc<Mutex<HashMap<String, InputVolumeSettings>>>,
	app: Arc<Mutex<HashMap<String, AppVolumeSettings>>>,
	/// Default sink/source volume settings (icon, font size, colours…), so an
	/// out-of-band redraw keeps the instance's appearance.
	volume: Arc<Mutex<HashMap<String, VolumeSettings>>>,
	mic: Arc<Mutex<HashMap<String, MicVolumeSettings>>>,
	/// Output-toggle settings, so its key can reflect an out-of-band default-sink
	/// change (another app, wpctl, media keys…) — active/greyed and which sink.
	output: Arc<Mutex<HashMap<String, OutputSettings>>>,
}

impl Refresher {
	pub fn set_device(&self, instance_id: &str, settings: &DeviceVolumeSettings) {
		self.device
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), settings.clone());
	}

	pub fn forget_device(&self, instance_id: &str) {
		self.device.lock().unwrap().remove(instance_id);
	}

	pub fn set_input(&self, instance_id: &str, settings: &InputVolumeSettings) {
		self.input
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), settings.clone());
	}

	pub fn forget_input(&self, instance_id: &str) {
		self.input.lock().unwrap().remove(instance_id);
	}

	pub fn set_app(&self, instance_id: &str, settings: &AppVolumeSettings) {
		self.app
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), settings.clone());
	}

	pub fn forget_app(&self, instance_id: &str) {
		self.app.lock().unwrap().remove(instance_id);
	}

	pub fn set_output(&self, instance_id: &str, settings: &OutputSettings) {
		self.output
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), settings.clone());
	}

	pub fn forget_output(&self, instance_id: &str) {
		self.output.lock().unwrap().remove(instance_id);
	}

	pub fn set_volume(&self, instance_id: &str, settings: &VolumeSettings) {
		self.volume
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), settings.clone());
	}

	pub fn forget_volume(&self, instance_id: &str) {
		self.volume.lock().unwrap().remove(instance_id);
	}

	pub fn set_mic(&self, instance_id: &str, settings: &MicVolumeSettings) {
		self.mic
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), settings.clone());
	}

	pub fn forget_mic(&self, instance_id: &str) {
		self.mic.lock().unwrap().remove(instance_id);
	}
}

/// Redraw every visible instance whose surface depends on live volume/mute state.
pub async fn refresh_all(pw: &PwHandle, refresher: &Refresher) {
	use crate::display;

	// Default sink volume (bar) + title (current output device, or a custom title).
	let sink = pw.default_sink_snapshot();
	let volume = refresher.volume.lock().unwrap().clone();
	for inst in visible_instances(volume::VolumeAction::UUID).await {
		let Some(settings) = volume.get(&inst.instance_id) else {
			continue;
		};
		let title = volume::resolve_title(&settings.title, pw);
		let _ = display::volume(
			&inst,
			display::Label::from_option(settings.title.as_deref(), &title),
			sink.known,
			sink.volume_cubic,
			sink.mute,
			&settings.colors,
			&settings.ui,
		)
		.await;
		let _ = display::title(&inst, &title).await;
	}

	// Output toggle: reflect the live default sink (active/greyed + which sink).
	let output = refresher.output.lock().unwrap().clone();
	for inst in visible_instances(output::OutputAction::UUID).await {
		let Some(settings) = output.get(&inst.instance_id) else {
			continue;
		};
		let s = output::surface(settings, pw);
		let _ = display::output(&inst, &s.title, s.image.as_deref()).await;
	}

	// Default source (mic) volume (bar) + title (current input device, or custom).
	let source = pw.default_source_snapshot();
	let mic = refresher.mic.lock().unwrap().clone();
	for inst in visible_instances(mic_volume::MicVolumeAction::UUID).await {
		let Some(settings) = mic.get(&inst.instance_id) else {
			continue;
		};
		let title = mic_volume::resolve_title(&settings.title, pw);
		let _ = display::mic(
			&inst,
			display::Label::from_option(settings.title.as_deref(), &title),
			source.known,
			source.volume_cubic,
			source.mute,
			&settings.colors,
			&settings.ui,
		)
		.await;
		let _ = display::title(&inst, &title).await;
	}
	if source.known {
		// Push to Talk shows the mic's live state (state 0 = open, 1 = muted).
		for inst in visible_instances(crate::actions::push_to_talk::PushToTalkAction::UUID).await {
			let _ = inst.set_state(if source.mute { 1 } else { 0 }).await;
		}
	}

	// Specific output devices (per-instance target sink).
	let device = refresher.device.lock().unwrap().clone();
	for inst in visible_instances(device_volume::DeviceVolumeAction::UUID).await {
		let Some(settings) = device.get(&inst.instance_id) else {
			continue;
		};
		let live = settings.sink.as_deref().and_then(|n| pw.sink_state(n));
		let (vol, mute) = live.unwrap_or((0.0, false));
		let text = device_volume::label(settings, pw);
		let _ = display::device(
			&inst,
			display::Label::from_field(&settings.name, &text),
			live.is_some(),
			vol,
			mute,
			&settings.colors,
			&settings.ui,
		)
		.await;
	}

	// Specific input devices (per-instance target source).
	let input = refresher.input.lock().unwrap().clone();
	for inst in visible_instances(input_volume::InputVolumeAction::UUID).await {
		let Some(settings) = input.get(&inst.instance_id) else {
			continue;
		};
		let live = settings.source.as_deref().and_then(|n| pw.source_state(n));
		let (vol, mute) = live.unwrap_or((0.0, false));
		let text = input_volume::label(settings, pw);
		let _ = display::input(
			&inst,
			display::Label::from_field(&settings.name, &text),
			live.is_some(),
			vol,
			mute,
			&settings.colors,
			&settings.ui,
		)
		.await;
	}

	// Per-app volume (per-instance target application).
	let app = refresher.app.lock().unwrap().clone();
	for inst in visible_instances(app_volume::AppVolumeAction::UUID).await {
		let Some(settings) = app.get(&inst.instance_id) else {
			continue;
		};
		let target = settings.app.as_deref().filter(|s| !s.is_empty());
		let live = target.and_then(|a| pw.app_state(a));
		let (vol, mute) = live.unwrap_or((0.0, false));
		let text = app_volume::label(settings);
		let _ = display::app(
			&inst,
			display::Label::from_field(&settings.name, &text),
			live.is_some(),
			vol,
			mute,
			&settings.colors,
			&settings.ui,
		)
		.await;
	}
}
