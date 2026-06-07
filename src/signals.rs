//! Lifecycle signal utilities.
//!
//! grackle previously used Unix user signals as a recording control channel.
//! That was removed in favour of the `grackctl` Unix-socket IPC.
//! Only lifecycle signals are handled here: SIGTERM (process managers, pkill)
//! and SIGINT (Ctrl-C) both trigger graceful shutdown.

use tokio::signal::unix::{signal, Signal, SignalKind};

/// Streams for the lifecycle shutdown signals (SIGTERM and SIGINT).
///
/// Both signals are treated as shutdown requests; callers should `select!`
/// over [`ShutdownSignals::recv`] alongside their other work.
pub struct ShutdownSignals {
    sigterm: Signal,
    sigint: Signal,
}

impl ShutdownSignals {
    /// Register handlers for SIGTERM and SIGINT.
    ///
    /// # Errors
    ///
    /// Returns an error if signal registration fails.
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            sigterm: signal(SignalKind::terminate())?,
            sigint: signal(SignalKind::interrupt())?,
        })
    }

    /// Resolve when either SIGTERM or SIGINT is delivered.
    ///
    /// Returns `None` only if both underlying signal streams are closed, which
    /// does not happen in practice for process-lifetime handlers.
    pub async fn recv(&mut self) -> Option<()> {
        tokio::select! {
            v = self.sigterm.recv() => v,
            v = self.sigint.recv() => v,
        }
    }
}
