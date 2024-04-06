use std::time::Duration;

use anyhow::Context;
use bytes::Bytes;
use moq_transport::serve::{
	DatagramsReader, GroupsReader, Object, ObjectWriter, ObjectsReader, StreamReader, TrackReader, TrackReaderMode,
	TrackWriter,
};

use tokio::time::sleep;

pub struct Producer {
	track: TrackWriter,
}

impl Producer {
	pub fn new(track: TrackWriter) -> Self {
		Self { track }
	}

	pub async fn run_objects(self) -> anyhow::Result<()> {
		let mut objects_writer = self.track.objects()?;

		let objects_per_group = 1000;
		let payload_stuffing: Bytes = "XXXXXXXX".repeat(100).bytes().collect();
		let object_delay = Duration::from_millis(1);
		let group_delay = Duration::from_millis(20000);

		let mut object_id = 0;
		let mut group_id = 0;
		let mut priority = 10000;
		loop {
			if object_id >= objects_per_group {
				group_id += 1;
				object_id = 0;
				println!("producing group {}", group_id);
				print_header();
				sleep(group_delay).await;
			}

			let writer = objects_writer.create(Object {
				group_id,
				object_id,
				priority,
			})?;

			let payload = payload_stuffing.clone();
			tokio::spawn(async move {
				if let Err(err) = Self::send_object(writer, payload).await {
					log::warn!("Failed sending object: {}", err);
				}
			});

			sleep(object_delay).await;

			object_id += 1;
			priority -= 1;
		}
	}

	async fn send_object(mut object: ObjectWriter, payload: Bytes) -> anyhow::Result<()> {
		object.write(payload.clone())?;
		pretty_print_object(object.group_id, object.object_id, object.priority, payload);
		Ok(())
	}
}

pub struct Consumer {
	track: TrackReader,
}

impl Consumer {
	pub fn new(track: TrackReader) -> Self {
		Self { track }
	}

	pub async fn run(self) -> anyhow::Result<()> {
		match self.track.mode().await.context("failed to get mode")? {
			TrackReaderMode::Stream(stream) => Self::recv_stream(stream).await,
			TrackReaderMode::Groups(groups) => Self::recv_groups(groups).await,
			TrackReaderMode::Objects(objects) => Self::recv_objects(objects).await,
			TrackReaderMode::Datagrams(datagrams) => Self::recv_datagrams(datagrams).await,
		}
	}

	async fn recv_stream(mut track: StreamReader) -> anyhow::Result<()> {
		while let Some(mut group) = track.next().await? {
			while let Some(object) = group.read_next().await? {
				let str = String::from_utf8_lossy(&object);
				println!("{}", str);
			}
		}

		Ok(())
	}

	async fn recv_groups(mut groups: GroupsReader) -> anyhow::Result<()> {
		while let Some(mut group) = groups.next().await? {
			let base = group
				.read_next()
				.await
				.context("failed to get first object")?
				.context("empty group")?;

			let base = String::from_utf8_lossy(&base);

			while let Some(object) = group.read_next().await? {
				let str = String::from_utf8_lossy(&object);
				println!("{}{}", base, str);
			}
		}

		Ok(())
	}

	async fn recv_objects(mut objects: ObjectsReader) -> anyhow::Result<()> {
		let mut prev_group_id = None;
		println!("start consuming");
		while let Some(mut object) = objects.next().await? {
			if prev_group_id.is_none() || object.group_id != prev_group_id.unwrap() {
				prev_group_id = Some(object.group_id);
				println!("consuming group {}", object.group_id);
				print_header();
			}
			let payload = object.read_all().await?;
			pretty_print_object(object.group_id, object.object_id, object.priority, payload);
		}

		Ok(())
	}

	async fn recv_datagrams(mut datagrams: DatagramsReader) -> anyhow::Result<()> {
		while let Some(datagram) = datagrams.read().await? {
			let str = String::from_utf8_lossy(&datagram.payload);
			println!("{}", str);
		}

		Ok(())
	}
}

fn print_header() {
	println!(
		"{0: <10}| {1: <10}| {2: <10}| {3: <10}",
		"group_id", "object_id", "priority", "payload len"
	);
}

fn pretty_print_object(group_id: u64, object_id: u64, priority: u64, payload: Bytes) {
	println!(
		"{0: <10}| {1: <10}| {2: <10}| {3: <10}",
		group_id,
		object_id,
		priority,
		payload.len()
	);
}
