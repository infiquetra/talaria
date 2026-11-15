//! Session notices retained separately from transcript cells and status notes.

/// One Hermes notice, with the payload values kept as received.
#[derive(Clone, Debug, PartialEq)]
pub struct NoticeRow {
    /// Short notice text.
    pub text: String,
    /// Severity reported by Hermes.
    pub level: String,
    /// Notice kind, including `sticky`, `ttl`, or `agent`.
    pub kind: String,
    /// Replacement and clear key. `None` uses the single keyless slot.
    pub key: Option<String>,
    /// Optional upstream identifier, independent of the replacement key.
    pub id: Option<String>,
    /// Optional full text behind the short notice.
    pub detail: Option<String>,
    /// Lifetime for `ttl` notices. Missing or negative values never expire.
    pub ttl_ms: Option<i64>,
}

/// Notices in show order, with one row per key and one keyless slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoticeStore {
    rows: Vec<StoredNotice>,
}

#[derive(Clone, Debug, PartialEq)]
struct StoredNotice {
    row: NoticeRow,
    started_at_ms: Option<u64>,
}

impl NoticeStore {
    /// Replace the matching key or keyless slot, making this show the newest.
    /// Even empty text is stored; payload text filtering belongs to the reducer.
    pub fn apply_show(&mut self, row: NoticeRow) {
        self.rows
            .retain(|stored| stored.row.key.as_deref() != row.key.as_deref());
        self.rows.push(StoredNotice {
            row,
            started_at_ms: None,
        });
    }

    /// Remove only the named key. The keyless slot is never cleared here.
    pub fn apply_clear(&mut self, key: &str) {
        self.rows
            .retain(|stored| stored.row.key.as_deref() != Some(key));
    }

    /// Arm timed rows on their first call, then expire them on later calls.
    /// `now_ms` uses one caller-owned origin for the lifetime of this store.
    pub fn expire(&mut self, now_ms: u64) {
        self.rows.retain_mut(|stored| {
            if stored.row.kind != "ttl" {
                return true;
            }
            let Some(ttl_ms) = stored.row.ttl_ms.and_then(|ttl| u64::try_from(ttl).ok()) else {
                return true;
            };
            let Some(start) = stored.started_at_ms else {
                stored.started_at_ms = Some(now_ms);
                return true;
            };
            now_ms
                .checked_sub(start)
                .is_none_or(|elapsed| elapsed < ttl_ms)
        });
    }

    /// The current row for this key, including an empty string key.
    pub fn by_key(&self, key: &str) -> Option<&NoticeRow> {
        self.rows
            .iter()
            .find(|stored| stored.row.key.as_deref() == Some(key))
            .map(|stored| &stored.row)
    }

    /// The single row shown without a key, if any.
    pub fn keyless(&self) -> Option<&NoticeRow> {
        self.rows
            .iter()
            .find(|stored| stored.row.key.is_none())
            .map(|stored| &stored.row)
    }

    /// The latest show that has not been replaced, cleared, or expired.
    pub fn most_recent(&self) -> Option<&NoticeRow> {
        self.rows.last().map(|stored| &stored.row)
    }

    /// Count current warning and error notices.
    pub fn warning_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|stored| matches!(stored.row.level.as_str(), "warn" | "error"))
            .count()
    }

    /// Earliest time a timed notice needs to be armed or expired.
    pub fn next_deadline(&self) -> Option<u64> {
        self.rows
            .iter()
            .filter(|stored| stored.row.kind == "ttl")
            .filter_map(|stored| {
                let ttl = stored.row.ttl_ms.and_then(|ttl| u64::try_from(ttl).ok())?;
                Some(
                    stored
                        .started_at_ms
                        .map_or(0, |start| start.saturating_add(ttl)),
                )
            })
            .min()
    }
}
