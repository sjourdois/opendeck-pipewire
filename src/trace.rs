//! Timing of the surface events OpenDeck delivers (key, dial, touchstrip).
//!
//! The openaction runtime handles inbound events one at a time, and every
//! outbound message goes through one shared lock, so a send that stalls holds
//! up every press behind it. Logging each event when its handler starts, and
//! warning when one runs long, shows whether presses reached the plugin late
//! or reached it on time and waited.

use std::time::{Duration, Instant};

use openaction::Instance;

/// A handler slower than this is logged as a warning when it ends.
const SLOW: Duration = Duration::from_millis(100);

/// A running event handler, logged on creation and, if it ran slow, on drop.
pub struct Event {
	name: &'static str,
	action: String,
	instance_id: String,
	start: Instant,
}

impl Event {
	/// Log `name` (e.g. `"dial_down"`) arriving on `instance`. Hold the returned
	/// value for the whole handler.
	pub fn start(instance: &Instance, name: &'static str) -> Self {
		let action = instance
			.action_uuid
			.rsplit('.')
			.next()
			.unwrap_or_default()
			.to_owned();
		log::info!("{name} {action} {}", instance.instance_id);
		Self {
			name,
			action,
			instance_id: instance.instance_id.clone(),
			start: Instant::now(),
		}
	}
}

impl Drop for Event {
	fn drop(&mut self) {
		let elapsed = self.start.elapsed();
		if elapsed >= SLOW {
			log::warn!(
				"{} {} {} took {} ms",
				self.name,
				self.action,
				self.instance_id,
				elapsed.as_millis()
			);
		}
	}
}

/// Warn when a background redraw of every visible instance ran long: it holds
/// the outbound lock for each message it sends, ahead of any press.
pub fn slow_refresh(start: Instant) {
	let elapsed = start.elapsed();
	if elapsed >= SLOW {
		log::warn!("refresh took {} ms", elapsed.as_millis());
	}
}
