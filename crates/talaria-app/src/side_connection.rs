//! The side connection: a second link for listing, picker renames and exports (#231).

/// What the side connection may reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideScope {
    /// Across profiles, over these served profiles.
    Host { profiles: Vec<String> },
    /// The foreground profile only, with why host scope failed.
    ProfileOnly { profile: String, why: String },
}

/// Why the side connection failed to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideOpenError {
    Discovery(String),
    Ticket(String),
    Connect(String),
    Handshake(String),
}

/// Why a side call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideCallError {
    Refused {
        code: i64,
        reason: Option<String>,
        data: serde_json::Value,
    },
    TooLarge,
    Dropped,
    Timeout,
}

/// The client over one connection. No session tracking, no reconnect loop.
pub struct SideConnection {
    _held: (),
}

impl SideConnection {
    /// Discover, mint a host-scoped interactive ticket, connect, handshake.
    /// Falls back once to a profile-scoped ticket when host scope is refused.
    pub async fn open(
        _home: &std::path::Path,
        _foreground_profile: &str,
    ) -> Result<SideConnection, SideOpenError> {
        unimplemented!("#231")
    }

    /// What this connection may reach.
    pub fn scope(&self) -> &SideScope {
        unimplemented!("#231")
    }

    /// True after the gateway closed it or the connection dropped.
    pub fn is_closed(&self) -> bool {
        unimplemented!("#231")
    }

    /// Call a method with a timeout.
    pub async fn call(
        &self,
        _method: &str,
        _params: serde_json::Value,
        _timeout: std::time::Duration,
    ) -> Result<serde_json::Value, SideCallError> {
        unimplemented!("#231")
    }

    /// Close the connection.
    pub async fn close(self) {
        unimplemented!("#231")
    }
}
