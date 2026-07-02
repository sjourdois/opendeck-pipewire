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

use crate::device_volume::{self, DeviceVolumeSettings};
use crate::input_volume::{self, InputVolumeSettings};
use crate::pw::PwHandle;

/// Shared, cheaply-cloneable store of the per-instance settings needed to redraw
/// the settings-bearing actions from a background task.
#[derive(Clone, Default)]
pub struct Refresher {
	device: Arc<Mutex<HashMap<String, DeviceVolumeSettings>>>,
	input: Arc<Mutex<HashMap<String, InputVolumeSettings>>>,
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
}

/// Redraw every visible instance whose image depends on live volume/mute state.
pub async fn refresh_all(pw: &PwHandle, refresher: &Refresher) {
	use crate::render;

	// Default sink volume.
	let sink = pw.default_sink_snapshot();
	if sink.known {
		for inst in visible_instances(crate::volume::VolumeAction::UUID).await {
			let _ = inst
				.set_image(Some(render::volume_key(sink.volume_cubic, sink.mute)), None)
				.await;
		}
		// Default sink mute (2-state icon).
		for inst in visible_instances(crate::mute::MuteAction::UUID).await {
			let _ = inst.set_state(if sink.mute { 1 } else { 0 }).await;
		}
	}

	// Default source (mic) volume.
	let source = pw.default_source_snapshot();
	if source.known {
		for inst in visible_instances(crate::mic_volume::MicVolumeAction::UUID).await {
			let _ = inst
				.set_image(
					Some(render::volume_key(source.volume_cubic, source.mute)),
					None,
				)
				.await;
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
		let _ = inst
			.set_image(
				Some(render::device_key(
					&device_volume::label(settings),
					vol,
					mute,
				)),
				None,
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
		let _ = inst
			.set_image(
				Some(render::device_key(
					&input_volume::label(settings),
					vol,
					mute,
				)),
				None,
			)
			.await;
	}
}
