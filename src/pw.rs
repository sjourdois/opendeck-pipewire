//! Native PipeWire backend.
//!
//! PipeWire's main loop is single-threaded and **not** `Send`, so everything
//! PipeWire-related lives inside one dedicated OS thread ([`run_loop`]). Actions
//! talk to it through a `pipewire::channel` (commands in) and a shared
//! `Arc<Mutex<SinkSnapshot>>` (state out).
//!
//! Volume scale: we expose the "perceptual"/cubic scale (like `wpctl`), where
//! the user-facing 0–100% maps to PipeWire's linear `channelVolumes` via
//! `linear = cubic³`. See [`cubic_to_linear`] / [`linear_to_cubic`].
//!
//! Hardware vs software volume: sinks backed by a sound card with a hardware
//! mixer (e.g. USB headsets) ignore node-level `channelVolumes` — WirePlumber
//! resets them. For those we set the volume on the owning `Device`'s active
//! `Route` instead (see [`set_device_route`] / [`Inner::apply_node`]); plain
//! software sinks use node Props.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::Result;

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

/// A running application producing audio, exposed to the property inspector.
#[derive(Clone, serde::Serialize)]
pub struct AppDesc {
	pub name: String,
}

enum Command {
	/// Change the default sink volume by this delta on the cubic scale.
	AdjustVolume(f32),
	/// Set mute on the default sink. `None` toggles.
	SetMute(Option<bool>),
	/// Make the sink with this `node.name` the system default output.
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

/// Cheap, cloneable handle held by every action.
#[derive(Clone)]
pub struct PwHandle {
	tx: Arc<Mutex<pipewire::channel::Sender<Command>>>,
	state: Arc<Mutex<SinkSnapshot>>,
	source_state: Arc<Mutex<SinkSnapshot>>,
	sinks: Arc<Mutex<Vec<SinkDesc>>>,
	sources: Arc<Mutex<Vec<SinkDesc>>>,
	apps: Arc<Mutex<Vec<AppDesc>>>,
	/// Bumped by the PipeWire thread whenever any published state changes, so the
	/// UI can re-render on out-of-band volume/mute changes (wpctl, media keys…).
	notify: Arc<tokio::sync::watch::Sender<u64>>,
}

impl PwHandle {
	pub fn adjust_default_sink_volume(&self, delta_cubic: f32) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::AdjustVolume(delta_cubic));
	}

	pub fn toggle_default_sink_mute(&self) {
		let _ = self.tx.lock().unwrap().send(Command::SetMute(None));
	}

	pub fn set_default_sink(&self, name: impl Into<String>) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetDefaultSink(name.into()));
	}

	pub fn adjust_app_volume(&self, app: impl Into<String>, delta_cubic: f32) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::AdjustAppVolume(app.into(), delta_cubic));
	}

	pub fn toggle_app_mute(&self, app: impl Into<String>) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetAppMute(app.into(), None));
	}

	/// Distinct applications currently producing audio (for the PI dropdown).
	pub fn apps(&self) -> Vec<AppDesc> {
		self.apps.lock().unwrap().clone()
	}

	pub fn adjust_sink_volume(&self, name: impl Into<String>, delta_cubic: f32) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::AdjustSinkVolume(name.into(), delta_cubic));
	}

	pub fn toggle_sink_mute(&self, name: impl Into<String>) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetSinkMute(name.into(), None));
	}

	/// Live (volume_cubic, mute) of a specific sink by `node.name`, if known.
	pub fn sink_state(&self, name: &str) -> Option<(f32, bool)> {
		self.sinks
			.lock()
			.unwrap()
			.iter()
			.find(|s| s.name == name)
			.map(|s| (s.volume_cubic, s.mute))
	}

	pub fn default_sink_snapshot(&self) -> SinkSnapshot {
		*self.state.lock().unwrap()
	}

	/// Current list of audio sinks (for the property inspector dropdown).
	pub fn sinks(&self) -> Vec<SinkDesc> {
		self.sinks.lock().unwrap().clone()
	}

	// --- Input / source side ---

	pub fn adjust_default_source_volume(&self, delta_cubic: f32) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::AdjustDefaultSourceVolume(delta_cubic));
	}

	pub fn toggle_default_source_mute(&self) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetDefaultSourceMute(None));
	}

	/// Set (not toggle) the default source mute — used by Push to Talk.
	pub fn set_default_source_mute(&self, mute: bool) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetDefaultSourceMute(Some(mute)));
	}

	pub fn adjust_source_volume(&self, name: impl Into<String>, delta_cubic: f32) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::AdjustSourceVolume(name.into(), delta_cubic));
	}

	pub fn toggle_source_mute(&self, name: impl Into<String>) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetSourceMute(name.into(), None));
	}

	pub fn set_default_source(&self, name: impl Into<String>) {
		let _ = self
			.tx
			.lock()
			.unwrap()
			.send(Command::SetDefaultSource(name.into()));
	}

	pub fn default_source_snapshot(&self) -> SinkSnapshot {
		*self.source_state.lock().unwrap()
	}

	pub fn source_state(&self, name: &str) -> Option<(f32, bool)> {
		self.sources
			.lock()
			.unwrap()
			.iter()
			.find(|s| s.name == name)
			.map(|s| (s.volume_cubic, s.mute))
	}

	/// Current list of audio sources (for the property inspector dropdown).
	pub fn sources(&self) -> Vec<SinkDesc> {
		self.sources.lock().unwrap().clone()
	}

	/// Subscribe to state-change notifications. `changed()` on the returned
	/// receiver resolves whenever any sink/source volume, mute, or default
	/// changes — including changes made outside this plugin.
	pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
		self.notify.subscribe()
	}
}

/// Start the PipeWire backend thread. Returns immediately.
pub fn start() -> Result<PwHandle> {
	let state = Arc::new(Mutex::new(SinkSnapshot::default()));
	let source_state = Arc::new(Mutex::new(SinkSnapshot::default()));
	let sinks = Arc::new(Mutex::new(Vec::new()));
	let sources = Arc::new(Mutex::new(Vec::new()));
	let apps = Arc::new(Mutex::new(Vec::new()));
	// Drop the initial receiver; consumers get their own via `PwHandle::subscribe`.
	let (notify_tx, _) = tokio::sync::watch::channel(0u64);
	let notify = Arc::new(notify_tx);
	let chans = Channels {
		state: state.clone(),
		source_state: source_state.clone(),
		sinks: sinks.clone(),
		sources: sources.clone(),
		apps: apps.clone(),
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
		notify,
	})
}

/// Shared state published from the PipeWire thread to the actions. Bundled into a
/// struct to keep `run_loop`/`Inner` signatures small as the surface grows.
struct Channels {
	state: Arc<Mutex<SinkSnapshot>>,
	source_state: Arc<Mutex<SinkSnapshot>>,
	sinks: Arc<Mutex<Vec<SinkDesc>>>,
	sources: Arc<Mutex<Vec<SinkDesc>>>,
	apps: Arc<Mutex<Vec<AppDesc>>>,
	notify: Arc<tokio::sync::watch::Sender<u64>>,
}

// ---------------------------------------------------------------------------
// Everything below runs on the PipeWire thread only.
// ---------------------------------------------------------------------------

/// Per-node info we track to resolve and mutate the default sink.
struct NodeRef {
	name: String,
	description: String,
	is_sink: bool,
	is_source: bool,
	is_stream: bool,
	/// For an application output stream: its `application.name`.
	app_name: Option<String>,
	/// For a hardware sink: the owning `Device` object id and the route's
	/// `card.profile.device` index, used to set the hardware (route) volume.
	device_id: Option<u32>,
	route_device: Option<i32>,
	volume_cubic: f32,
	mute: bool,
	proxy: pipewire::node::Node,
	// Listener must be kept alive for as long as the proxy.
	_listener: pipewire::node::NodeListener,
}

/// A tracked `Device` (sound card) and its output routes, keyed by the route's
/// `device` index (== a node's `card.profile.device`).
struct DeviceRef {
	proxy: pipewire::device::Device,
	routes: HashMap<i32, RouteInfo>,
	_listener: pipewire::device::DeviceListener,
}

#[derive(Clone, Copy)]
struct RouteInfo {
	index: i32,
	channels: usize,
}

struct Inner {
	nodes: HashMap<u32, NodeRef>,
	devices: HashMap<u32, DeviceRef>,
	/// `node.name` of the current default sink / source (from "default" metadata).
	default_sink_name: Option<String>,
	default_source_name: Option<String>,
	/// The "default" metadata proxy + its listener, kept alive while connected.
	_metadata: Option<(
		pipewire::metadata::Metadata,
		pipewire::metadata::MetadataListener,
	)>,
	chans: Channels,
}

impl Inner {
	fn new(chans: Channels) -> Self {
		Self {
			nodes: HashMap::new(),
			devices: HashMap::new(),
			default_sink_name: None,
			default_source_name: None,
			_metadata: None,
			chans,
		}
	}

	/// Build the (sorted, live volume/mute) descriptor list for sinks or sources.
	/// Node `volume_cubic`/`mute` are kept current from node Props (software sinks)
	/// and from the owning device's Route params (route-controlled USB devices, see
	/// [`update_device_route`]), so this reflects the real volume for both.
	fn node_descs(&self, want_sink: bool) -> Vec<SinkDesc> {
		let mut list: Vec<SinkDesc> = self
			.nodes
			.values()
			.filter(|n| if want_sink { n.is_sink } else { n.is_source })
			.map(|n| SinkDesc {
				name: n.name.clone(),
				description: if n.description.is_empty() {
					n.name.clone()
				} else {
					n.description.clone()
				},
				volume_cubic: n.volume_cubic,
				mute: n.mute,
			})
			.collect();
		list.sort_by(|a, b| a.description.cmp(&b.description));
		list
	}

	/// Wake any UI subscribers so they can re-render from the latest state.
	fn bump(&self) {
		self.chans.notify.send_modify(|v| *v = v.wrapping_add(1));
	}

	fn refresh_sinks(&self) {
		*self.chans.sinks.lock().unwrap() = self.node_descs(true);
		self.bump();
	}

	fn refresh_sources(&self) {
		*self.chans.sources.lock().unwrap() = self.node_descs(false);
		self.bump();
	}

	fn sink_id_by_name(&self, name: &str) -> Option<u32> {
		self.nodes
			.iter()
			.find(|(_, n)| n.is_sink && n.name == name)
			.map(|(id, _)| *id)
	}

	fn source_id_by_name(&self, name: &str) -> Option<u32> {
		self.nodes
			.iter()
			.find(|(_, n)| n.is_source && n.name == name)
			.map(|(id, _)| *id)
	}

	/// Apply a volume/mute change to a node (sink or source), using the hardware
	/// route when it has one (USB headsets/mics etc.), else the node's Props.
	fn apply_node(&self, node_id: u32, linear: Option<f32>, mute: Option<bool>) {
		let Some(n) = self.nodes.get(&node_id) else {
			return;
		};
		if let (Some(dev_id), Some(route_device)) = (n.device_id, n.route_device)
			&& let Some(dev) = self.devices.get(&dev_id)
			&& let Some(route) = dev.routes.get(&route_device)
		{
			set_device_route(
				&dev.proxy,
				route.index,
				route_device,
				linear,
				mute,
				route.channels,
			);
			return;
		}
		set_node_props(&n.proxy, linear, mute);
	}

	/// Republish the list of distinct applications producing audio.
	fn refresh_apps(&self) {
		let mut names: Vec<String> = self
			.nodes
			.values()
			.filter(|n| n.is_stream)
			.filter_map(|n| n.app_name.clone())
			.filter(|s| !s.is_empty())
			.collect();
		names.sort();
		names.dedup();
		*self.chans.apps.lock().unwrap() = names.into_iter().map(|name| AppDesc { name }).collect();
	}

	fn default_sink_id(&self) -> Option<u32> {
		self.sink_id_by_name(self.default_sink_name.as_deref()?)
	}

	fn default_source_id(&self) -> Option<u32> {
		self.source_id_by_name(self.default_source_name.as_deref()?)
	}

	/// Recompute the published snapshots from the default sink / source state.
	fn publish(&self) {
		if let Some(id) = self.default_sink_id() {
			let n = &self.nodes[&id];
			*self.chans.state.lock().unwrap() = SinkSnapshot {
				volume_cubic: n.volume_cubic,
				mute: n.mute,
				known: true,
			};
		}
		if let Some(id) = self.default_source_id() {
			let n = &self.nodes[&id];
			*self.chans.source_state.lock().unwrap() = SinkSnapshot {
				volume_cubic: n.volume_cubic,
				mute: n.mute,
				known: true,
			};
		}
		self.bump();
	}
}

fn run_loop(rx: pipewire::channel::Receiver<Command>, chans: Channels) -> Result<()> {
	use pipewire::context::Context;
	use pipewire::main_loop::MainLoop;

	pipewire::init();

	let main_loop = MainLoop::new(None)?;
	let context = Context::new(&main_loop)?;
	let core = context.connect(None)?;
	let registry = Rc::new(core.get_registry()?);

	let inner = Rc::new(RefCell::new(Inner::new(chans)));

	// --- Registry: discover audio sink nodes and the default-sink metadata ---
	let reg_for_cb = registry.clone();
	let inner_g = inner.clone();
	let inner_r = inner.clone();
	let _registry_listener = registry
		.add_listener_local()
		.global(move |global| on_global(&inner_g, &reg_for_cb, global))
		.global_remove(move |id| on_global_remove(&inner_r, id))
		.register();

	// --- Commands from actions, delivered inside this loop ---
	let inner_c = inner.clone();
	let _recv = rx.attach(main_loop.loop_(), move |cmd| {
		on_command(&inner_c, cmd);
	});

	log::info!("PipeWire backend connected; entering main loop");
	main_loop.run();
	Ok(())
}

fn on_global(
	inner: &Rc<RefCell<Inner>>,
	registry: &Rc<pipewire::registry::Registry>,
	global: &pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef>,
) {
	use pipewire::types::ObjectType;

	let Some(props) = global.props else { return };

	match global.type_ {
		ObjectType::Node => {
			let media_class = props.get("media.class").unwrap_or("");
			let is_sink = media_class == "Audio/Sink";
			let is_source = media_class == "Audio/Source";
			// Only streams carrying an `application.name` count as apps; this
			// excludes the internal combine/loopback streams (which have none).
			let app_name: Option<String> = props.get("application.name").map(str::to_owned);
			let is_stream = media_class == "Stream/Output/Audio" && app_name.is_some();
			if !is_sink && !is_source && !is_stream {
				return;
			}

			let device_id = props.get("device.id").and_then(|s| s.parse::<u32>().ok());
			let route_device = props
				.get("card.profile.device")
				.and_then(|s| s.parse::<i32>().ok());
			let name = props.get("node.name").unwrap_or("").to_owned();
			let description = if is_stream {
				props
					.get("media.name")
					.map(str::to_owned)
					.or_else(|| app_name.clone())
					.unwrap_or_else(|| name.clone())
			} else {
				props
					.get("node.description")
					.or_else(|| props.get("node.nick"))
					.unwrap_or("")
					.to_owned()
			};

			// Bind the node so we can read its Props (volume/mute) and set them.
			let node: pipewire::node::Node = match registry.bind(global) {
				Ok(n) => n,
				Err(e) => {
					log::warn!("Failed to bind node {}: {e}", global.id);
					return;
				}
			};

			let id = global.id;
			let inner_param = inner.clone();
			let inner_info = inner.clone();
			let listener = node
				.add_listener_local()
				.info(move |info| {
					// Full props (unlike the registry global) carry
					// `card.profile.device`, needed to address the hardware route.
					if let Some(props) = info.props() {
						let rd = props
							.get("card.profile.device")
							.and_then(|s| s.parse::<i32>().ok());
						let did = props.get("device.id").and_then(|s| s.parse::<u32>().ok());
						if let Some(n) = inner_info.borrow_mut().nodes.get_mut(&id) {
							if rd.is_some() {
								n.route_device = rd;
							}
							if did.is_some() {
								n.device_id = did;
							}
						}
					}
				})
				.param(move |_seq, param_type, _index, _next, pod| {
					if param_type == pipewire::spa::param::ParamType::Props
						&& let Some(pod) = pod
					{
						update_node_from_props(&inner_param, id, pod);
					}
				})
				.register();

			// Ask to be notified of the Props param now and on change.
			node.subscribe_params(&[pipewire::spa::param::ParamType::Props]);

			log::info!(
				"tracking {} {id} '{}'",
				if is_sink {
					"sink"
				} else if is_source {
					"source"
				} else {
					"stream"
				},
				if is_stream {
					app_name.as_deref().unwrap_or("")
				} else {
					&name
				}
			);
			inner.borrow_mut().nodes.insert(
				id,
				NodeRef {
					name,
					description,
					is_sink,
					is_source,
					is_stream,
					app_name: if is_stream { app_name } else { None },
					device_id,
					route_device,
					volume_cubic: 0.0,
					mute: false,
					proxy: node,
					_listener: listener,
				},
			);
			// A new sink/source for a known device can mean its hardware profile
			// changed (e.g. Astro A50 game<->chat); re-read that device's routes
			// so the new route index/device is available for volume control.
			if let Some(dev_id) = device_id
				&& let Some(dev) = inner.borrow().devices.get(&dev_id)
			{
				dev.proxy
					.enum_params(0, Some(pipewire::spa::param::ParamType::Route), 0, u32::MAX);
			}

			let b = inner.borrow();
			b.refresh_sinks();
			b.refresh_sources();
			b.refresh_apps();
		}

		ObjectType::Metadata => {
			// The "default" metadata holds the default sink/source names.
			if props.get("metadata.name") != Some("default") {
				return;
			}
			let metadata: pipewire::metadata::Metadata = match registry.bind(global) {
				Ok(m) => m,
				Err(e) => {
					log::warn!("Failed to bind 'default' metadata: {e}");
					return;
				}
			};

			let inner_m = inner.clone();
			let listener = metadata
				.add_listener_local()
				.property(move |_subject, key, _type, value| {
					// Value is JSON, e.g. {"name":"alsa_output...game"}.
					let parsed = value
						.and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
						.and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_owned));
					match (key, parsed) {
						(Some("default.audio.sink"), Some(name)) => {
							log::info!("default sink = '{name}'");
							inner_m.borrow_mut().default_sink_name = Some(name);
							inner_m.borrow().publish();
						}
						(Some("default.audio.source"), Some(name)) => {
							log::info!("default source = '{name}'");
							inner_m.borrow_mut().default_source_name = Some(name);
							inner_m.borrow().publish();
						}
						_ => {}
					}
					0
				})
				.register();

			inner.borrow_mut()._metadata = Some((metadata, listener));
		}

		ObjectType::Device => {
			// Track audio cards so we can set hardware (route) volume.
			if !props.get("media.class").unwrap_or("").starts_with("Audio/") {
				return;
			}
			let device: pipewire::device::Device = match registry.bind(global) {
				Ok(d) => d,
				Err(e) => {
					log::warn!("Failed to bind device {}: {e}", global.id);
					return;
				}
			};
			let id = global.id;
			let inner_p = inner.clone();
			let inner_di = inner.clone();
			let listener = device
				.add_listener_local()
				.info(move |_info| {
					// The device emits info whenever its profile/routes change
					// (e.g. Astro A50 game<->chat). Re-enumerate routes so the
					// newly-active route index/device gets captured — `subscribe`
					// alone does not reliably deliver it, and a one-shot re-enum on
					// node-add fires too early (before the route settles).
					if let Some(dev) = inner_di.borrow().devices.get(&id) {
						dev.proxy.enum_params(
							0,
							Some(pipewire::spa::param::ParamType::Route),
							0,
							u32::MAX,
						);
					}
				})
				.param(move |_seq, param_type, _index, _next, pod| {
					if param_type == pipewire::spa::param::ParamType::Route
						&& let Some(pod) = pod
					{
						update_device_route(&inner_p, id, pod);
					}
				})
				.register();
			device.subscribe_params(&[pipewire::spa::param::ParamType::Route]);
			device.enum_params(0, Some(pipewire::spa::param::ParamType::Route), 0, u32::MAX);

			inner.borrow_mut().devices.insert(
				id,
				DeviceRef {
					proxy: device,
					routes: HashMap::new(),
					_listener: listener,
				},
			);
		}

		_ => {}
	}

	// Fallback while metadata wiring is pending: if exactly one sink exists,
	// treat it as default so volume/mute already work on single-output setups.
	{
		let mut b = inner.borrow_mut();
		if b.default_sink_name.is_none() {
			let sinks: Vec<String> = b
				.nodes
				.values()
				.filter(|n| n.is_sink)
				.map(|n| n.name.clone())
				.collect();
			if sinks.len() == 1 {
				b.default_sink_name = Some(sinks[0].clone());
			}
		}
	}
	inner.borrow().publish();
}

fn on_global_remove(inner: &Rc<RefCell<Inner>>, id: u32) {
	{
		let mut b = inner.borrow_mut();
		b.nodes.remove(&id);
		b.devices.remove(&id);
	}
	let b = inner.borrow();
	b.refresh_sinks();
	b.refresh_sources();
	b.refresh_apps();
}

fn on_command(inner: &Rc<RefCell<Inner>>, cmd: Command) {
	// Commands that don't target the default sink are handled first.
	match &cmd {
		Command::SetDefaultSink(name) => {
			let b = inner.borrow();
			match b._metadata.as_ref() {
				Some((metadata, _)) => {
					let value = serde_json::json!({ "name": name }).to_string();
					metadata.set_property(
						0,
						"default.audio.sink",
						Some("Spa:String:JSON"),
						Some(&value),
					);
					log::info!("set default sink -> {name}");
				}
				None => log::warn!("No 'default' metadata available; cannot switch sink"),
			}
			return;
		}
		Command::AdjustAppVolume(app, delta) => {
			let b = inner.borrow();
			let mut matched = 0;
			for n in b.nodes.values().filter(|n| n.is_stream) {
				if n.app_name
					.as_deref()
					.is_some_and(|a| a.eq_ignore_ascii_case(app))
				{
					let target = (n.volume_cubic + delta).clamp(0.0, 1.5);
					set_node_props(&n.proxy, Some(cubic_to_linear(target)), None);
					matched += 1;
				}
			}
			if matched == 0 {
				log::warn!("No active streams for app '{app}'");
			}
			return;
		}
		Command::SetAppMute(app, value) => {
			let b = inner.borrow();
			for n in b.nodes.values().filter(|n| n.is_stream) {
				if n.app_name
					.as_deref()
					.is_some_and(|a| a.eq_ignore_ascii_case(app))
				{
					let mute = value.unwrap_or(!n.mute);
					set_node_props(&n.proxy, None, Some(mute));
				}
			}
			return;
		}
		Command::AdjustSinkVolume(name, delta) => {
			let b = inner.borrow();
			match b.sink_id_by_name(name) {
				Some(id) => {
					let target = (b.nodes[&id].volume_cubic + delta).clamp(0.0, 1.5);
					b.apply_node(id, Some(cubic_to_linear(target)), None);
				}
				None => log::warn!("No sink named '{name}'"),
			}
			return;
		}
		Command::SetSinkMute(name, value) => {
			let b = inner.borrow();
			if let Some(id) = b.sink_id_by_name(name) {
				let mute = value.unwrap_or(!b.nodes[&id].mute);
				b.apply_node(id, None, Some(mute));
			} else {
				log::warn!("No sink named '{name}'");
			}
			return;
		}

		// --- Input / source side ---
		Command::SetDefaultSource(name) => {
			let b = inner.borrow();
			match b._metadata.as_ref() {
				Some((metadata, _)) => {
					let value = serde_json::json!({ "name": name }).to_string();
					metadata.set_property(
						0,
						"default.audio.source",
						Some("Spa:String:JSON"),
						Some(&value),
					);
					log::info!("set default source -> {name}");
				}
				None => log::warn!("No 'default' metadata available; cannot switch source"),
			}
			return;
		}
		Command::AdjustSourceVolume(name, delta) => {
			let b = inner.borrow();
			match b.source_id_by_name(name) {
				Some(id) => {
					let target = (b.nodes[&id].volume_cubic + delta).clamp(0.0, 1.5);
					b.apply_node(id, Some(cubic_to_linear(target)), None);
				}
				None => log::warn!("No source named '{name}'"),
			}
			return;
		}
		Command::SetSourceMute(name, value) => {
			let b = inner.borrow();
			if let Some(id) = b.source_id_by_name(name) {
				let mute = value.unwrap_or(!b.nodes[&id].mute);
				b.apply_node(id, None, Some(mute));
			} else {
				log::warn!("No source named '{name}'");
			}
			return;
		}
		Command::AdjustDefaultSourceVolume(delta) => {
			let b = inner.borrow();
			match b.default_source_id() {
				Some(id) => {
					let target = (b.nodes[&id].volume_cubic + delta).clamp(0.0, 1.5);
					b.apply_node(id, Some(cubic_to_linear(target)), None);
				}
				None => log::warn!("No default source resolved yet; ignoring command"),
			}
			return;
		}
		Command::SetDefaultSourceMute(value) => {
			let b = inner.borrow();
			match b.default_source_id() {
				Some(id) => {
					let mute = value.unwrap_or(!b.nodes[&id].mute);
					b.apply_node(id, None, Some(mute));
				}
				None => log::warn!("No default source resolved yet; ignoring command"),
			}
			return;
		}
		_ => {}
	}

	let b = inner.borrow();
	let Some(id) = b.default_sink_id() else {
		log::warn!("No default sink resolved yet; ignoring command");
		return;
	};
	let node = &b.nodes[&id];

	match cmd {
		Command::AdjustVolume(delta) => {
			let target = (node.volume_cubic + delta).clamp(0.0, 1.5);
			b.apply_node(id, Some(cubic_to_linear(target)), None);
		}
		Command::SetMute(value) => {
			let mute = value.unwrap_or(!node.mute);
			b.apply_node(id, None, Some(mute));
		}
		_ => unreachable!("handled above"),
	}
	// The actual cached volume/mute updates when the Props param event arrives
	// (update_node_from_props), which then re-publishes the snapshot.
}

/// Parse a Props POD coming from a node and cache channelVolumes + mute.
fn update_node_from_props(inner: &Rc<RefCell<Inner>>, id: u32, pod: &pipewire::spa::pod::Pod) {
	use pipewire::spa::pod::Value;
	use pipewire::spa::pod::deserialize::PodDeserializer;

	let Ok((_, value)) = PodDeserializer::deserialize_from::<Value>(pod.as_bytes()) else {
		return;
	};
	let Value::Object(obj) = value else { return };

	let mut linear_avg: Option<f32> = None;
	let mut mute: Option<bool> = None;
	for prop in obj.properties {
		match prop.key {
			pipewire::spa::sys::SPA_PROP_channelVolumes => {
				if let Value::ValueArray(pipewire::spa::pod::ValueArray::Float(vals)) = prop.value
					&& !vals.is_empty()
				{
					linear_avg = Some(vals.iter().sum::<f32>() / vals.len() as f32);
				}
			}
			pipewire::spa::sys::SPA_PROP_mute => {
				if let Value::Bool(m) = prop.value {
					mute = Some(m);
				}
			}
			_ => {}
		}
	}

	let mut b = inner.borrow_mut();
	if let Some(n) = b.nodes.get_mut(&id) {
		if let Some(l) = linear_avg {
			n.volume_cubic = linear_to_cubic(l);
		}
		if let Some(m) = mute {
			n.mute = m;
		}
	}
	drop(b);
	let b = inner.borrow();
	b.refresh_sinks();
	b.refresh_sources();
	b.publish();
}

/// Build and send a Props POD setting channelVolumes and/or mute on a node.
fn set_node_props(node: &pipewire::node::Node, linear_volume: Option<f32>, mute: Option<bool>) {
	use pipewire::spa::param::ParamType;
	use pipewire::spa::pod::{
		Object, Property, PropertyFlags, Value, ValueArray, serialize::PodSerializer,
	};

	let mut properties: Vec<Property> = Vec::new();
	if let Some(v) = linear_volume {
		// Apply to a stereo pair; PipeWire fans out / matches channel count.
		properties.push(Property {
			key: pipewire::spa::sys::SPA_PROP_channelVolumes,
			flags: PropertyFlags::empty(),
			value: Value::ValueArray(ValueArray::Float(vec![v, v])),
		});
	}
	if let Some(m) = mute {
		properties.push(Property {
			key: pipewire::spa::sys::SPA_PROP_mute,
			flags: PropertyFlags::empty(),
			value: Value::Bool(m),
		});
	}
	if properties.is_empty() {
		return;
	}

	let object = Value::Object(Object {
		type_: pipewire::spa::sys::SPA_TYPE_OBJECT_Props,
		id: pipewire::spa::sys::SPA_PARAM_Props,
		properties,
	});

	let mut bytes = Vec::new();
	if PodSerializer::serialize(std::io::Cursor::new(&mut bytes), &object).is_err() {
		log::warn!("Failed to serialize Props POD");
		return;
	}
	match pipewire::spa::pod::Pod::from_bytes(&bytes) {
		Some(pod) => node.set_param(ParamType::Props, 0, pod),
		None => log::warn!("Failed to build Props POD from bytes"),
	}
}

/// Parse a Route POD from a device and cache (index, channels) by route device.
fn update_device_route(inner: &Rc<RefCell<Inner>>, dev_id: u32, pod: &pipewire::spa::pod::Pod) {
	use pipewire::spa::pod::deserialize::PodDeserializer;
	use pipewire::spa::pod::{Value, ValueArray};

	let Ok((_, value)) = PodDeserializer::deserialize_from::<Value>(pod.as_bytes()) else {
		return;
	};
	let Value::Object(obj) = value else { return };

	let mut index: Option<i32> = None;
	let mut route_device: Option<i32> = None;
	let mut channels: usize = 2;
	let mut linear_avg: Option<f32> = None;
	let mut mute: Option<bool> = None;
	for prop in obj.properties {
		match prop.key {
			pipewire::spa::sys::SPA_PARAM_ROUTE_index => {
				if let Value::Int(i) = prop.value {
					index = Some(i);
				}
			}
			pipewire::spa::sys::SPA_PARAM_ROUTE_device => {
				if let Value::Int(d) = prop.value {
					route_device = Some(d);
				}
			}
			pipewire::spa::sys::SPA_PARAM_ROUTE_props => {
				if let Value::Object(p) = prop.value {
					for pp in p.properties {
						match pp.key {
							pipewire::spa::sys::SPA_PROP_channelVolumes => {
								if let Value::ValueArray(ValueArray::Float(v)) = pp.value
									&& !v.is_empty()
								{
									channels = v.len();
									linear_avg = Some(v.iter().sum::<f32>() / v.len() as f32);
								}
							}
							pipewire::spa::sys::SPA_PROP_mute => {
								if let Value::Bool(m) = pp.value {
									mute = Some(m);
								}
							}
							_ => {}
						}
					}
				}
			}
			_ => {}
		}
	}

	let (Some(index), Some(route_device)) = (index, route_device) else {
		return;
	};

	let mut b = inner.borrow_mut();
	let Some(dev) = b.devices.get_mut(&dev_id) else {
		return;
	};
	dev.routes
		.insert(route_device, RouteInfo { index, channels });

	// Propagate an externally-changed hardware volume/mute (wpctl, pavucontrol,
	// media keys…) to the node(s) sitting on this route. Route-controlled devices
	// don't reliably mirror the route into their node Props, so without this the
	// plugin would never see out-of-band changes to a hardware device's volume.
	let mut changed = false;
	for n in b.nodes.values_mut() {
		if n.device_id == Some(dev_id) && n.route_device == Some(route_device) {
			if let Some(l) = linear_avg {
				n.volume_cubic = linear_to_cubic(l);
				changed = true;
			}
			if let Some(m) = mute {
				n.mute = m;
				changed = true;
			}
		}
	}
	drop(b);

	if changed {
		let b = inner.borrow();
		b.refresh_sinks();
		b.refresh_sources();
		b.publish();
	}
}

/// Set channelVolumes and/or mute on a device's route (hardware volume), the way
/// `wpctl` does — required for devices with a hardware mixer (USB headsets etc.).
fn set_device_route(
	device: &pipewire::device::Device,
	index: i32,
	route_device: i32,
	linear_volume: Option<f32>,
	mute: Option<bool>,
	channels: usize,
) {
	use pipewire::spa::param::ParamType;
	use pipewire::spa::pod::{
		Object, Property, PropertyFlags, Value, ValueArray, serialize::PodSerializer,
	};

	let mut props: Vec<Property> = Vec::new();
	if let Some(v) = linear_volume {
		props.push(Property {
			key: pipewire::spa::sys::SPA_PROP_channelVolumes,
			flags: PropertyFlags::empty(),
			value: Value::ValueArray(ValueArray::Float(vec![v; channels.max(1)])),
		});
	}
	if let Some(m) = mute {
		props.push(Property {
			key: pipewire::spa::sys::SPA_PROP_mute,
			flags: PropertyFlags::empty(),
			value: Value::Bool(m),
		});
	}
	if props.is_empty() {
		return;
	}

	let props_obj = Value::Object(Object {
		type_: pipewire::spa::sys::SPA_TYPE_OBJECT_Props,
		id: pipewire::spa::sys::SPA_PARAM_Props,
		properties: props,
	});

	let route = Value::Object(Object {
		type_: pipewire::spa::sys::SPA_TYPE_OBJECT_ParamRoute,
		id: pipewire::spa::sys::SPA_PARAM_Route,
		properties: vec![
			Property {
				key: pipewire::spa::sys::SPA_PARAM_ROUTE_index,
				flags: PropertyFlags::empty(),
				value: Value::Int(index),
			},
			Property {
				key: pipewire::spa::sys::SPA_PARAM_ROUTE_device,
				flags: PropertyFlags::empty(),
				value: Value::Int(route_device),
			},
			Property {
				key: pipewire::spa::sys::SPA_PARAM_ROUTE_props,
				flags: PropertyFlags::empty(),
				value: props_obj,
			},
			Property {
				key: pipewire::spa::sys::SPA_PARAM_ROUTE_save,
				flags: PropertyFlags::empty(),
				value: Value::Bool(true),
			},
		],
	});

	let mut bytes = Vec::new();
	if PodSerializer::serialize(std::io::Cursor::new(&mut bytes), &route).is_err() {
		log::warn!("Failed to serialize Route POD");
		return;
	}
	match pipewire::spa::pod::Pod::from_bytes(&bytes) {
		Some(pod) => device.set_param(ParamType::Route, 0, pod),
		None => log::warn!("Failed to build Route POD from bytes"),
	}
}

/// PipeWire `channelVolumes` are linear; the user-facing scale is cubic.
fn cubic_to_linear(cubic: f32) -> f32 {
	cubic.powi(3)
}
fn linear_to_cubic(linear: f32) -> f32 {
	linear.cbrt()
}
