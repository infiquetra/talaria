//! Message-driven bridge between the application loop and owned agents.

use std::sync::Arc;

use talaria_core::fleet::RowKey;
use talaria_core::fleet::acp::{AcpMapping, AcpSessionKey};
use tokio::sync::mpsc;

use super::{AcpConfig, AcpService, PermissionResponder};
use crate::app::AppMessage;
use crate::fleet::{ExternalCommand, ExternalHandle, ExternalResult, FleetReporter};

pub(crate) fn start(
    config: AcpConfig,
    fleet: FleetReporter,
    app: mpsc::UnboundedSender<AppMessage>,
) -> ExternalHandle {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let updates = app.clone();
    let observer = Arc::new(move |key: AcpSessionKey, view: AcpMapping| {
        let _ = updates.send(AppMessage::AcpChanged { key, view });
    });
    let mut service = AcpService::attach(config, fleet, observer);
    let join = tokio::spawn(async move {
        while let Some(command) = receiver.recv().await {
            let result = match command {
                ExternalCommand::Launch {
                    agent_id,
                    cwd,
                    prompt,
                } => Some(ExternalResult::Launch(
                    service
                        .new_session(&agent_id, &cwd, &prompt)
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                )),
                ExternalCommand::Prompt { key, text } => {
                    let result = if let Some(session) = session_key(&key) {
                        service
                            .prompt(&session, &text)
                            .await
                            .map_err(|error| error.to_string())
                    } else {
                        Err("Agent session is unavailable.".into())
                    };
                    Some(ExternalResult::Prompt { key, result })
                }
                ExternalCommand::Cancel { key } => {
                    let result = if let Some(session) = session_key(&key) {
                        service
                            .cancel(&session)
                            .await
                            .map_err(|error| error.to_string())
                    } else {
                        Err("Agent session is unavailable.".into())
                    };
                    Some(ExternalResult::Cancel { key, result })
                }
                ExternalCommand::Answer {
                    key,
                    responder,
                    tool_call_id,
                    option_id,
                } => {
                    let result = match (session_key(&key), responder.split_once(':')) {
                        (Some(session), Some((generation, id))) => match generation.parse() {
                            Ok(generation) => service
                                .answer_permission(
                                    &PermissionResponder {
                                        generation,
                                        request_id: id.into(),
                                        key: session,
                                        tool_call_id,
                                    },
                                    &option_id,
                                )
                                .await
                                .map_err(|error| error.to_string()),
                            Err(_) => Err("Permission is unavailable.".into()),
                        },
                        _ => Err("Permission is unavailable.".into()),
                    };
                    Some(ExternalResult::Answer { result })
                }
                ExternalCommand::Acknowledge { key } => {
                    if let Some(session) = session_key(&key) {
                        service.acknowledge(&session);
                    }
                    None
                }
                ExternalCommand::Shutdown(reply) => {
                    service.close().await;
                    let _ = reply.send(());
                    break;
                }
            };
            if let Some(result) = result {
                let _ = app.send(AppMessage::AcpResult(result));
            }
        }
        service.close().await;
    });
    ExternalHandle::new(sender, join)
}

fn session_key(key: &RowKey) -> Option<AcpSessionKey> {
    Some(AcpSessionKey {
        agent_id: key.profile.strip_prefix("acp:")?.to_owned(),
        session_id: key.session_id.clone(),
    })
}
