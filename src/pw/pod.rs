//! Building and sending SPA Props / Route PODs — the low-level hardware writes.
//!
//! Both helpers take an already-linear volume (`linear = cubic³`, see the parent
//! module) and a mute flag; either may be `None` to leave that attribute alone.

/// Build and send a Props POD setting channelVolumes and/or mute on a node.
pub(super) fn set_node_props(
	node: &pipewire::node::Node,
	linear_volume: Option<f32>,
	mute: Option<bool>,
) {
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

/// Set channelVolumes and/or mute on a device's route (hardware volume), the way
/// `wpctl` does — required for devices with a hardware mixer (USB headsets etc.).
pub(super) fn set_device_route(
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
