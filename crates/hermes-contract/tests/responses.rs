//! Test 31. The six recorded results decode as the canonical result types.
//!
//! The id-to-method mapping is inferred from order. The recording stores no
//! outbound frames. Id 105's status is the wire string `resolved`.

use hermes_contract::schemas::{CanonicalSubmitResult, Status};
use serde_json::Value as Json;

fn results() -> Vec<(i64, Json)> {
    include_str!("fixtures/live_two_turns.frames.jsonl")
        .lines()
        .filter_map(|line| {
            let value: Json = serde_json::from_str(line).expect("frame line is JSON");
            let id = value.get("id")?.as_i64()?;
            Some((id, value.get("result")?.clone()))
        })
        .collect()
}

fn result_of(id: i64) -> Json {
    results()
        .into_iter()
        .find(|(found, _)| *found == id)
        .unwrap_or_else(|| panic!("missing result {id}"))
        .1
}

#[test]
fn recorded_results_decode_into_canonical_result_types() {
    let ids: Vec<i64> = results().into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![101, 102, 103, 104, 105, 106]);

    let described: hermes_contract::methods::canonical::runtime_describe::Result =
        serde_json::from_value(result_of(101)).expect("runtime.describe");
    assert_eq!(described.instance_id, "id-0001");

    let created: hermes_contract::methods::canonical::session_create::Result =
        serde_json::from_value(result_of(102)).expect("session.create");
    assert_eq!(created.session_id, "local-session-1");

    for (id, sequence) in [(103, 68_i64), (104, 69)] {
        let submitted: hermes_contract::methods::canonical::prompt_submit::Result =
            serde_json::from_value(result_of(id))
                .unwrap_or_else(|error| panic!("prompt.submit {id} should decode, got {error}"));
        match submitted {
            CanonicalSubmitResult::AdmissionStatus(status) => {
                assert_eq!(status.status, Status::Queued);
                assert_eq!(status.sequence, sequence);
            }
            other => panic!("id {id} expected AdmissionStatus, got {other:?}"),
        }
    }

    let responded: hermes_contract::methods::canonical::approval_respond::Result =
        serde_json::from_value(result_of(105)).expect("approval.respond decodes status resolved");
    let again = serde_json::to_value(&responded).expect("round trip");
    assert_eq!(again["status"], "resolved");
    assert_eq!(again["prompt_id"], "id-0006");

    let detached: hermes_contract::methods::canonical::session_detach::Result =
        serde_json::from_value(result_of(106)).expect("session.detach");
    assert!(detached.detached);
    assert_eq!(detached.session_id, "local-session-1");
}
