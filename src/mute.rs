//! Mute action: toggles the default sink (output) mute state.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::pw::PwHandle;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct MuteSettings {}

pub struct MuteAction {
	pub pw: PwHandle,
}

#[async_trait]
impl Action for MuteAction {
	const UUID: &'static str = "fr.jourdois.pipewire.mute";
	type Settings = MuteSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.pw.toggle_default_sink_mute();
		self.refresh(instance).await
	}

	async fn dial_down(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.pw.toggle_default_sink_mute();
		self.refresh(instance).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.refresh(instance).await
	}
}

impl MuteAction {
	async fn refresh(&self, instance: &Instance) -> OpenActionResult<()> {
		let snap = self.pw.default_sink_snapshot();
		// State 0 = unmuted, state 1 = muted (see manifest "States").
		let state = if snap.mute { 1 } else { 0 };
		instance.set_state(state).await
	}
}
