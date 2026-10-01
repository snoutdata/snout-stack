//! snout-stack: the whole stack on one machine.
//!
//!   snout-stack init [--out <file>] [--force]   write every secret the stack needs, as a .env
//!   snout-stack setup                           prepare the database and register the project (one-shot)
//!   snout-stack gateway                         serve the front door
//!   snout-stack functions [--source <dir>] [--out <dir>]
//!                                               lay a folder of functions out for the runtime
//!
//! `compose.yaml` beside this crate runs `setup` once before the services that need it, and
//! `gateway` for as long as the stack is up.

mod env;
mod functions;
mod gateway;
mod init;
mod keys;
mod routes;
mod s3;
mod setup;

use std::process::ExitCode;

const USAGE: &str = "usage: snout-stack <init [--out <file>] [--force] | setup | gateway | functions [--source <dir>] [--out <dir>] | version>";

fn main() -> ExitCode {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let Some(command) = args.first() else {
		eprintln!("{USAGE}");
		return ExitCode::from(2);
	};
	if command == "init" {
		return report(init::run(&args[1..]));
	}
	if command == "functions" {
		return report(functions::run(&args[1..]));
	}
	if command == "version" || command == "--version" {
		println!("snout-stack {}", env!("CARGO_PKG_VERSION"));
		return ExitCode::SUCCESS;
	}
	logging();
	let runtime = match tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
	{
		Ok(runtime) => runtime,
		Err(error) => {
			eprintln!("cannot start: {error}");
			return ExitCode::FAILURE;
		}
	};
	match command.as_str() {
		"gateway" => report(runtime.block_on(gateway::run())),
		"setup" => report(runtime.block_on(setup::run())),
		_ => {
			eprintln!("{USAGE}");
			ExitCode::from(2)
		}
	}
}

fn report(result: Result<(), String>) -> ExitCode {
	match result {
		Ok(()) => ExitCode::SUCCESS,
		Err(message) => {
			eprintln!("snout-stack: {message}");
			ExitCode::FAILURE
		}
	}
}

fn logging() {
	let filter = tracing_subscriber::EnvFilter::try_from_env("LOG_LEVEL")
		.unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
	tracing_subscriber::fmt()
		.with_env_filter(filter)
		.with_target(false)
		.init();
}

/// SIGTERM (what `docker compose down` sends) or Ctrl-C.
pub async fn shutdown_signal() {
	let interrupt = tokio::signal::ctrl_c();
	#[cfg(unix)]
	{
		let mut terminate =
			match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
				Ok(signal) => signal,
				Err(_) => {
					let _ = interrupt.await;
					return;
				}
			};
		tokio::select! {
			_ = interrupt => {}
			_ = terminate.recv() => {}
		}
	}
	#[cfg(not(unix))]
	{
		let _ = interrupt.await;
	}
}
