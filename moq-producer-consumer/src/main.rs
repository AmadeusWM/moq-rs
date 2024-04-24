use std::{fs, io, sync::Arc, time};

use anyhow::Context;
use clap::Parser;

mod cli;
mod producer_consumer;

use moq_native::quic;
use moq_transport::serve::{self, GroupsReader, TrackReader};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	env_logger::init();

	// Disable tracing so we don't get a bunch of Quinn spam.
	let tracer = tracing_subscriber::FmtSubscriber::builder()
		.with_max_level(tracing::Level::WARN)
		.finish();
	tracing::subscriber::set_global_default(tracer).unwrap();

	let config = cli::Config::parse();

	let tls = config.tls.load()?;

	// Create a list of acceptable root certificates.
	let quic = quic::Endpoint::new(quic::Config { bind: config.bind, tls })?;

	log::info!("connecting to server: url={}", config.url);

	let session = quic.client.connect(&config.url).await?;

	log::info!("connecting to server: url={}", config.url);
	run(session, config).await?;
	Ok(())
}

async fn run(session: web_transport::Session, config: cli::Config) -> anyhow::Result<()> {
	if config.publish {
		let (session, mut publisher) = moq_transport::session::Publisher::connect(session)
			.await
			.context("failed to create MoQ Transport session")?;

		let (mut broadcast, _, broadcast_sub) = serve::Tracks {
			namespace: config.namespace.clone(),
		}
		.produce();

		let track = broadcast.create(&config.track).unwrap();

		let producer = producer_consumer::Producer::new(track);

		tokio::select! {
			res = session.run() => res.context("session error")?,
			res = producer.run_objects() => res.context("producer error")?,
			res = publisher.announce(broadcast_sub) => res.context("failed to serve broadcast")?,
		};
	} else {
		observe_participants(session, config).await?;
	}
	Ok(())
}

async fn observe_participants(session: web_transport::Session, config: cli::Config) -> anyhow::Result<()> {
	let (session, mut subscriber) = moq_transport::session::Subscriber::connect(session)
		.await
		.context("failed to create MoQ Transport session")?;

	let (prod, sub) = serve::Track::new(config.namespace, config.track).produce();

	tokio::select! {
		res = session.run() => res.context("session error")?,
		res = run_participants(sub) => res.context("participant error")?,
		res = subscriber.subscribe(prod) => res.context("subscribe closed")?,
	}
	Ok(())
}

async fn run_participants(reader: TrackReader) -> anyhow::Result<()> {
	match reader.mode().await? {
		serve::TrackReaderMode::Stream(_) => todo!(),
		serve::TrackReaderMode::Groups(groups) => recv_groups(groups).await,
		serve::TrackReaderMode::Objects(_) => todo!(),
		serve::TrackReaderMode::Datagrams(_) => todo!(),
	}
}

async fn recv_groups(mut groups: GroupsReader) -> anyhow::Result<()> {
	while let Some(mut group) = groups.next().await? {
		let base = group
			.read_next()
			.await
			.context("failed to get first object")?
			.context("empty group")?;

		let base = String::from_utf8_lossy(&base);

		log::info!("base: {}", base);
		while let Some(object) = group.read_next().await? {
			let str = String::from_utf8_lossy(&object);
			log::info!("str: {}", str);
		}
	}

	Ok(())
}

async fn subscribe_on_participant(session: web_transport::Session, config: cli::Config) -> anyhow::Result<()> {
	let (session, mut subscriber) = moq_transport::session::Subscriber::connect(session)
		.await
		.context("failed to create MoQ Transport session")?;

	let (prod, sub) = serve::Track::new(config.namespace, config.track).produce();

	let consumer = producer_consumer::Consumer::new(sub);

	tokio::select! {
		res = session.run() => res.context("session error")?,
		res = consumer.run() => res.context("consumer error")?,
		res = subscriber.subscribe(prod) => res.context("subscribe closed")?,
	}
	Ok(())
}

pub struct NoCertificateVerification {}

impl rustls::client::ServerCertVerifier for NoCertificateVerification {
	fn verify_server_cert(
		&self,
		_end_entity: &rustls::Certificate,
		_intermediates: &[rustls::Certificate],
		_server_name: &rustls::ServerName,
		_scts: &mut dyn Iterator<Item = &[u8]>,
		_ocsp_response: &[u8],
		_now: time::SystemTime,
	) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
		Ok(rustls::client::ServerCertVerified::assertion())
	}
}
