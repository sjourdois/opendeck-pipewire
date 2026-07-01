//! Native PipeWire backend.
//!
//! PipeWire's main loop is single-threaded and **not** `Send`, so everything
//! PipeWire-related lives inside one dedicated OS thread (`backend::run_loop`).
//! Actions hold a cheap, cloneable [`PwHandle`]: a `pipewire::channel` carries
//! commands in, while shared `Arc<Mutex<..>>` state and a `watch` channel carry
//! volume / mute / device state out.
//!
//! Volume scale: we expose the "perceptual"/cubic scale (like `wpctl`), where
//! the user-facing 0–100% maps to PipeWire's linear `channelVolumes` via
//! `linear = cubic³`.
//!
//! Hardware vs software volume: sinks backed by a sound card with a hardware
//! mixer (e.g. USB headsets) ignore node-level `channelVolumes` — WirePlumber
//! resets them. For those we set the volume on the owning `Device`'s active
//! `Route` instead (the `pod` submodule); plain software sinks use node Props.
//!
//! Submodules: `types` (published data), `handle` (the action-facing API),
//! `backend` (the loop thread), `pod` (SPA POD writes).

mod backend;
mod handle;
mod pod;
mod types;

use std::sync::{Arc, Mutex};

pub use handle::{PwHandle, start};
pub use types::{AppDesc, SinkDesc, SinkSnapshot};

/// Shared state published from the PipeWire thread to the actions. Bundled into a
/// struct to keep `run_loop`/`Inner` signatures small as the surface grows.
struct Channels {
	state: Arc<Mutex<SinkSnapshot>>,
	source_state: Arc<Mutex<SinkSnapshot>>,
	sinks: Arc<Mutex<Vec<SinkDesc>>>,
	sources: Arc<Mutex<Vec<SinkDesc>>>,
	apps: Arc<Mutex<Vec<AppDesc>>>,
	/// `node.name` of the current default sink / source, for the cycling pickers.
	default_sink_name: Arc<Mutex<Option<String>>>,
	default_source_name: Arc<Mutex<Option<String>>>,
	notify: Arc<tokio::sync::watch::Sender<u64>>,
}
