//! The `/theme` and `/skin` selectors: what opening, moving, Enter and Esc do.
//!
//! The list itself is `talaria_core::commands::ChoiceList`. A theme preview changes only what the
//! screen resolves, in memory. Hermes has no call that shows a skin without making it the
//! profile's skin, so a skin preview writes it once the selection has rested, and Esc or quitting
//! writes the original back.

use std::time::Duration;

use talaria_core::appearance::{AppearanceChange, AppearanceState, NotAppliedReason, Polarity};
use talaria_core::commands::{ChoiceEffect, ChoiceItem, ChoiceKey, ChoiceList};

use super::{AppEffect, AppMessage, AppState, Executor, SessionService, notice_lines};
use crate::appearance::{self, SkinChoices, SkinPreviewError, ThemeFileSession, ThemePreview};
use crate::theme_file::{self, ThemeFile};

/// How long the skin selection rests before Talaria writes the preview.
const SKIN_REST: Duration = Duration::from_millis(300);
/// How long quitting waits for the original skin to be written back.
const RESTORE_WAIT: Duration = Duration::from_secs(2);
/// The skin selector's footer note.
const SKIN_NOTE: &str = "Previewing changes the Hermes profile's skin until you cancel.";
/// Id prefixes of the `/theme` rows.
const FILE: &str = "file:";
const BACKGROUND: &str = "background:";

/// An open selector.
pub struct Selector {
    /// The rows, the filter and the selection.
    pub list: ChoiceList,
    kind: Kind,
}

enum Kind {
    Theme {
        /// Every theme the list offers, loaded, by identifier.
        files: Vec<(String, ThemeFile)>,
        /// The `config.toml` table that keeps a file choice.
        table: String,
    },
    /// The profile's skin when the list opened.
    Skin(String),
}

/// A skin preview that has written, or may write, the profile's skin.
pub struct SkinPreview {
    original: String,
    /// The skin Talaria last wrote.
    applied: String,
    /// The selection waiting to be written.
    wanted: String,
    /// Bumped by each move; only the newest rest writes.
    timer: u64,
    in_flight: bool,
    /// A refusal stopped previewing.
    stopped: bool,
    /// What to do once the write in flight answers.
    finish: Option<Finish>,
    /// The write in flight puts the original back.
    restoring: bool,
}

enum Finish {
    Restore,
    Apply(String),
}

/// Work a selector asks the executor for.
pub enum ChoiceWork {
    /// List the theme files off the loop.
    ListThemes,
    /// Print the `/skin` report, then ask Hermes for the skins and their descriptions.
    ListSkins,
    /// Wake after the skin selection has rested.
    Rest(u64),
    /// Write this skin and describe it.
    SetSkin(String),
}

/// A selector's work, finished.
pub enum ChoiceReply {
    Themes(Box<ThemeListing>),
    Skins {
        /// The `/skin` report: the skin state it produced when it changed one, and its lines.
        report: (Option<AppearanceState>, Vec<String>),
        result: Result<SkinChoices, String>,
        /// Why the current theme shows no skin, if it does not.
        ignored: Option<NotAppliedReason>,
    },
    Rested(u64),
    SkinSet {
        name: String,
        skin: Option<AppearanceState>,
        result: Result<(), SkinPreviewError>,
    },
}

/// What the `/theme` selector lists, read off the loop.
pub struct ThemeListing {
    users: Vec<(String, Result<ThemeFile, String>)>,
    session: Option<String>,
    configured: Option<String>,
    table: String,
}

/// The selector the view draws.
pub(super) fn list(state: &AppState) -> Option<&ChoiceList> {
    state.choice.as_ref().map(|selector| &selector.list)
}

/// Run a selector's work. Replies come back as [`AppMessage::Choice`].
pub(super) fn run(
    state: &AppState,
    executor: &Executor,
    work: ChoiceWork,
    ignored: Option<NotAppliedReason>,
) {
    match work {
        ChoiceWork::ListThemes => {
            let files = state.appearance.theme_files().cloned().or_else(|| {
                let home = state.home.as_ref()?;
                Some(ThemeFileSession::new(
                    crate::config::user_config_path(home),
                    crate::config::Config::default().theme,
                    &state.profile,
                ))
            });
            executor.send_later(async move {
                let listing = tokio::task::spawn_blocking(move || ThemeListing {
                    users: files
                        .as_ref()
                        .map(|files| theme_file::list_themes(files.config_path()))
                        .unwrap_or_default(),
                    session: files
                        .as_ref()
                        .and_then(|files| files.session_choice().map(str::to_owned)),
                    configured: files
                        .as_ref()
                        .and_then(|files| files.configured_id().map(str::to_owned)),
                    table: files
                        .as_ref()
                        .map_or_else(|| "[theme]".to_owned(), ThemeFileSession::file_table),
                })
                .await
                .unwrap_or(ThemeListing {
                    users: Vec::new(),
                    session: None,
                    configured: None,
                    table: "[theme]".to_owned(),
                });
                AppMessage::Choice(ChoiceReply::Themes(Box::new(listing)))
            });
        }
        ChoiceWork::ListSkins => {
            let client = state
                .service
                .as_ref()
                .and_then(SessionService::client)
                .cloned();
            let session = session_id(state);
            let profile = state.profile.clone();
            let mut service = state.appearance.clone();
            executor.send_later(async move {
                let Some(client) = client else {
                    return AppMessage::AppearanceDone {
                        skin: None,
                        lines: appearance::offline_skin_lines(None),
                        generation: None,
                    };
                };
                let before = service.skin_state().clone();
                let lines = service
                    .skin_command(&client, None, session.as_deref(), &profile)
                    .await;
                let skin = (service.skin_state() != &before).then(|| service.skin_state().clone());
                let result = appearance::skin_choices(&client, session.as_deref()).await;
                AppMessage::Choice(ChoiceReply::Skins {
                    report: (skin, lines),
                    result,
                    ignored,
                })
            });
        }
        ChoiceWork::Rest(timer) => executor.send_later(async move {
            tokio::time::sleep(SKIN_REST).await;
            AppMessage::Choice(ChoiceReply::Rested(timer))
        }),
        ChoiceWork::SetSkin(name) => {
            let client = state
                .service
                .as_ref()
                .and_then(SessionService::client)
                .cloned();
            let session = session_id(state);
            let mut service = state.appearance.clone();
            executor.send_later(async move {
                let Some(client) = client else {
                    return AppMessage::Choice(ChoiceReply::SkinSet {
                        name,
                        skin: None,
                        result: Err(SkinPreviewError::Refused(
                            "Talaria could not reach Hermes".to_owned(),
                        )),
                    });
                };
                let before = service.skin_state().clone();
                let result = service
                    .preview_skin(&client, &name, session.as_deref())
                    .await;
                let skin = (service.skin_state() != &before).then(|| service.skin_state().clone());
                AppMessage::Choice(ChoiceReply::SkinSet { name, skin, result })
            });
        }
    }
}

fn session_id(state: &AppState) -> Option<String> {
    state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
}

/// Fold a finished piece of work into the selector.
pub(super) fn reply(state: &mut AppState, reply: ChoiceReply) -> Vec<AppEffect> {
    match reply {
        // Typing that began before the list arrived goes on to the composer.
        ChoiceReply::Themes(_) if !state.draft.is_empty() => Vec::new(),
        ChoiceReply::Themes(listing) => {
            let mut effects = close(state);
            effects.extend(open_themes(state, *listing));
            effects
        }
        ChoiceReply::Skins {
            report: (skin, lines),
            result,
            ignored,
        } => {
            let mut effects = super::update(
                state,
                AppMessage::AppearanceDone {
                    skin,
                    lines,
                    generation: None,
                },
            );
            if let Ok(choices) = result
                && !choices.items.is_empty()
                && state.draft.is_empty()
            {
                effects.extend(close(state));
                effects.push(open_skins(state, choices, ignored));
            }
            effects
        }
        ChoiceReply::Rested(timer) => rested(state, timer),
        ChoiceReply::SkinSet { name, skin, result } => skin_set(state, name, skin, result),
    }
}

fn open_themes(state: &mut AppState, listing: ThemeListing) -> Vec<AppEffect> {
    let ThemeListing {
        users,
        session,
        configured,
        ..
    } = &listing;
    let row =
        |id: String, name: &str, description: String, group: &str, current: bool| ChoiceItem {
            id,
            name: name.to_owned(),
            description,
            group: Some(group.to_owned()),
            current,
        };
    let follows = match configured.as_deref() {
        None => "Follow config.toml (no theme file set)".to_owned(),
        Some(id) if theme_file::builtin(id).is_some() => format!("Follow config.toml (now {id})"),
        Some(id) => format!("Follow config.toml (now themes/{id}.toml)"),
    };
    let mut items = vec![row(
        FILE.to_owned(),
        "Configured",
        follows,
        "Theme",
        session.is_none(),
    )];
    let mut files = Vec::new();
    for id in theme_file::BUILTIN_THEMES {
        let description = theme_file::builtin_description(id).unwrap_or_default();
        items.push(row(
            format!("{FILE}{id}"),
            id,
            description.to_owned(),
            "Theme",
            session.as_deref() == Some(id),
        ));
        if let Some(file) = theme_file::builtin(id) {
            files.push((id.to_owned(), file));
        }
    }
    for (id, loaded) in users {
        let description = match loaded {
            Ok(file) => match &file.name {
                Some(name) => format!("Theme file themes/{id}.toml: {name}"),
                None => format!("Theme file themes/{id}.toml"),
            },
            Err(error) => format!("Cannot be used: {error}"),
        };
        items.push(row(
            format!("{FILE}{id}"),
            id,
            description,
            "Theme",
            session.as_deref() == Some(id.as_str()),
        ));
        if let Ok(file) = loaded {
            files.push((id.clone(), file.clone()));
        }
    }
    let override_ = state.appearance.polarity_override();
    for (id, description, polarity) in [
        (
            "light",
            "Light terminal background, for this session",
            Some(Polarity::Light),
        ),
        (
            "dark",
            "Dark terminal background, for this session",
            Some(Polarity::Dark),
        ),
        ("auto", "Detect the background, as at start-up", None),
    ] {
        items.push(row(
            format!("{BACKGROUND}{id}"),
            id,
            description.to_owned(),
            "Background",
            override_ == polarity,
        ));
    }
    state.choice = Some(Selector {
        list: ChoiceList::new(items),
        kind: Kind::Theme {
            files,
            table: listing.table.clone(),
        },
    });
    vec![AppEffect::Redraw]
}

fn open_skins(
    state: &mut AppState,
    choices: SkinChoices,
    ignored: Option<NotAppliedReason>,
) -> AppEffect {
    let items = choices
        .items
        .into_iter()
        .map(|(name, description)| ChoiceItem {
            current: name == choices.configured,
            id: name.clone(),
            name,
            description,
            group: Some("Skins Hermes has".to_owned()),
        })
        .collect();
    let mut list = ChoiceList::new(items);
    let note = match ignored {
        None => SKIN_NOTE.to_owned(),
        Some(reason) => {
            let why = match reason {
                NotAppliedReason::ColoursOff => "colours off",
                NotAppliedReason::FollowOff => "follow_hermes_skin = false",
                NotAppliedReason::SixteenColours => "16-colour terminal",
            };
            format!(
                "Skins do not show under the current theme ({why}); previewing still changes \
                 the Hermes profile's skin until you cancel."
            )
        }
    };
    list.set_note(Some(note));
    state.choice = Some(Selector {
        list,
        kind: Kind::Skin(choices.configured),
    });
    AppEffect::Redraw
}

/// One key for the open selector.
pub(super) fn key(state: &mut AppState, key: ChoiceKey) -> Vec<AppEffect> {
    let Some(selector) = &mut state.choice else {
        return Vec::new();
    };
    match selector.list.update(key) {
        ChoiceEffect::Redraw => vec![AppEffect::Redraw],
        ChoiceEffect::Preview(id) => preview(state, &id),
        ChoiceEffect::Apply(id) => apply(state, &id),
        ChoiceEffect::Cancel => close(state),
    }
}

fn preview(state: &mut AppState, id: &str) -> Vec<AppEffect> {
    let Some(selector) = &state.choice else {
        return Vec::new();
    };
    match &selector.kind {
        Kind::Theme { files, .. } => {
            let preview = if let Some(id) = id.strip_prefix(BACKGROUND) {
                ThemePreview::Background(match id {
                    "light" => Some(Polarity::Light),
                    "dark" => Some(Polarity::Dark),
                    _ => None,
                })
            } else {
                let id = id.strip_prefix(FILE).unwrap_or(id);
                let wanted = if id.is_empty() {
                    state
                        .appearance
                        .theme_files()
                        .and_then(ThemeFileSession::configured_id)
                        .map(str::to_owned)
                } else {
                    Some(id.to_owned())
                };
                let found = wanted.and_then(|wanted| files.iter().find(|(id, _)| *id == wanted));
                match found {
                    Some(found) => ThemePreview::File(Some(found.clone())),
                    // A file that cannot be loaded previews nothing.
                    None if !id.is_empty() => {
                        state.appearance.set_theme_preview(None);
                        return vec![AppEffect::Appearance, AppEffect::Redraw];
                    }
                    None => ThemePreview::File(None),
                }
            };
            state.appearance.set_theme_preview(Some(preview));
            vec![AppEffect::Appearance, AppEffect::Redraw]
        }
        Kind::Skin(original) => {
            let preview = state.skin_preview.get_or_insert_with(|| SkinPreview {
                original: original.clone(),
                applied: original.clone(),
                wanted: original.clone(),
                timer: 0,
                in_flight: false,
                stopped: false,
                finish: None,
                restoring: false,
            });
            if preview.stopped || preview.restoring || preview.finish.is_some() {
                return vec![AppEffect::Redraw];
            }
            preview.wanted = id.to_owned();
            preview.timer += 1;
            vec![
                AppEffect::Choice(ChoiceWork::Rest(preview.timer)),
                AppEffect::Redraw,
            ]
        }
    }
}

fn apply(state: &mut AppState, id: &str) -> Vec<AppEffect> {
    let Some(selector) = state.choice.take() else {
        return Vec::new();
    };
    match selector.kind {
        Kind::Theme { files, table } => {
            state.appearance.set_theme_preview(None);
            let mut effects = vec![AppEffect::Appearance, AppEffect::Redraw];
            if let Some(background) = id.strip_prefix(BACKGROUND) {
                effects.extend(super::theme_command(state, background));
                if background != "auto" {
                    effects.push(notice_lines(&format!(
                        "To keep the {background} background after this session, set mode = \
                         \"{background}\" under [theme] in config.toml."
                    )));
                }
            } else {
                let file = id.strip_prefix(FILE).unwrap_or(id);
                if file.is_empty() {
                    effects.push(AppEffect::ThemeFileCommand("use".to_owned()));
                } else {
                    effects.push(AppEffect::ThemeFileCommand(format!("use {file}")));
                    // Only a file that loaded is worth keeping; `/theme use` reports why the
                    // others cannot be used.
                    if files.iter().any(|(id, _)| id == file) {
                        effects.push(notice_lines(&format!(
                            "To keep {file} after this session, add file = \"{file}\" under \
                             {table} in config.toml."
                        )));
                    }
                }
            }
            effects
        }
        Kind::Skin(_) => {
            let skin = AppEffect::Skin(Some(id.to_owned()));
            let Some(preview) = &mut state.skin_preview else {
                return vec![skin, AppEffect::Redraw];
            };
            preview.timer += 1;
            if preview.in_flight {
                preview.finish = Some(Finish::Apply(id.to_owned()));
                return vec![AppEffect::Redraw];
            }
            state.skin_preview = None;
            vec![skin, AppEffect::Redraw]
        }
    }
}

/// Close the open selector, putting back what was in force when it opened.
pub(super) fn close(state: &mut AppState) -> Vec<AppEffect> {
    let Some(selector) = state.choice.take() else {
        return Vec::new();
    };
    match selector.kind {
        Kind::Theme { .. } => {
            state.appearance.set_theme_preview(None);
            vec![AppEffect::Appearance, AppEffect::Redraw]
        }
        Kind::Skin(_) => {
            let Some(preview) = &mut state.skin_preview else {
                return vec![AppEffect::Redraw];
            };
            preview.timer += 1;
            if preview.in_flight {
                preview.finish = Some(Finish::Restore);
                return vec![AppEffect::Redraw];
            }
            let mut effects = restore(state);
            effects.push(AppEffect::Redraw);
            effects
        }
    }
}

/// Write the original skin back, when a preview changed it.
fn restore(state: &mut AppState) -> Vec<AppEffect> {
    let Some(preview) = &mut state.skin_preview else {
        return Vec::new();
    };
    if preview.applied == preview.original {
        state.skin_preview = None;
        return Vec::new();
    }
    preview.restoring = true;
    preview.in_flight = true;
    vec![AppEffect::Choice(ChoiceWork::SetSkin(
        preview.original.clone(),
    ))]
}

fn rested(state: &mut AppState, timer: u64) -> Vec<AppEffect> {
    let open = matches!(
        state.choice.as_ref().map(|selector| &selector.kind),
        Some(Kind::Skin(_))
    );
    let Some(preview) = &mut state.skin_preview else {
        return Vec::new();
    };
    if !open
        || timer != preview.timer
        || preview.in_flight
        || preview.stopped
        || preview.wanted == preview.applied
    {
        return Vec::new();
    }
    preview.in_flight = true;
    vec![AppEffect::Choice(ChoiceWork::SetSkin(
        preview.wanted.clone(),
    ))]
}

fn skin_set(
    state: &mut AppState,
    name: String,
    skin: Option<AppearanceState>,
    result: Result<(), SkinPreviewError>,
) -> Vec<AppEffect> {
    let Some(preview) = &mut state.skin_preview else {
        return Vec::new();
    };
    preview.in_flight = false;
    let mut effects = Vec::new();
    let failure = match result {
        Ok(()) => {
            preview.applied.clone_from(&name);
            None
        }
        Err(SkinPreviewError::Refused(reason)) => Some(reason),
        Err(SkinPreviewError::Unknown(reason)) => {
            preview.applied.clone_from(&name);
            Some(reason)
        }
    };
    if let Some(skin) = skin
        && state.appearance.adopt(skin) != AppearanceChange::Unchanged
    {
        effects.push(AppEffect::Appearance);
    }
    let Some(preview) = &mut state.skin_preview else {
        return effects;
    };
    if preview.restoring {
        let original = preview.original.clone();
        state.skin_preview = None;
        effects.push(notice_lines(&match failure {
            None => format!("Skin preview cancelled; the profile's skin is \"{original}\" again."),
            Some(reason) => format!(
                "Talaria could not put the profile's skin back to \"{original}\" ({reason}). \
                 Run /skin {original} to restore it."
            ),
        }));
        return effects;
    }
    if let Some(reason) = failure {
        preview.stopped = true;
        if let Some(selector) = &mut state.choice {
            selector.list.set_note(Some(format!(
                "Preview stopped: {reason}. Enter still applies."
            )));
            effects.push(AppEffect::Redraw);
        }
    }
    let Some(preview) = &mut state.skin_preview else {
        return effects;
    };
    match preview.finish.take() {
        Some(Finish::Restore) => effects.extend(restore(state)),
        Some(Finish::Apply(name)) => {
            state.skin_preview = None;
            effects.push(AppEffect::Skin(Some(name)));
        }
        None if !preview.stopped && preview.wanted != preview.applied => {
            preview.timer += 1;
            effects.push(AppEffect::Choice(ChoiceWork::Rest(preview.timer)));
        }
        None => {}
    }
    effects
}

/// Before Talaria exits: write the original skin back when a preview changed it.
pub(super) async fn restore_on_exit(state: &mut AppState) {
    let Some(preview) = state.skin_preview.take() else {
        return;
    };
    if preview.applied == preview.original && !preview.in_flight {
        return;
    }
    let Some(client) = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned()
    else {
        return;
    };
    let session = session_id(state);
    let mut service = state.appearance.clone();
    let _ = tokio::time::timeout(
        RESTORE_WAIT,
        service.preview_skin(&client, &preview.original, session.as_deref()),
    )
    .await;
}
