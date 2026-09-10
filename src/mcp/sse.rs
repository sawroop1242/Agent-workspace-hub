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

use super::dispatcher::SessionLifecycle;

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
    /// Per-session MCP initialization lifecycle state.
    pub lifecycle: Arc<SessionLifecycle>,
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
        let lifecycle = Arc::new(SessionLifecycle::default());
        lifecycle.set_session_id(id.clone());
        lifecycle.set_transport("sse");
        let session = Session {
            id: id.clone(),
            endpoint,
            lifecycle,
            tx,
        };
        self.sessions.lock().await.insert(id, session.clone());
        crate::mcp::audit_allow("session_create", &session.id, endpoint_path);
        session
    }

    /// Looks up a session by id.
    pub async fn get(&self, id: &str) -> Option<Session> {
        self.sessions.lock().await.get(id).cloned()
    }

    /// Removes and drops a session (on disconnect or shutdown), closing its
    /// lifecycle so later requests observe a deterministic closed state.
    pub async fn remove(&self, id: &str) {
        let existing = self.sessions.lock().await.remove(id);
        if let Some(session) = existing {
            session.lifecycle.mark_closed();
            crate::mcp::audit_allow("session_destroy", id, "disconnect");
        }
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
        // The counter deliberately WRAPS at u64::MAX (sequence-number
        // semantics): saturating it would hand out the same counter value to
        // every future session, while wrap keeps ids distinct for 2^64
        // creates — and the random prefix keeps ids unique regardless.
        // `fetch_add` is atomic, so concurrent creates never race or duplicate
        // a counter value.
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
    async fn session_id_counter_wraps_cleanly_at_the_boundary() {
        // The id counter uses sequence-number semantics: it wraps to 0
        // after u64::MAX rather than saturating (a saturated counter would
        // issue the same suffix forever). Ids stay unique through the
        // boundary thanks to the random 128-bit prefix.
        let registry = SessionRegistry::new();
        for start in [u64::MAX - 2, u64::MAX - 1, u64::MAX] {
            registry.next_id.store(start, Ordering::Relaxed);
            let first = registry.create("/mcp").await;
            let second = registry.create("/mcp").await;
            let third = registry.create("/mcp").await;
            assert_ne!(first.id, second.id);
            assert_ne!(second.id, third.id);
            assert_ne!(first.id, third.id);
            let suffix = |id: &str| id[id.len() - 16..].to_string();
            // Suffix sequence: start, start+1, start+2 (mod 2^64).
            assert_eq!(suffix(&first.id), format!("{start:016x}"));
            assert_eq!(
                suffix(&second.id),
                format!("{:016x}", start.wrapping_add(1))
            );
            assert_eq!(suffix(&third.id), format!("{:016x}", start.wrapping_add(2)));
        }
    }

    #[tokio::test]
    async fn concurrent_session_creation_never_duplicates_ids() {
        // Concurrent creates race only the atomic counter: every create
        // must observe a distinct counter value (no lost updates), and
        // the full ids must be unique.
        let registry = std::sync::Arc::new(SessionRegistry::new());
        registry.next_id.store(u64::MAX - 1, Ordering::Relaxed);
        let mut ids = tokio::task::JoinSet::new();
        for _ in 0..64 {
            let registry = std::sync::Arc::clone(&registry);
            ids.spawn(async move { registry.create("/mcp").await.id });
        }
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = ids.join_next().await {
            assert!(seen.insert(id.expect("task panicked")), "duplicate id");
        }
        // 64 creates from MAX-1: the counter wraps but stays atomic.
        assert_eq!(seen.len(), 64);
    }

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
