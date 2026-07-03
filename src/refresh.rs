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
use crate::color::BarColors;
use crate::pw::PwHandle;

/// Shared, cheaply-cloneable store of the per-instance settings needed to redraw
/// the settings-bearing actions from a background task.
#[derive(Clone, Default)]
pub struct Refresher {
	device: Arc<Mutex<HashMap<String, DeviceVolumeSettings>>>,
	input: Arc<Mutex<HashMap<String, InputVolumeSettings>>>,
	app: Arc<Mutex<HashMap<String, AppVolumeSettings>>>,
	/// Bar colours of the default sink/source volume actions (which otherwise
	/// carry no per-instance settings), so their custom colours survive an
	/// out-of-band redraw.
	default_colors: Arc<Mutex<HashMap<String, BarColors>>>,
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

	pub fn set_colors(&self, instance_id: &str, colors: &BarColors) {
		self.default_colors
			.lock()
			.unwrap()
			.insert(instance_id.to_owned(), colors.clone());
	}

	pub fn forget_colors(&self, instance_id: &str) {
		self.default_colors.lock().unwrap().remove(instance_id);
	}
}

/// Redraw every visible instance whose surface depends on live volume/mute state.
pub async fn refresh_all(pw: &PwHandle, refresher: &Refresher) {
	use crate::display;

	let default_colors = refresher.default_colors.lock().unwrap().clone();
	let colors_for = |inst: &Instance| {
		default_colors
			.get(&inst.instance_id)
			.cloned()
			.unwrap_or_default()
	};

	// Default sink volume + mute.
	let sink = pw.default_sink_snapshot();
	if sink.known {
		for inst in visible_instances(crate::actions::volume::VolumeAction::UUID).await {
			let _ = display::volume(
				&inst,
				true,
				sink.volume_cubic,
				sink.mute,
				&colors_for(&inst),
			)
			.await;
		}
	}

	// Default source (mic) volume + mute.
	let source = pw.default_source_snapshot();
	if source.known {
		for inst in visible_instances(crate::actions::mic_volume::MicVolumeAction::UUID).await {
			let _ = display::mic(
				&inst,
				true,
				source.volume_cubic,
				source.mute,
				&colors_for(&inst),
			)
			.await;
		}
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
		let (vol, mute) = settings
			.sink
			.as_deref()
			.and_then(|n| pw.sink_state(n))
			.unwrap_or((0.0, false));
		let _ = display::device(
			&inst,
			&device_volume::label(settings, pw),
			vol,
			mute,
			&settings.colors,
		)
		.await;
	}

	// Specific input devices (per-instance target source).
	let input = refresher.input.lock().unwrap().clone();
	for inst in visible_instances(input_volume::InputVolumeAction::UUID).await {
		let Some(settings) = input.get(&inst.instance_id) else {
			continue;
		};
		let (vol, mute) = settings
			.source
			.as_deref()
			.and_then(|n| pw.source_state(n))
			.unwrap_or((0.0, false));
		let _ = display::input(
			&inst,
			&input_volume::label(settings, pw),
			vol,
			mute,
			&settings.colors,
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
		let (vol, mute) = target.and_then(|a| pw.app_state(a)).unwrap_or((0.0, false));
		let _ = display::app(&inst, target, vol, mute, &settings.colors).await;
	}
}
