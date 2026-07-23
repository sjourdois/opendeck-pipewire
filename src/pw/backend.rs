//! The PipeWire main-loop thread.
//!
//! Everything here runs on the single dedicated PipeWire thread (not `Send`):
//! registry discovery of sink/source nodes, sound-card `Device`s and the
//! `default` metadata; handling commands from the actions; and reading back
//! node Props / device Route params to keep the published state current.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use anyhow::Result;

use crate::command::Command;

use super::pod::{set_device_route, set_node_props};
use super::{AppDesc, Channels, SinkDesc, SinkSnapshot};

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

	/// Republish the distinct applications producing audio, each with its
	/// aggregate volume/mute: the mean of its streams' volumes, and muted only
	/// when every stream is muted. The `BTreeMap` keeps the list name-sorted.
	fn refresh_apps(&self) {
		use std::collections::BTreeMap;

		let mut by_app: BTreeMap<String, (f32, u32, bool)> = BTreeMap::new();
		for n in self.nodes.values().filter(|n| n.is_stream) {
			let Some(app) = n.app_name.as_deref().filter(|s| !s.is_empty()) else {
				continue;
			};
			let entry = by_app.entry(app.to_owned()).or_insert((0.0, 0, true));
			entry.0 += n.volume_cubic;
			entry.1 += 1;
			entry.2 &= n.mute;
		}
		*self.chans.apps.lock().unwrap() = by_app
			.into_iter()
			.map(|(name, (sum, count, mute))| AppDesc {
				name,
				volume_cubic: sum / count as f32,
				mute,
			})
			.collect();
		self.bump();
	}

	fn default_sink_id(&self) -> Option<u32> {
		self.sink_id_by_name(self.default_sink_name.as_deref()?)
	}

	fn default_source_id(&self) -> Option<u32> {
		self.source_id_by_name(self.default_source_name.as_deref()?)
	}

	/// Recompute the published snapshots from the default sink / source state.
	fn publish(&self) {
		// Publish the resolved default names for the cycling pickers.
		*self.chans.default_sink_name.lock().unwrap() = self.default_sink_name.clone();
		*self.chans.default_source_name.lock().unwrap() = self.default_source_name.clone();
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

	/// Forget every tracked object after a disconnect: their proxies belong to the
	/// now-dead core, and the next connection rediscovers everything from a fresh
	/// registry. The emptied lists are republished so the UI reflects the gap.
	fn reset(&mut self) {
		self.nodes.clear();
		self.devices.clear();
		self._metadata = None;
		self.default_sink_name = None;
		self.default_source_name = None;
		self.refresh_sinks();
		self.refresh_sources();
		self.refresh_apps();
	}
}

/// How long to wait before re-dialing the daemon after a lost connection (also
/// the retry interval while it is still down, e.g. mid-`restart`).
const RECONNECT_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

pub(super) fn run_loop(rx: pipewire::channel::Receiver<Command>, chans: Channels) -> Result<()> {
	use pipewire::context::ContextRc;
	use pipewire::main_loop::MainLoopRc;

	pipewire::init();

	let main_loop = MainLoopRc::new(None)?;
	let inner = Rc::new(RefCell::new(Inner::new(chans)));

	// --- Commands from actions, delivered inside this loop ---
	// Attached once for the thread's whole life: the receiver survives reconnects,
	// so commands keep flowing after the PipeWire daemon is restarted.
	let inner_c = inner.clone();
	let _recv = rx.attach(main_loop.loop_(), move |cmd| {
		on_command(&inner_c, cmd);
	});

	// Reconnect loop: `systemctl --user restart pipewire` (or any daemon crash)
	// severs the connection and fires a fatal core error, which quits the inner
	// `run()`. We then drop the dead connection, clear the stale object state, and
	// dial back in — rediscovering everything from the fresh registry.
	loop {
		let context = ContextRc::new(&main_loop, None)?;
		let core = match context.connect_rc(None) {
			Ok(core) => core,
			Err(error) => {
				log::warn!("PipeWire connect failed: {error}; retrying");
				std::thread::sleep(RECONNECT_DELAY);
				continue;
			}
		};
		// A ref-counted registry so the `global` callback below can bind new objects.
		let registry = core.get_registry_rc()?;

		// A fatal error on the core object (id 0) — most often the daemon going
		// away — is our disconnect signal: quit the loop so we reconnect below.
		let ml = main_loop.clone();
		let _core_listener = core
			.add_listener_local()
			.error(move |id, _seq, res, message| {
				log::warn!("core error (id {id}, res {res}): {message}");
				if id == pipewire::core::PW_ID_CORE {
					ml.quit();
				}
			})
			.register();

		// --- Registry: discover audio sink nodes and the default-sink metadata ---
		let reg_for_cb = registry.clone();
		let inner_g = inner.clone();
		let inner_r = inner.clone();
		let _registry_listener = registry
			.add_listener_local()
			.global(move |global| on_global(&inner_g, &reg_for_cb, global))
			.global_remove(move |id| on_global_remove(&inner_r, id))
			.register();

		log::info!("PipeWire backend connected; entering main loop");
		main_loop.run();

		// `run()` only returns once the core error quit it, i.e. the connection
		// dropped. Clear the stale nodes/devices/metadata while their (dead) core
		// still exists, then let the connection objects drop and reconnect.
		inner.borrow_mut().reset();
		log::warn!("PipeWire connection lost; reconnecting in {RECONNECT_DELAY:?}");
		std::thread::sleep(RECONNECT_DELAY);
	}
}

fn on_global(
	inner: &Rc<RefCell<Inner>>,
	registry: &pipewire::registry::Registry,
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

	// Fallback while metadata wiring is pending: if exactly one sink / source
	// exists, treat it as default so volume/mute already work on single-device
	// setups (and Mute Mic knows the current state on the first press).
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
		if b.default_source_name.is_none() {
			let sources: Vec<String> = b
				.nodes
				.values()
				.filter(|n| n.is_source)
				.map(|n| n.name.clone())
				.collect();
			if sources.len() == 1 {
				b.default_source_name = Some(sources[0].clone());
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
					// Set the *configured* default (sticky, like `wpctl set-default`)
					// rather than the transient `default.audio.sink`: the latter can be
					// overridden by WirePlumber on its next default-node evaluation (e.g.
					// when another agent has set `default.configured.audio.sink`), so a
					// plain switch would not "stick".
					metadata.set_property(
						0,
						"default.configured.audio.sink",
						Some("Spa:String:JSON"),
						Some(&value),
					);
					log::info!("set configured default sink -> {name}");
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
			let streams: Vec<&NodeRef> = b
				.nodes
				.values()
				.filter(|n| n.is_stream)
				.filter(|n| {
					n.app_name
						.as_deref()
						.is_some_and(|a| a.eq_ignore_ascii_case(app))
				})
				.collect();
			// Toggle the app as a whole: it counts as muted only when every stream
			// is, so one press always flips all its streams together (keeping them
			// in sync rather than each toggling on its own state).
			let currently_muted = !streams.is_empty() && streams.iter().all(|n| n.mute);
			let mute = value.unwrap_or(!currently_muted);
			for n in streams {
				set_node_props(&n.proxy, None, Some(mute));
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
	b.refresh_apps();
	b.publish();
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

/// PipeWire `channelVolumes` are linear; the user-facing scale is cubic.
fn cubic_to_linear(cubic: f32) -> f32 {
	cubic.powi(3)
}
fn linear_to_cubic(linear: f32) -> f32 {
	linear.cbrt()
}
