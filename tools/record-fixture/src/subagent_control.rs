//! Record controls through the same client as the inbound event recorder.

use std::time::Duration;

use hermes_client::{Client, Inbound, InboundStream, ReplaySource};
use serde_json::{Value, json};

use crate::{AttemptResult, Scenario, Session, call_options, on_event, turn_status};

pub(super) async fn drive(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    session: &mut Session,
) -> AttemptResult {
    let mut child = None;
    let mut stopped = false;
    let mut completed = false;
    let mut interrupt_at = None;
    loop {
        if completed && !session.statuses.is_empty() {
            return AttemptResult::Ok;
        }
        let item = if let Some(deadline) = interrupt_at {
            tokio::select! {
                item = inbound.next() => item,
                () = tokio::time::sleep_until(deadline) => {
                    let Some(id) = child.as_deref() else {
                        return failed("the child disappeared before interrupt");
                    };
                    match call(client, "subagent.interrupt", session, id, None).await {
                        Ok(reply) if reply["found"] == true => stopped = true,
                        Ok(_) => return failed("the interrupt did not find the child"),
                        Err(result) => return result,
                    }
                    interrupt_at = None;
                    continue;
                }
            }
        } else {
            inbound.next().await
        };
        match item {
            Some(Inbound::Event(event)) => {
                if event.raw["session_id"] != session.session_id {
                    continue;
                }
                let payload = &event.raw["payload"];
                match event.raw["type"].as_str() {
                    Some("subagent.start") if child.is_none() => {
                        let Some(id) = payload["subagent_id"].as_str() else {
                            return failed("subagent.start has no id");
                        };
                        child = Some(id.to_owned());
                        match call(client, "subagent.list", session, id, None).await {
                            Ok(reply)
                                if reply["subagents"].as_array().is_some_and(|rows| {
                                    rows.iter().any(|row| {
                                        row["subagent_id"] == id && row["status"] == "running"
                                    })
                                }) => {}
                            Ok(_) => return failed("the list did not include the running child"),
                            Err(result) => return result,
                        }
                    }
                    Some("subagent.tool")
                        if interrupt_at.is_none()
                            && !stopped
                            && payload["subagent_id"].as_str() == child.as_deref() =>
                    {
                        let Some(id) = child.as_deref() else {
                            continue;
                        };
                        if let Err(result) = call(client, "subagent.tail", session, id, None).await
                        {
                            return result;
                        }
                        if let Err(result) = call(
                            client,
                            "subagent.steer",
                            session,
                            id,
                            Some("Reply with the single word steered after your current command."),
                        )
                        .await
                        {
                            return result;
                        }
                        interrupt_at = Some(tokio::time::Instant::now() + Duration::from_secs(5));
                    }
                    Some("subagent.complete")
                        if payload["subagent_id"].as_str() == child.as_deref() =>
                    {
                        completed = payload["status"] == "interrupted";
                        if !completed {
                            return failed("the child finished without interrupted status");
                        }
                    }
                    _ => {}
                }
                if let Some(result) = on_event(client, scenario, session, &event.raw).await {
                    return result;
                }
            }
            Some(Inbound::ServerRequest(request)) => {
                let _ = request
                    .responder
                    .fail(-32601, "the fixture script does not answer this request")
                    .await;
            }
            Some(Inbound::Malformed(_)) => {}
            Some(Inbound::Closed(_)) | None => {
                return failed("the connection closed during subagent controls");
            }
        }
    }
}

fn failed(reason: &str) -> AttemptResult {
    AttemptResult::Failed(reason.to_owned())
}

async fn call(
    client: &Client,
    method: &str,
    session: &Session,
    child: &str,
    text: Option<&str>,
) -> Result<Value, AttemptResult> {
    let mut params = json!({"session_id": session.session_id});
    if method != "subagent.list" {
        params["subagent_id"] = json!(child);
    }
    if let Some(text) = text {
        params["text"] = json!(text);
    }
    client
        .call_raw(method, params, call_options(Duration::from_secs(10)))
        .await
        .map_err(|error| AttemptResult::Failed(format!("{method} failed: {error}")))
}

/// Preserve the observed parent status; child-only interruption has no assumed parent result.
pub(super) fn record_statuses(text: &str) -> String {
    let mut rows: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let statuses: Vec<String> = rows
        .iter()
        .filter(|row| row["dir"] == "in" && row["frame"]["params"]["type"] == "message.complete")
        .map(|row| turn_status(&row["frame"]["params"]["payload"]))
        .collect();
    if let Some(header) = rows.first_mut() {
        header["expectedStatuses"] = json!(statuses);
    }
    rows.into_iter().map(|row| format!("{row}\n")).collect()
}

pub(super) fn check_evidence(source: &ReplaySource) -> Result<(), String> {
    let frames: Vec<&Value> = source
        .inbound()
        .chain(source.outbound())
        .map(|row| &row.frame)
        .collect();
    let mut child = None;
    for frame in &frames {
        if frame["method"] == "event" && frame["params"]["type"] == "subagent.start" {
            child = frame["params"]["payload"]["subagent_id"].as_str();
            break;
        }
    }
    let child = child.ok_or("no subagent.start")?;
    for method in [
        "subagent.list",
        "subagent.tail",
        "subagent.steer",
        "subagent.interrupt",
    ] {
        let request = frames
            .iter()
            .find(|frame| frame["method"] == method)
            .ok_or_else(|| format!("no {method} request"))?;
        let reply = frames
            .iter()
            .find(|frame| frame["id"] == request["id"] && frame.get("result").is_some())
            .ok_or_else(|| format!("no {method} answer"))?;
        let result = &reply["result"];
        let valid = match method {
            "subagent.list" => result["subagents"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["subagent_id"] == child && row["status"] == "running")
            }),
            "subagent.steer" => {
                result["subagent_id"] == child
                    && matches!(result["status"].as_str(), Some("queued" | "rejected"))
            }
            "subagent.interrupt" => result["subagent_id"] == child && result["found"] == true,
            _ => result["subagent_id"] == child,
        };
        if !valid {
            return Err(format!("{method} answer lacks required control evidence"));
        }
    }
    if !frames.iter().any(|frame| {
        frame["params"]["type"] == "subagent.complete"
            && frame["params"]["payload"]["subagent_id"] == child
            && frame["params"]["payload"]["status"] == "interrupted"
    }) {
        return Err("no interrupted completion for the controlled child".to_owned());
    }
    let statuses: Vec<String> = frames
        .iter()
        .filter(|frame| frame["params"]["type"] == "message.complete")
        .map(|frame| turn_status(&frame["params"]["payload"]))
        .collect();
    if statuses.is_empty() || statuses != source.header().expected_statuses {
        return Err("the header must record the observed parent status".to_owned());
    }
    Ok(())
}
