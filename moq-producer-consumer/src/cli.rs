use clap::Parser;
use std::{net, path};
use url::Url;

#[derive(Parser, Clone)]
pub struct Config {
	/// Listen for UDP packets on the given address.
	#[arg(long, default_value = "[::]:0")]
	pub bind: net::SocketAddr,

	/// Connect to the given URL starting with https://
	#[arg(value_parser = moq_url)]
	pub url: Url,

	#[command(flatten)]
	pub tls: moq_native::tls::Cli,

	/// Publish the current time to the relay, otherwise only subscribe.
	#[arg(long)]
	pub publish: bool,

	/// The namespace of the producer track.
	#[arg(long, default_value = ".")]
	pub namespace: String,

	/// The name of the producer track.
	#[arg(long, default_value = ".xyz.abc")]
	pub track: String,
}

fn moq_url(s: &str) -> Result<Url, String> {
	Url::try_from(s).map_err(|e| e.to_string())
}
