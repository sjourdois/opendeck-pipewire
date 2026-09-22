//! Shared per-instance appearance settings for the volume actions.
//!
//! Flattened into each volume action's settings (`icon` / `limit_100`). All
//! fields are optional / backward compatible: profiles saved before these
//! settings existed deserialize to "no icon, no limit".

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct VolumeUi {
	/// Custom icon (data URI) picked in the property inspector. Shown at the top
	/// of the keypad key image and sent as the encoder touchstrip `icon` in
	/// `setFeedback` (ignored by layouts that don't support it).
	pub icon: Option<String>,
	/// When true, volume increases through this action stop at 100% instead of
	/// allowing the +50% boost range.
	pub limit_100: bool,
}

impl VolumeUi {
	/// The cubic-scale ceiling for volume changes through this action.
	pub fn max_cubic(&self) -> f32 {
		if self.limit_100 { 1.0 } else { 1.5 }
	}

	/// Shrink `delta` so that `current + delta` stays within `0..=max_cubic`.
	/// The backend only takes relative adjustments, so the limit is applied by
	/// trimming the delta before sending. A current volume above the ceiling
	/// (e.g. set externally) is left alone: lowering passes through untrimmed.
	pub fn limit_delta(&self, current: f32, delta: f32) -> f32 {
		if delta > 0.0 {
			(current + delta).min(self.max_cubic()) - current
		} else {
			(current + delta).max(0.0) - current
		}
	}

	/// The configured icon data URI, if any (empty strings count as unset).
	pub fn icon(&self) -> Option<&str> {
		self.icon.as_deref().filter(|s| !s.is_empty())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn defaults_mean_no_change() {
		let ui = VolumeUi::default();
		assert_eq!(ui.max_cubic(), 1.5);
		assert_eq!(ui.icon(), None);
		// Without a limit the delta passes through untouched.
		assert!((ui.limit_delta(0.9, 0.2) - 0.2).abs() < 1e-6);
	}

	#[test]
	fn limit_stops_at_full_scale() {
		let ui = VolumeUi {
			limit_100: true,
			..VolumeUi::default()
		};
		assert_eq!(ui.max_cubic(), 1.0);
		assert!((ui.limit_delta(0.9, 0.2) - 0.1).abs() < 1e-6);
		assert_eq!(ui.limit_delta(1.0, 0.2), 0.0);
		// Lowering still works from above the ceiling (e.g. set externally).
		assert!((ui.limit_delta(1.2, -0.1) - -0.1).abs() < 1e-6);
		// … but never below silence.
		assert!((ui.limit_delta(0.05, -0.2) - -0.05).abs() < 1e-6);
	}
}
