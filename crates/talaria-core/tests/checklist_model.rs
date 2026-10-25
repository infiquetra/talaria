//! The typed checklist: parsing, tree order, counts, focus.

use proptest::prelude::*;
use serde_json::{Value, json};
use talaria_core::checklist::{Checklist, TodoStatus, TreeRow};
use talaria_core::session::TodoState;

fn todos_41() -> Vec<Value> {
    vec![
        json!({"id": "1", "content": "Read the failing test", "status": "completed"}),
        json!({"id": "2", "content": "Fix average()", "status": "in_progress"}),
        json!({"id": "3", "content": "Run the test suite", "status": "pending"}),
    ]
}

fn state_41() -> TodoState {
    TodoState {
        revision: 2,
        todos: todos_41(),
    }
}

#[test]
fn parses_contract_shaped_items() {
    let list = Checklist::from_state(&state_41());
    assert_eq!(list.revision, 2);
    assert_eq!(list.items.len(), 3);
    assert_eq!(list.skipped, 0);
    assert_eq!(list.items[0].id, "1");
    assert_eq!(list.items[0].status, TodoStatus::Completed);
    assert_eq!(list.items[1].status, TodoStatus::InProgress);
    assert_eq!(list.items[2].status, TodoStatus::Pending);
    let negative = Checklist::from_state(&TodoState {
        revision: -1,
        todos: todos_41(),
    });
    assert_eq!(negative.revision, 0);
    assert!(negative.items.is_empty());
    assert_eq!(negative.skipped, 0);
}

#[test]
fn skips_malformed_items_and_counts_them() {
    // Robustness data: Hermes's own tool cannot send these (it normalises
    // first); another source could, so they must skip without breaking.
    let items = [
        json!(42),
        json!({"id": "x", "content": "c", "status": "done"}),
        json!({"id": " ", "content": "c", "status": "pending"}),
        json!({"id": "y", "content": "  ", "status": "pending"}),
        json!({"id": null, "content": "c", "status": "pending"}),
        json!({"id": "z", "content": 5, "status": "pending"}),
        json!({"id": " a ", "content": " Write it ", "status": "pending"}),
        json!({"id": "?", "content": "(invalid item)", "status": "pending"}),
    ];
    let list = Checklist::from_values(1, &items);
    assert_eq!(list.skipped, 6);
    assert_eq!(list.items.len(), 2);
    assert_eq!(list.items[0].id, "a");
    assert_eq!(list.items[0].content, "Write it");
    assert_eq!(list.items[1].id, "?");
    assert!(!list.is_empty());
    let all_bad = Checklist::from_values(
        1,
        &[
            json!(42),
            json!({"id": true, "content": "c", "status": "pending"}),
        ],
    );
    assert_eq!(all_bad.skipped, 2);
    assert!(all_bad.is_empty());
}

#[test]
fn ids_parents_and_unknown_keys() {
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": 7, "content": "seven", "status": "pending"}),
            json!({"id": "8", "content": "eight", "status": "pending", "parent": 7}),
            json!({"id": "9", "content": "nine", "status": "pending", "parent": "9", "priority": 1}),
        ],
    );
    assert_eq!(list.items[0].id, "7");
    assert_eq!(list.items[1].parent.as_deref(), Some("7"));
    assert_eq!(list.items[2].parent, None);
    assert_eq!(
        list.tree(),
        vec![
            TreeRow { index: 0, depth: 0 },
            TreeRow { index: 1, depth: 1 },
            TreeRow { index: 2, depth: 0 },
        ]
    );
}

#[test]
fn tree_orders_parents_before_children() {
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": "a", "content": "a", "status": "pending"}),
            json!({"id": "b", "content": "b", "status": "pending", "parent": "a"}),
            json!({"id": "c", "content": "c", "status": "pending"}),
            json!({"id": "d", "content": "d", "status": "pending", "parent": "b"}),
        ],
    );
    assert_eq!(
        list.tree(),
        vec![
            TreeRow { index: 0, depth: 0 },
            TreeRow { index: 1, depth: 1 },
            TreeRow { index: 3, depth: 2 },
            TreeRow { index: 2, depth: 0 },
        ]
    );
}

#[test]
fn tree_degrades_dangling_and_cyclic_parents() {
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": "a", "content": "a", "status": "pending", "parent": "b"}),
            json!({"id": "b", "content": "b", "status": "pending", "parent": "a"}),
            json!({"id": "c", "content": "c", "status": "pending", "parent": "zzz"}),
            json!({"id": "e", "content": "e", "status": "pending", "parent": "a"}),
        ],
    );
    assert_eq!(
        list.tree(),
        vec![
            TreeRow { index: 2, depth: 0 },
            TreeRow { index: 0, depth: 0 },
            TreeRow { index: 1, depth: 0 },
            TreeRow { index: 3, depth: 0 },
        ]
    );
}

#[test]
fn tree_keeps_duplicate_ids_by_index() {
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": "x", "content": "first", "status": "pending"}),
            json!({"id": "x", "content": "second", "status": "pending"}),
            json!({"id": "k", "content": "k", "status": "pending", "parent": "x"}),
        ],
    );
    assert_eq!(
        list.tree(),
        vec![
            TreeRow { index: 0, depth: 0 },
            TreeRow { index: 2, depth: 1 },
            TreeRow { index: 1, depth: 0 },
        ]
    );
}

#[test]
fn counts_and_activity() {
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": "a", "content": "a", "status": "pending"}),
            json!({"id": "b", "content": "b", "status": "in_progress"}),
            json!({"id": "c", "content": "c", "status": "completed"}),
            json!({"id": "d", "content": "d", "status": "cancelled"}),
        ],
    );
    let counts = list.counts();
    assert_eq!(counts.total, 4);
    assert_eq!(counts.completed, 1);
    assert_eq!(counts.in_progress, 1);
    assert_eq!(counts.pending, 1);
    assert_eq!(counts.cancelled, 1);
    assert!(list.is_active());
    let closed = Checklist::from_values(
        1,
        &[
            json!({"id": "a", "content": "a", "status": "completed"}),
            json!({"id": "b", "content": "b", "status": "cancelled"}),
        ],
    );
    assert!(!closed.is_active());
}

#[test]
fn focus_prefers_in_progress_then_pending() {
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": "a", "content": "a", "status": "pending"}),
            json!({"id": "b", "content": "b", "status": "in_progress"}),
            json!({"id": "c", "content": "c", "status": "pending"}),
        ],
    );
    assert_eq!(list.focus(), Some(1));
    let list = Checklist::from_values(
        1,
        &[
            json!({"id": "a", "content": "a", "status": "completed"}),
            json!({"id": "b", "content": "b", "status": "pending"}),
        ],
    );
    assert_eq!(list.focus(), Some(1));
    let list = Checklist::from_values(
        1,
        &[json!({"id": "a", "content": "a", "status": "completed"})],
    );
    assert_eq!(list.focus(), None);
}

fn todo_value() -> impl Strategy<Value = Value> {
    (0..8usize, prop::option::of(0..8usize), 0..4usize).prop_map(|(id, parent, status)| {
        let words = ["pending", "in_progress", "completed", "cancelled"];
        let mut item = json!({
            "id": format!("p{id}"),
            "content": "step",
            "status": words[status],
        });
        if let Some(pool) = parent {
            item["parent"] = json!(format!("p{pool}"));
        }
        item
    })
}

proptest! {
    #[test]
    fn tree_lists_every_item_once(items in prop::collection::vec(todo_value(), 0..40)) {
        let list = Checklist::from_values(1, &items);
        let tree = list.tree();
        let mut seen: Vec<usize> = tree.iter().map(|row| row.index).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..items.len()).collect::<Vec<_>>());
        let position_of = |index: usize| {
            tree.iter().position(|row| row.index == index).expect("every index once")
        };
        for (index, item) in items.iter().enumerate() {
            let Some(parent_id) = item.get("parent").and_then(Value::as_str) else {
                continue;
            };
            let Some(parent) = items.iter().position(|other| other["id"] == parent_id) else {
                continue;
            };
            if parent == index {
                continue;
            }
            let mut chain = parent;
            let mut visited = vec![index];
            let mut rooted = false;
            while !visited.contains(&chain) {
                visited.push(chain);
                let Some(next) = items[chain].get("parent").and_then(Value::as_str) else {
                    rooted = true;
                    break;
                };
                let Some(next_index) = items.iter().position(|other| other["id"] == next) else {
                    rooted = true;
                    break;
                };
                if next_index == chain {
                    rooted = true;
                    break;
                }
                chain = next_index;
            }
            if rooted {
                assert!(
                    position_of(index) > position_of(parent),
                    "child {index} follows its parent {parent}"
                );
            }
        }
    }
}
