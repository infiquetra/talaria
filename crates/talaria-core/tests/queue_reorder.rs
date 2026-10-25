//! Issue 232. Queue and steer. Reorder by sending rows to the back.

use proptest::prop_assert_eq;
use talaria_core::queue::AdmissionId;
use talaria_core::queue::reorder::{Placement, move_target, plan_reorder};

fn id(name: &str) -> AdmissionId {
    AdmissionId(name.to_string())
}

fn ids(names: &[&str]) -> Vec<AdmissionId> {
    names.iter().copied().map(id).collect()
}

fn names(rows: &[AdmissionId]) -> Vec<&str> {
    rows.iter().map(|row| row.0.as_str()).collect()
}

#[test]
fn move_up_by_one() {
    let current = ids(&["a", "b", "c", "d"]);
    let target = move_target(&current, &id("c"), Placement::Up);
    assert_eq!(names(&target), ["a", "c", "b", "d"]);
    assert_eq!(names(&plan_reorder(&current, &target)), ["b", "d"]);
}

#[test]
fn send_now_to_head() {
    let current = ids(&["a", "b", "c", "d"]);
    let target = move_target(&current, &id("d"), Placement::Head);
    assert_eq!(names(&target), ["d", "a", "b", "c"]);
    assert_eq!(names(&plan_reorder(&current, &target)), ["a", "b", "c"]);
}

#[test]
fn edit_restores_place() {
    let current = ids(&["a", "c", "d", "e"]);
    let target = ids(&["a", "e", "c", "d"]);
    assert_eq!(names(&plan_reorder(&current, &target)), ["c", "d"]);
}

#[test]
fn move_target_ends_and_index_past_end() {
    let current = ids(&["a", "b", "c"]);
    assert_eq!(move_target(&current, &id("a"), Placement::Up), current);
    assert_eq!(move_target(&current, &id("c"), Placement::Down), current);
    assert_eq!(
        names(&move_target(&current, &id("a"), Placement::Index(99))),
        ["b", "c", "a"]
    );
    assert!(plan_reorder(&current, &current).is_empty());
    assert_eq!(
        names(&plan_reorder(&current, &ids(&["c", "b", "a"]))),
        ["b", "a"]
    );
}

fn kept_prefix(current: &[AdmissionId], target: &[AdmissionId]) -> usize {
    let mut rest = current;
    let mut kept = 0;
    for wanted in target {
        match rest.iter().position(|row| row == wanted) {
            Some(index) => {
                rest = &rest[index + 1..];
                kept += 1;
            }
            None => break,
        }
    }
    kept
}

fn readmit(current: &[AdmissionId], plan: &[AdmissionId]) -> Vec<AdmissionId> {
    let mut rows = current.to_vec();
    for step in plan {
        rows.retain(|row| row != step);
        rows.push(step.clone());
    }
    rows
}

fn permutations() -> impl proptest::strategy::Strategy<Value = (Vec<AdmissionId>, Vec<AdmissionId>)>
{
    use proptest::prelude::{Just, Strategy};
    (0usize..=32).prop_flat_map(|width| {
        let base: Vec<AdmissionId> = (0..width)
            .map(|index| id(&format!("row-{index}")))
            .collect();
        (Just(base.clone()).prop_shuffle(), Just(base).prop_shuffle())
    })
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig {
        cases: 256,
        ..proptest::prelude::ProptestConfig::default()
    })]

    #[test]
    fn plan_reaches_any_target_minimally((current, target) in permutations()) {
        let plan = plan_reorder(&current, &target);
        let kept = kept_prefix(&current, &target);
        prop_assert_eq!(plan.len(), current.len() - kept);
        prop_assert_eq!(readmit(&current, &plan), target);
    }
}
