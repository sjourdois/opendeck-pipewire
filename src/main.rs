//! OpenDeck PipeWire audio control plugin — native Rust, no Node.js, no Wine.
//!
//! The plugin process is launched by OpenDeck and speaks the Elgato/OpenAction
//! WebSocket protocol via the `openaction` crate. All audio control goes through
//! a dedicated PipeWire main-loop thread (see [`pw`]).

mod app_volume;
mod device_volume;
mod input_volume;
mod mic_volume;
mod mute;
mod output;
mod push_to_talk;
mod pw;
mod render;
mod switch_input;
mod volume;

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

	register_action(volume::VolumeAction { pw: pw.clone() }).await;
	register_action(mute::MuteAction { pw: pw.clone() }).await;
	register_action(output::OutputAction { pw: pw.clone() }).await;
	register_action(app_volume::AppVolumeAction { pw: pw.clone() }).await;
	register_action(device_volume::DeviceVolumeAction { pw: pw.clone() }).await;
	register_action(mic_volume::MicVolumeAction { pw: pw.clone() }).await;
	register_action(input_volume::InputVolumeAction { pw: pw.clone() }).await;
	register_action(switch_input::SwitchInputAction { pw: pw.clone() }).await;
	register_action(push_to_talk::PushToTalkAction { pw: pw.clone() }).await;

	run(std::env::args().collect()).await
}
