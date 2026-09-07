//! Length-prefixed TCP transport with reconnect and timeouts.

use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

use crate::error::TellError;

/// TCP transport with auto-reconnect.
///
/// `network_timeout` bounds both the connect and each frame write, so a peer
/// that stops reading cannot stall the worker indefinitely.
pub(crate) struct TcpTransport {
    endpoint: String,
    stream: Option<BufWriter<TcpStream>>,
    network_timeout: Duration,
}

impl TcpTransport {
    /// Create an unconnected transport; the first send dials `endpoint`.
    pub fn new(endpoint: String, network_timeout: Duration) -> Self {
        Self {
            endpoint,
            stream: None,
            network_timeout,
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
        let stream = tokio::time::timeout(self.network_timeout, TcpStream::connect(&self.endpoint))
            .await
            .map_err(|_| TellError::network(format!("connection timeout to {}", self.endpoint)))?
            .map_err(TellError::Io)?;

        // Best effort: a failed NODELAY only costs latency, never correctness.
        stream.set_nodelay(true).ok();
        self.stream = Some(BufWriter::new(stream));
        Ok(())
    }

    /// Send a length-prefixed frame: [4 bytes BE length][payload].
    ///
    /// On any error or timeout the connection is dropped so the next call redials.
    pub async fn send_frame(&mut self, data: &[u8]) -> Result<(), TellError> {
        self.ensure_connected().await?;

        let Some(writer) = self.stream.as_mut() else {
            return Err(TellError::network("connection not established"));
        };
        let len = data.len() as u32;

        let write = async {
            writer.write_all(&len.to_be_bytes()).await?;
            writer.write_all(data).await?;
            writer.flush().await?;
            Ok::<(), std::io::Error>(())
        };

        match tokio::time::timeout(self.network_timeout, write).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                self.stream = None;
                Err(TellError::Io(e))
            }
            Err(_) => {
                self.stream = None;
                Err(TellError::network(format!(
                    "write timeout to {}",
                    self.endpoint
                )))
            }
        }
    }

    /// Close the connection.
    pub async fn close(&mut self) {
        if let Some(mut writer) = self.stream.take() {
            // Shutdown failures on close are expected when the peer already hung up.
            let _ = writer.get_mut().shutdown().await;
        }
    }
}
