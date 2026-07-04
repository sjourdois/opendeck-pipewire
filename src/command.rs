//! Intents the actions send to the PipeWire backend.
//!
//! Actions build a [`Command`] and hand it to [`crate::pw::PwHandle::send`]; the
//! backend thread applies it. Volume deltas are on the cubic/perceptual scale
//! (see [`crate::pw`]). Names are `node.name` / `application.name` strings.

pub enum Command {
	/// Change the default sink volume by this delta on the cubic scale.
	AdjustVolume(f32),
	/// Set mute on the default sink. `None` toggles.
	SetMute(Option<bool>),
	/// Make the sink with this `node.name` the system default output, by setting
	/// the *configured* default (`default.configured.audio.sink`) so the choice is
	/// sticky rather than transient.
	SetDefaultSink(String),
	/// Change the volume of every stream of an app by this cubic delta.
	AdjustAppVolume(String, f32),
	/// Set mute on every stream of an app. `None` toggles.
	SetAppMute(String, Option<bool>),
	/// Change a specific sink's volume (by `node.name`) by this cubic delta.
	AdjustSinkVolume(String, f32),
	/// Set mute on a specific sink (by `node.name`). `None` toggles.
	SetSinkMute(String, Option<bool>),

	// --- Input / source side (microphones, capture devices) ---
	/// Change the default source (input) volume by this cubic delta.
	AdjustDefaultSourceVolume(f32),
	/// Set mute on the default source. `None` toggles.
	SetDefaultSourceMute(Option<bool>),
	/// Change a specific source's volume (by `node.name`) by this cubic delta.
	AdjustSourceVolume(String, f32),
	/// Set mute on a specific source (by `node.name`). `None` toggles.
	SetSourceMute(String, Option<bool>),
	/// Make the source with this `node.name` the system default input.
	SetDefaultSource(String),
}
