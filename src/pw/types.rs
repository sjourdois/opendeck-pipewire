//! Data types published from the PipeWire backend to the actions.

/// State published from the PipeWire thread to the OpenDeck actions.
#[derive(Clone, Copy, Default)]
pub struct SinkSnapshot {
	/// Volume on the cubic/perceptual scale (0.0..=1.0, may exceed 1.0).
	pub volume_cubic: f32,
	pub mute: bool,
	/// False until we have resolved and read the default sink at least once.
	pub known: bool,
}

/// A selectable audio sink, exposed to the property inspector and used by the
/// Device Volume action for live rendering.
#[derive(Clone, serde::Serialize)]
pub struct SinkDesc {
	pub name: String,
	pub description: String,
	pub volume_cubic: f32,
	pub mute: bool,
}

/// A running application producing audio, exposed to the property inspector and
/// used by the App Volume action for live rendering. `volume_cubic`/`mute`
/// aggregate the app's streams (see [`crate::pw`] `refresh_apps`).
#[derive(Clone, serde::Serialize)]
pub struct AppDesc {
	pub name: String,
	pub volume_cubic: f32,
	pub mute: bool,
}
