use std::io;

use crate::coding::{Encode, EncodeError};

use super::SessionError;
use bytes::Buf;
use web_transport::WriteError;

pub struct Writer {
	stream: web_transport::SendStream,
	buffer: bytes::BytesMut,
}

impl Writer {
	pub fn new(stream: web_transport::SendStream) -> Self {
		Self {
			stream,
			buffer: Default::default(),
		}
	}

	pub async fn encode<T: Encode>(&mut self, msg: &T) -> Result<(), SessionError> {
		self.buffer.clear();
		msg.encode(&mut self.buffer)?;

		while !self.buffer.is_empty() {
			self.stream.write_buf(&mut self.buffer).await?;
		}

		Ok(())
	}

	pub async fn write(&mut self, buf: &[u8]) -> Result<(), SessionError> {
		let mut cursor = io::Cursor::new(buf);

		while cursor.has_remaining() {
			let size = self.stream.write_buf(&mut cursor).await?;
			if size == 0 {
				return Err(EncodeError::More(cursor.remaining()).into());
			}
		}

		Ok(())
	}

	pub async fn finish(&mut self) -> Result<(), WriteError> {
		self.stream.finish().await
	}
}
