//! Labels in a draft and the payloads that belong to them.

use std::collections::HashSet;
use std::ops::Range;

use crate::session::SessionId;

use super::expand::{Attachment, ExpandError, Expanded};

/// When a paste becomes a label. Zero turns that rule off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollapseRule {
    /// Minimum line count that collapses.
    pub lines: usize,
    /// Minimum character count that collapses.
    pub chars: usize,
}

impl CollapseRule {
    /// Five lines or 2,000 characters.
    pub const HERMES_DEFAULT: Self = Self {
        lines: 5,
        chars: 2000,
    };

    /// Whether `text` meets either threshold.
    #[must_use]
    pub fn should_collapse(&self, text: &str) -> bool {
        let text = normalized(text);
        (self.lines > 0 && line_count(&text) >= self.lines)
            || (self.chars > 0 && text.chars().count() >= self.chars)
    }
}

/// Identity of one chip. Paste numbers are not chip ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChipId(pub u64);

impl ChipId {
    /// The raw counter value.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Where an image label is in its life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageState {
    /// Bytes are being staged.
    Pending { session: SessionId },
    /// The gateway accepted the bytes.
    Ready {
        session: SessionId,
        path: String,
        mime: String,
    },
    /// A receipt already covered this image.
    Sent,
}

/// What a chip holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipKind {
    /// Collapsed paste text.
    Paste { text: String, lines: usize },
    /// An image label.
    Image { n: u32, state: ImageState },
    /// A file label.
    File {
        n: u32,
        name: String,
        ref_text: String,
        session: SessionId,
    },
}

/// One label and its payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    /// Stable id for this payload.
    pub id: ChipId,
    /// The exact label text in the draft.
    pub label: String,
    /// Payload behind that label.
    pub kind: ChipKind,
}

/// A chip could not be added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipError {
    /// The draft already has ten live images.
    TooManyImages,
}

/// Paste texts and the labels that point at them.
pub struct ChipStore {
    budget: usize,
    next_paste: u64,
    chips: Vec<Chip>,
    held: usize,
    next_id: u64,
    missing_pastes: HashSet<String>,
    active_pastes: HashSet<ChipId>,
    evicted_pastes: HashSet<String>,
    image_highest: u32,
    file_highest: u32,
}

impl ChipStore {
    /// A store that keeps at most `paste_budget_bytes` of paste text.
    #[must_use]
    pub fn new(paste_budget_bytes: usize) -> Self {
        Self {
            budget: paste_budget_bytes,
            next_paste: 1,
            chips: Vec::new(),
            held: 0,
            next_id: 1,
            missing_pastes: HashSet::new(),
            active_pastes: HashSet::new(),
            evicted_pastes: HashSet::new(),
            image_highest: 0,
            file_highest: 0,
        }
    }

    fn id(&mut self) -> ChipId {
        let id = ChipId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// Collapse `text` into a label and evict older pastes while over budget.
    pub fn collapse_paste(&mut self, text: &str) -> String {
        let measured = normalized(text);
        let lines = line_count(&measured);
        let n = self.next_paste;
        self.next_paste = self.next_paste.saturating_add(1);
        let label = if lines > 1 {
            format!("[Pasted text #{n} +{lines} lines]")
        } else {
            format!("[Pasted text #{n} {} chars]", measured.chars().count())
        };
        let id = self.id();
        self.chips.push(Chip {
            id,
            label: label.clone(),
            kind: ChipKind::Paste {
                text: text.to_string(),
                lines,
            },
        });
        self.active_pastes.insert(id);
        self.held = self.held.saturating_add(text.len());
        while self.held > self.budget {
            let Some(at) = self
                .chips
                .iter()
                .position(|chip| chip.id != id && matches!(chip.kind, ChipKind::Paste { .. }))
            else {
                break;
            };
            let chip = self.chips.remove(at);
            if let ChipKind::Paste { text, .. } = chip.kind {
                self.held -= text.len();
            }
            if self.active_pastes.remove(&chip.id) {
                self.missing_pastes.insert(chip.label.clone());
            }
            self.evicted_pastes.insert(chip.label);
        }
        label
    }

    /// Start an image label. The number is one past the highest in `draft`.
    pub fn begin_image(
        &mut self,
        draft: &str,
        session: &SessionId,
    ) -> Result<(ChipId, String), ChipError> {
        let count = self
            .chips
            .iter()
            .filter(|chip| {
                matches!(
                    chip.kind,
                    ChipKind::Image {
                        state: ImageState::Pending { .. } | ImageState::Ready { .. },
                        ..
                    }
                )
            })
            .count();
        if count >= 10 {
            return Err(ChipError::TooManyImages);
        }
        self.image_highest = self
            .image_highest
            .max(highest_number(draft, "[Image #", "]"))
            .saturating_add(1);
        let n = self.image_highest;
        let label = format!("[Image #{n}]");
        let id = self.id();
        self.chips.push(Chip {
            id,
            label: label.clone(),
            kind: ChipKind::Image {
                n,
                state: ImageState::Pending {
                    session: session.clone(),
                },
            },
        });
        Ok((id, label))
    }

    /// Mark an image ready with the gateway path and media type.
    pub fn image_ready(&mut self, id: ChipId, path: String, mime: String) {
        if let Some(Chip {
            kind: ChipKind::Image { state, .. },
            ..
        }) = self.chips.iter_mut().find(|chip| chip.id == id)
            && let ImageState::Pending { session } = state
        {
            *state = ImageState::Ready {
                session: session.clone(),
                path,
                mime,
            };
        }
    }

    /// Drop a failed image and return the label to remove.
    pub fn image_failed(&mut self, id: ChipId) -> Option<String> {
        let at = self.chips.iter().position(|chip| {
            chip.id == id
                && matches!(
                    chip.kind,
                    ChipKind::Image {
                        state: ImageState::Pending { .. },
                        ..
                    }
                )
        })?;
        Some(self.chips.remove(at).label)
    }

    /// Add a file label. `name` is sanitized before it enters the label.
    pub fn add_file(
        &mut self,
        draft: &str,
        name: &str,
        ref_text: String,
        session: &SessionId,
    ) -> String {
        self.file_highest = self
            .file_highest
            .max(highest_number(draft, "[File #", ":"))
            .saturating_add(1);
        let n = self.file_highest;
        let name = super::sanitize(name);
        let label = format!("[File #{n}: {name}]");
        let id = self.id();
        self.chips.push(Chip {
            id,
            label: label.clone(),
            kind: ChipKind::File {
                n,
                name,
                ref_text,
                session: session.clone(),
            },
        });
        label
    }

    /// Deactivate missing labels. Paste text stays archived for explicit history recall.
    pub fn reconcile(&mut self, draft: &str) {
        self.active_pastes.retain(|id| {
            self.chips
                .iter()
                .any(|chip| chip.id == *id && draft.contains(&chip.label))
        });
        self.chips.retain(|chip| {
            matches!(chip.kind, ChipKind::Paste { .. }) || draft.contains(&chip.label)
        });
        self.missing_pastes
            .retain(|label| draft.contains(label.as_str()));
        self.image_highest = highest_number(draft, "[Image #", "]");
        self.file_highest = highest_number(draft, "[File #", ":");
    }

    /// Live chips and the first intact range of each label.
    #[must_use]
    pub fn live<'a>(&'a self, draft: &str) -> Vec<(&'a Chip, Range<usize>)> {
        let mut ranges: Vec<_> = self
            .chips
            .iter()
            .filter(|chip| {
                (!matches!(chip.kind, ChipKind::Paste { .. })
                    || self.active_pastes.contains(&chip.id))
                    && !matches!(
                        chip.kind,
                        ChipKind::Image {
                            state: ImageState::Sent,
                            ..
                        }
                    )
            })
            .filter_map(|chip| {
                draft
                    .find(&chip.label)
                    .map(|at| (chip, at..at + chip.label.len()))
            })
            .collect();
        ranges.sort_by_key(|(_, range)| range.start);
        // A label-like filename can contain another label: only the outer live label owns the text.
        let mut end = 0;
        ranges.retain(|(_, range)| {
            if range.start < end {
                false
            } else {
                end = range.end;
                true
            }
        });
        ranges
    }

    /// The live label that ends at `byte`, if the caret is just after it.
    #[must_use]
    pub fn label_ending_at(&self, draft: &str, byte: usize) -> Option<Range<usize>> {
        self.live(draft)
            .into_iter()
            .find_map(|(_, range)| (range.end == byte).then_some(range))
    }

    /// The live label that starts at `byte`, if the caret is just before it.
    #[must_use]
    pub fn label_starting_at(&self, draft: &str, byte: usize) -> Option<Range<usize>> {
        self.live(draft)
            .into_iter()
            .find_map(|(_, range)| (range.start == byte).then_some(range))
    }

    /// Expand live labels. Image labels stay in the text.
    pub fn expand(&self, draft: &str) -> Result<Expanded, ExpandError> {
        if let Some((_, label)) = self
            .missing_pastes
            .iter()
            .filter_map(|label| draft.find(label).map(|at| (at, label)))
            .min_by_key(|(at, _)| *at)
        {
            return Err(ExpandError::MissingPaste {
                label: label.clone(),
            });
        }
        let live = self.live(draft);
        let image_count = live
            .iter()
            .filter(|(chip, _)| matches!(chip.kind, ChipKind::Image { .. }))
            .count();
        if image_count > 10 {
            return Err(ExpandError::TooManyImages);
        }
        let mut text = String::with_capacity(draft.len());
        let mut attachments = Vec::new();
        let mut end = 0;
        for (chip, range) in live {
            text.push_str(&draft[end..range.start]);
            match &chip.kind {
                ChipKind::Paste { text: paste, .. } => text.push_str(paste),
                ChipKind::File { ref_text, .. } => text.push_str(ref_text),
                ChipKind::Image {
                    state: ImageState::Pending { .. },
                    ..
                } => {
                    return Err(ExpandError::Pending {
                        label: chip.label.clone(),
                    });
                }
                ChipKind::Image {
                    state: ImageState::Ready { path, mime, .. },
                    ..
                } => {
                    text.push_str(&chip.label);
                    attachments.push(Attachment {
                        path: path.clone(),
                        mime: mime.clone(),
                    });
                }
                ChipKind::Image {
                    state: ImageState::Sent,
                    ..
                } => text.push_str(&chip.label),
            }
            end = range.end;
        }
        text.push_str(&draft[end..]);
        let reference_count = super::mention::count_references(&text);
        Ok(Expanded {
            text,
            display: draft.to_string(),
            attachments,
            reference_count,
        })
    }

    /// Mark the images in `expanded` sent. Call this only after a receipt.
    pub fn mark_sent(&mut self, expanded: &Expanded) {
        let sent: HashSet<ChipId> = self
            .live(&expanded.display)
            .into_iter()
            .filter(|(chip, _)| match &chip.kind {
                ChipKind::Image {
                    state: ImageState::Ready { path, mime, .. },
                    ..
                } => expanded
                    .attachments
                    .iter()
                    .any(|attachment| attachment.path == *path && attachment.mime == *mime),
                ChipKind::Image { .. } => false,
                _ => true,
            })
            .map(|(chip, _)| chip.id)
            .collect();
        for chip in &mut self.chips {
            if sent.contains(&chip.id)
                && let ChipKind::Image { state, .. } = &mut chip.kind
            {
                *state = ImageState::Sent;
            }
        }
        self.active_pastes.retain(|id| !sent.contains(id));
        self.chips.retain(|chip| {
            !sent.contains(&chip.id)
                || matches!(chip.kind, ChipKind::Image { .. } | ChipKind::Paste { .. })
        });
        self.image_highest = 0;
        self.file_highest = 0;
    }

    /// Drop image and file chips for `session`. True when any were dropped.
    pub fn drop_session_payloads(&mut self, session: &SessionId) -> bool {
        let before = self.chips.len();
        self.chips.retain(|chip| match &chip.kind {
            ChipKind::Image { state: ImageState::Pending { session: owner } | ImageState::Ready { session: owner, .. }, .. } | ChipKind::File { session: owner, .. } => owner != session,
            _ => true,
        });
        before != self.chips.len()
    }

    /// Release payloads owned by any session except the new foreground session.
    pub fn drop_foreign_payloads(&mut self, session: &SessionId) -> bool {
        let before = self.chips.len();
        self.chips.retain(|chip| match &chip.kind {
            ChipKind::Image { state: ImageState::Pending { session: owner } | ImageState::Ready { session: owner, .. }, .. }
                | ChipKind::File { session: owner, .. } => owner == session,
            _ => true,
        });
        before != self.chips.len()
    }

    /// Release image payloads after the gateway rejects stale staging paths.
    pub fn drop_images(&mut self) {
        self.chips
            .retain(|chip| !matches!(chip.kind, ChipKind::Image { .. }));
    }

    /// Transfer media into a submission's ownership and deactivate its paste labels.
    /// Paste text stays in the archive and is never copied into the returned payloads.
    pub fn take_draft_payloads(&mut self, draft: &str) -> Vec<Chip> {
        let ids: HashSet<ChipId> = self
            .live(draft)
            .into_iter()
            .map(|(chip, _)| chip.id)
            .collect();
        self.active_pastes.retain(|id| !ids.contains(id));
        let mut taken = Vec::new();
        self.chips = std::mem::take(&mut self.chips)
            .into_iter()
            .filter_map(|chip| {
                if ids.contains(&chip.id) && !matches!(chip.kind, ChipKind::Paste { .. }) {
                    taken.push(chip);
                    None
                } else {
                    Some(chip)
                }
            })
            .collect();
        self.image_highest = 0;
        self.file_highest = 0;
        taken
    }

    /// Restore media owned by a submission when its display is restored to the composer.
    pub fn restore_payloads(&mut self, chips: Vec<Chip>) {
        for chip in chips {
            if matches!(
                chip.kind,
                ChipKind::Paste { .. }
                    | ChipKind::Image {
                        state: ImageState::Sent,
                        ..
                    }
            ) {
                continue;
            }
            self.chips
                .retain(|existing| existing.id != chip.id && existing.label != chip.label);
            self.chips.push(chip);
        }
    }

    /// Invalidate only ready images whose cache path and type match the failed submission.
    pub fn invalidate_images(&mut self, attachments: &[Attachment]) {
        self.chips.retain(|chip| match &chip.kind {
            ChipKind::Image {
                state: ImageState::Ready { path, mime, .. },
                ..
            } => !attachments
                .iter()
                .any(|attachment| attachment.path == *path && attachment.mime == *mime),
            _ => true,
        });
    }

    /// Deactivate a paste or release another payload without removing draft text.
    pub fn remove(&mut self, id: ChipId) {
        self.active_pastes.remove(&id);
        self.chips
            .retain(|chip| chip.id != id || matches!(chip.kind, ChipKind::Paste { .. }));
    }

    /// Activate archived paste labels only when the application explicitly recalls history.
    pub fn restore_pastes(&mut self, draft: &str) {
        self.reconcile(draft);
        self.active_pastes.extend(
            self.chips
                .iter()
                .filter(|chip| {
                    matches!(chip.kind, ChipKind::Paste { .. }) && draft.contains(&chip.label)
                })
                .map(|chip| chip.id),
        );
        self.missing_pastes.extend(
            self.evicted_pastes
                .iter()
                .filter(|label| draft.contains(label.as_str()))
                .cloned(),
        );
    }
}

fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string()
}

fn line_count(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn highest_number(draft: &str, prefix: &str, suffix: &str) -> u32 {
    draft
        .match_indices(prefix)
        .filter_map(|(at, _)| {
            let after = &draft[at + prefix.len()..];
            let digits = after.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || !after[digits..].starts_with(suffix) {
                return None;
            }
            after[..digits].parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_media_cannot_be_revived_by_retyping_labels() {
        let mut store = ChipStore::new(1024);
        let session = SessionId::new("local-session-1");
        let (id, image) = store.begin_image("", &session).unwrap();
        store.image_ready(id, "cache/first.png".into(), "image/png".into());
        let file = store.add_file(&image, "notes.txt", "@file:notes.txt".into(), &session);
        let paste = store.collapse_paste("full paste");
        let draft = format!("{image} {file} {paste}");
        let taken = store.take_draft_payloads(&draft);
        assert_eq!(taken.len(), 2);
        assert!(
            taken
                .iter()
                .all(|chip| !matches!(chip.kind, ChipKind::Paste { .. }))
        );
        store.reconcile("");
        store.reconcile(&draft);
        assert!(store.live(&draft).is_empty());
        let plain = store.expand(&draft).unwrap();
        assert_eq!(plain.text, draft);
        assert!(plain.attachments.is_empty());
        assert_eq!(store.held, "full paste".len());
    }

    #[test]
    fn restoring_owned_payloads_recovers_the_exact_submission() {
        let mut store = ChipStore::new(1024);
        let session = SessionId::new("local-session-1");
        let (id, image) = store.begin_image("", &session).unwrap();
        store.image_ready(id, "cache/original.png".into(), "image/png".into());
        let file = store.add_file(
            &image,
            "notes.txt",
            "@file:stored-notes.txt".into(),
            &session,
        );
        let paste = store.collapse_paste("full\npaste");
        let draft = format!("{file} {paste} {image}");
        let before = store.expand(&draft).unwrap();
        let taken = store.take_draft_payloads(&draft);
        store.reconcile("");
        store.restore_payloads(taken);
        store.restore_pastes(&draft);
        assert_eq!(store.expand(&draft).unwrap(), before);
        assert_eq!(store.live(&draft).len(), 3);
        assert_eq!(store.held, "full\npaste".len());
    }

    #[test]
    fn invalidation_of_old_submission_keeps_new_draft_images() {
        let mut store = ChipStore::new(1024);
        let session = SessionId::new("local-session-1");
        let (old_id, old_label) = store.begin_image("", &session).unwrap();
        store.image_ready(old_id, "cache/old.png".into(), "image/png".into());
        let old = store.expand(&old_label).unwrap();
        let _taken = store.take_draft_payloads(&old_label);
        store.reconcile("");
        let (new_id, new_label) = store.begin_image("", &session).unwrap();
        store.image_ready(new_id, "cache/new.png".into(), "image/png".into());
        assert_eq!(old_label, new_label);
        store.invalidate_images(&old.attachments);
        assert_eq!(
            store.expand(&new_label).unwrap().attachments[0].path,
            "cache/new.png"
        );
        let current = store.expand(&new_label).unwrap();
        store.invalidate_images(&current.attachments);
        assert!(store.expand(&new_label).unwrap().attachments.is_empty());
    }

    #[test]
    fn deleted_paste_requires_explicit_history_restore() {
        let mut store = ChipStore::new(1024);
        let label = store.collapse_paste("original\ntext");
        store.reconcile(&label);
        store.reconcile(&label.replace("lines", "line"));
        store.reconcile(&label);
        assert!(store.live(&label).is_empty());
        assert_eq!(store.expand(&label).unwrap().text, label);
        store.restore_pastes(&label);
        assert_eq!(store.expand(&label).unwrap().text, "original\ntext");
    }

    #[test]
    fn receipt_deactivates_paste_without_losing_history_text() {
        let mut store = ChipStore::new(1024);
        let label = store.collapse_paste("the complete paste");
        let submitted = store.expand(&label).unwrap();
        store.mark_sent(&submitted);
        store.reconcile("");
        store.reconcile(&label);
        assert!(store.live(&label).is_empty());
        store.restore_pastes(&label);
        assert_eq!(store.expand(&label).unwrap().text, "the complete paste");
        assert_eq!(
            store
                .chips
                .iter()
                .filter(|chip| matches!(chip.kind, ChipKind::Paste { .. }))
                .count(),
            1
        );
        assert_eq!(store.held, "the complete paste".len());
    }

    #[test]
    fn archive_eviction_is_fifo_across_receipts_and_recall() {
        let mut store = ChipStore::new(8);
        let first = store.collapse_paste("1111");
        store.mark_sent(&store.expand(&first).unwrap());
        let second = store.collapse_paste("2222");
        store.mark_sent(&store.expand(&second).unwrap());
        store.restore_pastes(&first);
        assert_eq!(store.expand(&first).unwrap().text, "1111");
        store.reconcile("");
        let third = store.collapse_paste("3333");
        assert_eq!(store.held, 8);
        store.restore_pastes(&first);
        assert_eq!(
            store.expand(&first),
            Err(ExpandError::MissingPaste { label: first })
        );
        store.restore_pastes(&format!("{second} {third}"));
        assert_eq!(store.expand(&second).unwrap().text, "2222");
        assert_eq!(store.expand(&third).unwrap().text, "3333");
    }

    #[test]
    fn active_eviction_blocks_until_label_is_removed() {
        let mut store = ChipStore::new(4);
        let first = store.collapse_paste("1111");
        let second = store.collapse_paste("2222");
        assert_eq!(
            store.expand(&first),
            Err(ExpandError::MissingPaste {
                label: first.clone()
            })
        );
        store.reconcile(&second);
        store.reconcile(&first);
        assert_eq!(store.expand(&first).unwrap().text, first);
        store.restore_pastes(&first);
        assert_eq!(
            store.expand(&first),
            Err(ExpandError::MissingPaste { label: first })
        );
    }
}
