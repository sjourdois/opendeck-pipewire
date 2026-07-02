//! Push to Talk action: unmutes the default source while the key is held, and
//! mutes it again on release (hold-to-talk). With `inverted`, it is push-to-mute.

use openaction::*;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::pw::PwHandle;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct PushToTalkSettings {
	/// If true, hold to **mute** (push-to-mute) instead of hold to talk.
	pub inverted: bool,
}

pub struct PushToTalkAction {
	pub pw: PwHandle,
}

// State 0 = mic open (talking), state 1 = mic muted.
fn state_for(mute: bool) -> u16 {
	if mute { 1 } else { 0 }
}

#[async_trait]
impl Action for PushToTalkAction {
	const UUID: &'static str = super::action_uuid!("pushtotalk");
	type Settings = PushToTalkSettings;

	async fn key_down(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		// Held: talk (unmute), or push-to-mute (mute) when inverted.
		let mute = settings.inverted;
		self.pw.send(Command::SetDefaultSourceMute(Some(mute)));
		instance.set_state(state_for(mute)).await
	}

	async fn key_up(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		// Released: back to the resting state (muted for talk, open for mute).
		let mute = !settings.inverted;
		self.pw.send(Command::SetDefaultSourceMute(Some(mute)));
		instance.set_state(state_for(mute)).await
	}

	async fn will_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		// Reflect the mic's actual state; don't presume it is muted before the key
		// has been used (the mic may still be open).
		instance
			.set_state(state_for(self.pw.default_source_snapshot().mute))
			.await
	}
}
