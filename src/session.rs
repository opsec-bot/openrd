//! A live connection to a detector: an event stream plus (for Uniden) the
//! commands that write settings and mute.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use btleplug::api::Peripheral as _;
use btleplug::platform::Peripheral;
use futures::StreamExt;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::Stream;

use crate::alert::Event;
use crate::detect::{self, DetectorKind};
use crate::error::{Error, Result};
use crate::transport::{self, Channels};

/// How long to wait for a detector to acknowledge a command.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
/// Capacity of the event broadcast buffer. Slow consumers lag rather than
/// block the reader.
const EVENT_BUFFER: usize = 256;

/// A connected detector session.
///
/// Dropping the session aborts the background reader and leaves the BLE
/// connection open; call [`Session::disconnect`] to tear the link down.
pub struct Session {
    peripheral: Peripheral,
    kind: DetectorKind,
    channels: Channels,
    events_tx: broadcast::Sender<Event>,
    pending_reply: Arc<Mutex<Option<oneshot::Sender<String>>>>,
    command_guard: tokio::sync::Mutex<()>,
    reader: JoinHandle<()>,
}

impl Session {
    /// Open a session on an already-identified peripheral.
    pub(crate) async fn start(peripheral: Peripheral, kind: DetectorKind) -> Result<Session> {
        let channels = transport::open(&peripheral, &kind.service_uuids()).await?;
        peripheral.subscribe(&channels.notify).await?;

        let (events_tx, _) = broadcast::channel(EVENT_BUFFER);
        let pending_reply: Arc<Mutex<Option<oneshot::Sender<String>>>> =
            Arc::new(Mutex::new(None));

        let reader = spawn_reader(
            peripheral.clone(),
            kind,
            events_tx.clone(),
            pending_reply.clone(),
        )
        .await?;

        Ok(Session {
            peripheral,
            kind,
            channels,
            events_tx,
            pending_reply,
            command_guard: tokio::sync::Mutex::new(()),
            reader,
        })
    }

    /// The detector model this session is connected to.
    pub fn kind(&self) -> DetectorKind {
        self.kind
    }

    /// A stream of decoded events (alerts and, for Valentine, raw ESP packets).
    ///
    /// Each call returns an independent subscriber; events emitted before a
    /// subscriber is created are not replayed.
    pub fn events(&self) -> impl Stream<Item = Event> {
        BroadcastStream::new(self.events_tx.subscribe()).filter_map(|r| async move { r.ok() })
    }

    /// Mute the current alert. **Uniden only.**
    pub async fn mute(&self) -> Result<()> {
        let (cmd, bytes) = detect::uniden_mute_command(true);
        self.send_uniden_command(cmd, bytes, |reply| {
            reply.starts_with(detect::UNIDEN_MUTE_ACK_PREFIX) && reply.ends_with("=1")
        })
        .await
    }

    /// Unmute. **Uniden only.**
    pub async fn unmute(&self) -> Result<()> {
        let (cmd, bytes) = detect::uniden_mute_command(false);
        // The detector still replies on the mute channel; accept any mute ack.
        self.send_uniden_command(cmd, bytes, |reply| {
            reply.starts_with(detect::UNIDEN_MUTE_ACK_PREFIX)
        })
        .await
    }

    /// Write a single detector setting by its storage index. **Uniden only.**
    ///
    /// `index` and `value` come from the detector's menu map (the `write_index`
    /// / `byte_value` pair documented in `PROTOCOL.md`). This is deliberately
    /// low-level: openrd does not ship the full settings schema, so callers map
    /// human settings to indices themselves.
    pub async fn set_setting(&self, index: u16, value: u8) -> Result<()> {
        let (cmd, bytes) = detect::uniden_set_command(index, value);
        self.send_uniden_command(cmd, bytes, |reply| reply == detect::UNIDEN_SET_ACK)
            .await
    }

    /// Serialize a command, write it, and wait for the detector's ack on the
    /// notify channel.
    async fn send_uniden_command(
        &self,
        cmd: String,
        bytes: Vec<u8>,
        ack_ok: impl Fn(&str) -> bool,
    ) -> Result<()> {
        if !self.kind.is_uniden() {
            return Err(Error::Unsupported(self.kind));
        }
        let write_char = self
            .channels
            .write
            .as_ref()
            .ok_or(Error::CharacteristicNotFound("write"))?;

        // One command in flight at a time: the ack is correlated purely by
        // ordering, so overlapping commands would cross their replies.
        let _guard = self.command_guard.lock().await;

        let (tx, rx) = oneshot::channel();
        *self.pending_reply.lock().unwrap() = Some(tx);

        self.peripheral
            .write(write_char, &bytes, self.channels.write_type)
            .await?;

        let reply = match tokio::time::timeout(COMMAND_TIMEOUT, rx).await {
            Ok(Ok(reply)) => reply,
            // Timeout or the reader dropped the sender: clear any stale slot.
            _ => {
                *self.pending_reply.lock().unwrap() = None;
                return Err(Error::CommandTimeout(cmd));
            }
        };

        if ack_ok(&reply) {
            Ok(())
        } else {
            Err(Error::CommandRejected { sent: cmd, got: reply })
        }
    }

    /// Unsubscribe and disconnect from the detector.
    pub async fn disconnect(self) -> Result<()> {
        self.reader.abort();
        let _ = self.peripheral.unsubscribe(&self.channels.notify).await;
        self.peripheral.disconnect().await?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// Spawn the background task that pumps GATT notifications into the event
/// broadcast and routes command replies to the waiting command.
async fn spawn_reader(
    peripheral: Peripheral,
    kind: DetectorKind,
    events_tx: broadcast::Sender<Event>,
    pending_reply: Arc<Mutex<Option<oneshot::Sender<String>>>>,
) -> Result<JoinHandle<()>> {
    let mut notifications = peripheral.notifications().await?;
    let handle = tokio::spawn(async move {
        while let Some(n) = notifications.next().await {
            let value = n.value;

            // Uniden command replies ride the notify channel; peel them off and
            // hand them to whoever is awaiting a command ack.
            if kind.is_uniden() {
                let text = String::from_utf8_lossy(&value);
                if text.starts_with(detect::UNIDEN_REPLY_PREFIX) {
                    if let Some(tx) = pending_reply.lock().unwrap().take() {
                        let _ = tx.send(text.into_owned());
                    }
                    continue;
                }
            }

            for event in kind.decode(&value) {
                // Ignore send errors: they only mean there are no subscribers.
                let _ = events_tx.send(event);
            }
        }
    });
    Ok(handle)
}
