use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

use crate::error::TellError;

/// TCP transport with auto-reconnect.
pub(crate) struct TcpTransport {
    endpoint: String,
    stream: Option<BufWriter<TcpStream>>,
    connect_timeout: Duration,
}

impl TcpTransport {
    pub fn new(endpoint: String, connect_timeout: Duration) -> Self {
        Self {
            endpoint,
            stream: None,
            connect_timeout,
        }
    }

    /// Ensure we have a live connection, reconnecting if needed.
    pub async fn ensure_connected(&mut self) -> Result<(), TellError> {
        if self.stream.is_some() {
            return Ok(());
        }
        self.connect().await
    }

    /// Connect to the endpoint.
    async fn connect(&mut self) -> Result<(), TellError> {
        let stream = tokio::time::timeout(self.connect_timeout, TcpStream::connect(&self.endpoint))
            .await
            .map_err(|_| TellError::network(format!("connection timeout to {}", self.endpoint)))?
            .map_err(TellError::Io)?;

        stream.set_nodelay(true).ok();
        self.stream = Some(BufWriter::new(stream));
        Ok(())
    }

    /// Send a length-prefixed frame: [4 bytes BE length][payload].
    pub async fn send_frame(&mut self, data: &[u8]) -> Result<(), TellError> {
        self.ensure_connected().await?;

        let Some(writer) = self.stream.as_mut() else {
            return Err(TellError::network("connection not established"));
        };
        let len = data.len() as u32;

        if let Err(e) = async {
            writer.write_all(&len.to_be_bytes()).await?;
            writer.write_all(data).await?;
            writer.flush().await?;
            Ok::<(), std::io::Error>(())
        }
        .await
        {
            // Connection broken, clear it so next call reconnects
            self.stream = None;
            return Err(TellError::Io(e));
        }

        Ok(())
    }

    /// Close the connection.
    pub async fn close(&mut self) {
        if let Some(mut writer) = self.stream.take() {
            let _ = writer.get_mut().shutdown().await;
        }
    }
}
