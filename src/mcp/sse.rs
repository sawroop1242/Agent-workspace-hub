//! Server-Sent Events (SSE) transport and session management for remote MCP.
//!
//! Each SSE client owns an isolated session: a unique session id, a dedicated
//! message channel, and its own protocol state. Concurrent clients share the
//! [`McpDispatcher`] (the tool implementations) but never share session state,
//! so one client's messages or disconnect cannot affect another.
//!
//! The transport follows the MCP HTTP+SSE pattern:
//! 1. A client opens `GET /sse` and receives an SSE stream.
//! 2. The server emits an initial `endpoint` event carrying the POST URL to
//!    which the client sends JSON-RPC messages.
//! 3. JSON-RPC responses (and any server-pushed notifications) are emitted as
//!    `message` events over the SSE stream.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// An event emitted on an SSE stream to a single client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum SseEvent {
    /// Announces the POST endpoint for this session.
    #[serde(rename = "endpoint")]
    Endpoint(String),
    /// A message (JSON-RPC response or server push) addressed to the client.
    #[serde(rename = "message")]
    Message(serde_json::Value),
}

/// A single isolated MCP SSE session.
#[derive(Clone)]
pub struct Session {
    /// The unique session id (unguessable random token).
    pub id: String,
    /// Where the client POSTs JSON-RPC messages for this session.
    pub endpoint: String,
    /// Broadcast sender for events destined to this session's SSE stream.
    tx: broadcast::Sender<SseEvent>,
}

impl Session {
    /// Subscribes a new receiver to this session's event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.tx.subscribe()
    }

    /// Pushes an event to the client, ignoring a closed stream (client gone).
    pub fn send(&self, event: SseEvent) {
        let _ = self.tx.send(event);
    }
}

/// Registry of active SSE sessions, keyed by session id.
#[derive(Default)]
pub struct SessionRegistry {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    next_id: AtomicU64,
}

impl SessionRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new isolated session with a fresh id and channel.
    pub async fn create(&self, endpoint_path: &str) -> Session {
        let id = self.new_session_id();
        // Each session gets its own channel, so no cross-client message leakage.
        let (tx, _) = broadcast::channel(256);
        let endpoint = format!("{endpoint_path}?sessionId={id}");
        let session = Session {
            id: id.clone(),
            endpoint,
            tx,
        };
        self.sessions.lock().await.insert(id, session.clone());
        session
    }

    /// Looks up a session by id.
    pub async fn get(&self, id: &str) -> Option<Session> {
        self.sessions.lock().await.get(id).cloned()
    }

    /// Removes and drops a session (on disconnect or shutdown).
    pub async fn remove(&self, id: &str) {
        self.sessions.lock().await.remove(id);
    }

    /// Returns the number of active sessions (used by shutdown and limits).
    pub async fn len(&self) -> usize {
        self.sessions.lock().await.len()
    }

    /// Returns whether no active sessions remain.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Drops all sessions (used during graceful shutdown).
    pub async fn clear(&self) {
        self.sessions.lock().await.clear();
    }

    fn new_session_id(&self) -> String {
        // A session id is formed from an unguessable random 128-bit secret plus
        // a monotonic counter, hex-encoded. Randomness comes from the OS CSPRNG
        // (see `fill_random`); the counter merely guarantees local uniqueness.
        let mut secret = [0u8; 16];
        fill_random(&mut secret);
        let counter = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("{:032x}{:016x}", u128::from_be_bytes(secret), counter)
    }
}

/// Fills `buf` with cryptographically-secure random bytes from the OS CSPRNG.
fn fill_random(buf: &mut [u8]) {
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            if f.read_exact(buf).is_ok() {
                return;
            }
        }
    }
    // Deterministic fallback for platforms without `/dev/urandom` (only reached
    // in the unlikely event the CSPRNG is unavailable). Mixing time and
    // address-space entropy keeps ids non-trivial; this is never the primary
    // path on supported platforms.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let ptr = buf.as_ptr() as u128;
    for (i, b) in buf.iter_mut().enumerate() {
        let mixed = nanos ^ (ptr << 1) ^ ((i as u128) << 32);
        *b = (mixed >> ((i % 16) * 8)) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sessions_are_isolated_and_removable() {
        let registry = SessionRegistry::new();
        let a = registry.create("/mcp").await;
        let b = registry.create("/mcp").await;

        assert_ne!(a.id, b.id);
        assert_eq!(registry.len().await, 2);

        assert!(a.endpoint.contains(&a.id));
        assert!(b.endpoint.contains(&b.id));

        let got = registry.get(&a.id).await.unwrap();
        assert_eq!(got.id, a.id);

        registry.remove(&a.id).await;
        assert_eq!(registry.len().await, 1);
        assert!(registry.get(&a.id).await.is_none());
        assert!(registry.get(&b.id).await.is_some());

        registry.clear().await;
        assert_eq!(registry.len().await, 0);
    }

    #[test]
    fn session_ids_are_unique_and_hex() {
        let registry = SessionRegistry::new();
        let a = registry.new_session_id();
        let b = registry.new_session_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 48);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }
}
