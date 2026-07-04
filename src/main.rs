//! OpenDeck PipeWire audio control plugin — native Rust, no Node.js.
//!
//! The plugin process is launched by OpenDeck and speaks the Elgato/OpenAction
//! WebSocket protocol via the `openaction` crate. All audio control goes through
//! a dedicated PipeWire main-loop thread (see [`pw`]).

mod actions;
mod color;
mod command;
mod display;
mod pw;
mod refresh;
mod render;

use actions::{
	app_volume, device_volume, input_volume, mic_volume, output, push_to_talk, switch_input, volume,
};
use openaction::*;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
	// Logs are written to stdout, which OpenDeck redirects into the plugin log
	// file (~/.config/opendeck/logs/plugins/<uuid>.log).
	{
		use simplelog::*;
		if let Err(error) = TermLogger::init(
			LevelFilter::Info,
			Config::default(),
			TerminalMode::Stdout,
			ColorChoice::Never,
		) {
			eprintln!("Logger initialization failed: {error}");
		}
	}

	// Spin up the PipeWire backend (its own OS thread; not Send-safe for tokio).
	let pw = match pw::start() {
		Ok(handle) => handle,
		Err(error) => {
			log::error!("Failed to start PipeWire backend: {error}");
			// Still connect so OpenDeck doesn't treat the plugin as crashed.
			return run(std::env::args().collect()).await;
		}
	};

	let refresher = refresh::Refresher::default();

	register_action(volume::VolumeAction {
		pw: pw.clone(),
		refresher: refresher.clone(),
	})
	.await;
	register_action(output::OutputAction {
		pw: pw.clone(),
		refresher: refresher.clone(),
	})
	.await;
	register_action(app_volume::AppVolumeAction {
		pw: pw.clone(),
		refresher: refresher.clone(),
	})
	.await;
	register_action(device_volume::DeviceVolumeAction {
		pw: pw.clone(),
		refresher: refresher.clone(),
	})
	.await;
	register_action(mic_volume::MicVolumeAction {
		pw: pw.clone(),
		refresher: refresher.clone(),
	})
	.await;
	register_action(input_volume::InputVolumeAction {
		pw: pw.clone(),
		refresher: refresher.clone(),
	})
	.await;
	register_action(switch_input::SwitchInputAction { pw: pw.clone() }).await;
	register_action(push_to_talk::PushToTalkAction { pw: pw.clone() }).await;

	// Re-render visible keys whenever PipeWire state changes out-of-band (volume
	// changed by wpctl, media keys, another app…). The PipeWire thread signals
	// changes on the watch channel; we coalesce bursts and redraw once each.
	{
		let pw = pw.clone();
		let refresher = refresher.clone();
		tokio::spawn(async move {
			let mut changes = pw.subscribe();
			loop {
				refresh::refresh_all(&pw, &refresher).await;
				if changes.changed().await.is_err() {
					break;
				}
			}
		});
	}

	run(std::env::args().collect()).await
}
