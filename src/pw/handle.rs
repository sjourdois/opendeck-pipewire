//! The cheap, cloneable handle every action uses to talk to the backend, plus
//! the entry point that starts the PipeWire thread.

use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::command::Command;

use super::backend::run_loop;
use super::{AppDesc, Channels, SinkDesc, SinkSnapshot};

/// Cheap, cloneable handle held by every action.
#[derive(Clone)]
pub struct PwHandle {
	tx: Arc<Mutex<pipewire::channel::Sender<Command>>>,
	state: Arc<Mutex<SinkSnapshot>>,
	source_state: Arc<Mutex<SinkSnapshot>>,
	sinks: Arc<Mutex<Vec<SinkDesc>>>,
	sources: Arc<Mutex<Vec<SinkDesc>>>,
	apps: Arc<Mutex<Vec<AppDesc>>>,
	default_sink_name: Arc<Mutex<Option<String>>>,
	default_source_name: Arc<Mutex<Option<String>>>,
	/// Bumped by the PipeWire thread whenever any published state changes, so the
	/// UI can re-render on out-of-band volume/mute changes (wpctl, media keys…).
	notify: Arc<tokio::sync::watch::Sender<u64>>,
}

impl PwHandle {
	/// Send an intent to the backend thread (fire-and-forget; a dead channel is
	/// ignored, as it only happens during shutdown).
	pub fn send(&self, cmd: Command) {
		let _ = self.tx.lock().unwrap().send(cmd);
	}

	// --- Live state read back from the PipeWire thread ---

	pub fn default_sink_snapshot(&self) -> SinkSnapshot {
		*self.state.lock().unwrap()
	}

	pub fn default_source_snapshot(&self) -> SinkSnapshot {
		*self.source_state.lock().unwrap()
	}

	/// Live (volume_cubic, mute) of a specific sink by `node.name`, if known.
	pub fn sink_state(&self, name: &str) -> Option<(f32, bool)> {
		node_state(&self.sinks, name)
	}

	/// Live (volume_cubic, mute) of a specific source by `node.name`, if known.
	pub fn source_state(&self, name: &str) -> Option<(f32, bool)> {
		node_state(&self.sources, name)
	}

	/// Current list of audio sinks (for the property inspector dropdown).
	pub fn sinks(&self) -> Vec<SinkDesc> {
		self.sinks.lock().unwrap().clone()
	}

	/// Current list of audio sources (for the property inspector dropdown).
	pub fn sources(&self) -> Vec<SinkDesc> {
		self.sources.lock().unwrap().clone()
	}

	/// Distinct applications currently producing audio (for the PI dropdown).
	pub fn apps(&self) -> Vec<AppDesc> {
		self.apps.lock().unwrap().clone()
	}

	/// `node.name` of the current default sink, if resolved (for cycling).
	pub fn default_sink_name(&self) -> Option<String> {
		self.default_sink_name.lock().unwrap().clone()
	}

	/// `node.name` of the current default source, if resolved (for cycling).
	pub fn default_source_name(&self) -> Option<String> {
		self.default_source_name.lock().unwrap().clone()
	}

	/// Live aggregate (volume_cubic, mute) of an app's streams by
	/// `application.name`, if it is currently producing audio.
	pub fn app_state(&self, name: &str) -> Option<(f32, bool)> {
		self.apps
			.lock()
			.unwrap()
			.iter()
			.find(|a| a.name == name)
			.map(|a| (a.volume_cubic, a.mute))
	}

	/// Subscribe to state-change notifications. `changed()` on the returned
	/// receiver resolves whenever any sink/source volume, mute, or default
	/// changes — including changes made outside this plugin.
	pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
		self.notify.subscribe()
	}
}

/// Look up a node's live (volume_cubic, mute) by `node.name` in a descriptor list.
fn node_state(list: &Mutex<Vec<SinkDesc>>, name: &str) -> Option<(f32, bool)> {
	list.lock()
		.unwrap()
		.iter()
		.find(|s| s.name == name)
		.map(|s| (s.volume_cubic, s.mute))
}

/// Start the PipeWire backend thread. Returns immediately.
pub fn start() -> Result<PwHandle> {
	let state = Arc::new(Mutex::new(SinkSnapshot::default()));
	let source_state = Arc::new(Mutex::new(SinkSnapshot::default()));
	let sinks = Arc::new(Mutex::new(Vec::new()));
	let sources = Arc::new(Mutex::new(Vec::new()));
	let apps = Arc::new(Mutex::new(Vec::new()));
	let default_sink_name = Arc::new(Mutex::new(None));
	let default_source_name = Arc::new(Mutex::new(None));
	// Drop the initial receiver; consumers get their own via `PwHandle::subscribe`.
	let (notify_tx, _) = tokio::sync::watch::channel(0u64);
	let notify = Arc::new(notify_tx);
	let chans = Channels {
		state: state.clone(),
		source_state: source_state.clone(),
		sinks: sinks.clone(),
		sources: sources.clone(),
		apps: apps.clone(),
		default_sink_name: default_sink_name.clone(),
		default_source_name: default_source_name.clone(),
		notify: notify.clone(),
	};

	let (tx, rx) = pipewire::channel::channel::<Command>();

	std::thread::Builder::new()
		.name("pipewire".into())
		.spawn(move || {
			if let Err(error) = run_loop(rx, chans) {
				log::error!("PipeWire loop exited: {error}");
			}
		})?;

	Ok(PwHandle {
		tx: Arc::new(Mutex::new(tx)),
		state,
		source_state,
		sinks,
		sources,
		apps,
		default_sink_name,
		default_source_name,
		notify,
	})
}
