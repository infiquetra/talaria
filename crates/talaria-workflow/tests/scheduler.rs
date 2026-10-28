mod scheduler_support;
use futures_util::poll;
use scheduler_support::*;
use serde_json::json;
use std::task::Poll;
use talaria_workflow::{
    journal::{AdmissionGate, GateDecision, GateReason, SettledTurn},
    schedule::{BudgetKind, CapScope, SchedulerEvent, WaitReason},
    staffing::{ProviderLimits, RunBudget},
};
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn global_width_bounds_in_flight() {
    let mut data = roster_data();
    data["providers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"slug":"gamma","name":"Gamma","authenticated":true,"models":["m"]}));
    let h = Harness::new(10, data).await;
    let (s, mut events) = h.start(no_budget()).await;
    let mut held = Vec::new();
    let mut waiting = Vec::new();
    for i in 0..25 {
        let l = launch("fixture", ["alpha", "beta", "gamma"][(i % 3) as usize]);
        let mut f = s.acquire(&request(i), &l, GateReason::Fresh);
        match poll!(&mut f) {
            Poll::Ready(d) => held.push(permit(d)),
            Poll::Pending => waiting.push((i, f)),
        }
    }
    assert_eq!(held.len(), 10);
    for (_, future) in waiting {
        drop(held.remove(0));
        held.push(permit(future.await));
        assert!(s.status().in_flight <= 10);
    }
    drop(held);
    assert_eq!(s.status().in_flight, 0);
    let admitted: Vec<_> = std::iter::from_fn(|| events.0.try_recv().ok())
        .filter_map(|e| match e.event {
            SchedulerEvent::Admitted { key, provider, .. } => Some((provider, key)),
            _ => None,
        })
        .collect();
    assert_eq!(admitted.len(), 25);
    for (index, provider) in ["alpha", "beta", "gamma"].iter().enumerate() {
        let expected: Vec<_> = (0..25)
            .filter(|i| (*i as usize) % 3 == index)
            .map(|i| {
                talaria_workflow::journal::journal_key(
                    &request(i).call_id,
                    &launch("fixture", provider),
                )
            })
            .collect();
        let actual: Vec<_> = admitted
            .iter()
            .filter(|(lane, _)| lane == provider)
            .map(|(_, key)| key.clone())
            .collect();
        assert_eq!(actual, expected);
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn lanes_are_per_profile_and_provider() {
    let mut value = roster_data();
    value["providers"][0]["limit"] = json!({"scope":"account","resets_at":null});
    let mut h = Harness::new(10, value).await;
    h.table
        .providers
        .insert("alpha".into(), ProviderLimits { max_in_flight: 2 });
    h.source.push("default", Ok(roster_data()));
    let (s, _) = h.start(no_budget()).await;
    let mut blocked = s.acquire(&request(0), &launch("fixture", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut blocked).is_pending());
    let other = s.acquire(&request(1), &launch("default", "alpha"), GateReason::Fresh);
    drop(permit(other.await));
    drop(acquire(&s, 2, "fixture", "beta").await);
    assert!(poll!(&mut blocked).is_pending());
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn limited_provider_waits_until_reset() {
    let mut data = roster_data();
    data["providers"][0]["limit"] = json!({"scope":"account","resets_at":"2026-10-22T16:05:00Z"});
    let h = Harness::new(10, data).await;
    h.source.push("fixture", Ok(roster_data()));
    let (s, mut events) = h.start(no_budget()).await;
    let mut f = s.acquire(&request(0), &launch("fixture", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut f).is_pending());
    drop(acquire(&s, 1, "fixture", "beta").await);
    h.advance(299).await;
    assert!(poll!(&mut f).is_pending());
    h.advance(1).await;
    drop(permit(f.await));
    assert_eq!(h.source.fetches().len(), 2);
    assert!(
        std::iter::from_fn(|| events.0.try_recv().ok()).any(|e| matches!(
            e.event,
            SchedulerEvent::Queued {
                reason: WaitReason::ProviderLimited { .. },
                ..
            }
        ))
    );
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unknown_reset_rechecks_every_cooldown() {
    let mut data = roster_data();
    data["providers"][0]["limit"] = json!({"scope":"account","resets_at":null});
    let h = Harness::new(10, data.clone()).await;
    h.source.push("fixture", Ok(data));
    h.source.push("fixture", Ok(roster_data()));
    let (s, _) = h.start(no_budget()).await;
    let mut f = s.acquire(&request(0), &launch("fixture", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut f).is_pending());
    h.advance(59).await;
    assert_eq!(h.source.fetches().len(), 1);
    h.advance(1).await;
    assert_eq!(h.source.fetches().len(), 2);
    assert!(poll!(&mut f).is_pending());
    h.advance(60).await;
    drop(permit(f.await));
    assert_eq!(h.source.fetches().len(), 3);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn reattach_never_waits() {
    let h = Harness::new(1, roster_data()).await;
    let (s, _) = h.start(no_budget()).await;
    let a = acquire(&s, 0, "fixture", "alpha").await;
    let b = permit(
        s.acquire(
            &request(1),
            &launch("fixture", "alpha"),
            GateReason::Reattach,
        )
        .await,
    );
    assert_eq!(s.status().in_flight, 2);
    let mut c = s.acquire(&request(2), &launch("fixture", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut c).is_pending());
    drop(a);
    assert!(poll!(&mut c).is_pending());
    drop(b);
    drop(permit(c.await));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn first_rate_limit_halves_global_width_once() {
    let h = Harness::new(10, roster_data()).await;
    let (s, mut events) = h.start(no_budget()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    let p = permit(s.acquire(&q, &l, GateReason::Fresh).await);
    h.settle(&q, &l, 0, 0, Some("rate_limit"), json!({"total_tokens":1}));
    h.advance(1).await;
    drop(p);
    assert_eq!(s.status().global_width, 5);
    let q2 = request(1);
    h.settle(&q2, &l, 0, 0, Some("rate_limit"), json!({"total_tokens":1}));
    h.advance(1).await;
    let _ = s.budget();
    assert_eq!(s.status().global_width, 5);
    let halves: Vec<_> = std::iter::from_fn(|| events.0.try_recv().ok())
        .filter_map(|e| {
            if let SchedulerEvent::CapHalved { scope, from, to } = e.event {
                Some((scope, from, to))
            } else {
                None
            }
        })
        .collect();
    assert!(
        halves
            .iter()
            .any(|(scope, from, to)| matches!(scope, CapScope::Global) && *from == 10 && *to == 5)
    );
    assert_eq!(halves.len(), 3);
    assert!(matches!(&halves[0], (CapScope::Global, 10, 5)));
    assert!(
        matches!(&halves[1],(CapScope::Lane { profile, provider },10,5) if profile=="fixture" && provider=="alpha")
    );
    assert!(
        matches!(&halves[2],(CapScope::Lane { profile, provider },5,2) if profile=="fixture" && provider=="alpha")
    );
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rate_limit_pauses_provider() {
    let h = Harness::new(10, roster_data()).await;
    h.source.push("fixture", Ok(roster_data()));
    let (s, _) = h.start(no_budget()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    let p = permit(s.acquire(&q, &l, GateReason::Fresh).await);
    h.settle(&q, &l, 0, 0, Some("rate_limit"), json!({"total_tokens":1}));
    h.advance(1).await;
    drop(p);
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    let mut next = s.acquire(&request(1), &l, GateReason::Fresh);
    assert!(poll!(&mut next).is_pending());
    drop(acquire(&s, 2, "fixture", "beta").await);
    h.advance(60).await;
    drop(permit(
        tokio::time::timeout(std::time::Duration::from_secs(1), next)
            .await
            .expect("provider remains paused after reset"),
    ));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn billing_skips_the_provider_for_this_execution() {
    let h = Harness::new(10, roster_data()).await;
    let (s, _) = h.start(no_budget()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    let p = permit(s.acquire(&q, &l, GateReason::Fresh).await);
    h.settle(&q, &l, 0, 0, Some("billing"), json!({"total_tokens":1}));
    h.advance(1).await;
    drop(p);
    assert!(matches!(
        s.acquire(&request(1), &l, GateReason::Fresh).await,
        GateDecision::Skip
    ));
    drop(acquire(&s, 2, "fixture", "beta").await);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn retry_decision() {
    let h = Harness::new(10, roster_data()).await;
    let (s, _) = h.start(no_budget()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    for reason in ["rate_limit", "upstream_rate_limit", "overloaded"] {
        let settled = SettledTurn {
            turn: 0,
            outcome: "failed".into(),
            final_text: None,
            result_meta: json!({"failure_reason":reason}),
            usage: None,
        };
        assert!(s.retry_after_failure(&q, &l, &settled, 0));
    }
    for reason in ["billing", "other"] {
        let settled = SettledTurn {
            turn: 0,
            outcome: "failed".into(),
            final_text: None,
            result_meta: json!({"failure_reason":reason}),
            usage: None,
        };
        assert!(!s.retry_after_failure(&q, &l, &settled, 0));
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn events_are_timed_and_complete() {
    let h = Harness::new(10, roster_data()).await;
    let (s, mut events) = h.start(no_budget()).await;
    for i in 0..100 {
        drop(acquire(&s, i, "fixture", "alpha").await);
    }
    let all: Vec<_> = std::iter::from_fn(|| events.0.try_recv().ok()).collect();
    assert_eq!(all.len(), 200);
    assert!(all.windows(2).all(|v| v[0].at <= v[1].at));
    assert!(all.iter().all(|e| e.at.starts_with("2026-10-22T16:00:00")));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn money_budget_stops_admission() {
    let h = Harness::new(1, roster_data()).await;
    let (s, mut events) = h
        .start(RunBudget {
            tokens: None,
            money_usd: Some(0.02),
        })
        .await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    let p = permit(s.acquire(&q, &l, GateReason::Fresh).await);
    let mut waiting = s.acquire(&request(1), &l, GateReason::Fresh);
    let mut waiting_two = s.acquire(&request(2), &l, GateReason::Fresh);
    assert!(poll!(&mut waiting).is_pending());
    assert!(poll!(&mut waiting_two).is_pending());
    h.settle(&q,&l,0,0,None,json!({"total_tokens":1,"session_cost":{"estimated_cost_usd":0.025,"cost_status":"actual"}}));
    h.advance(1).await;
    drop(p);
    assert!(matches!(waiting.await, GateDecision::Skip));
    assert!(matches!(waiting_two.await, GateDecision::Skip));
    assert!(matches!(
        s.acquire(&request(3), &l, GateReason::Fresh).await,
        GateDecision::Skip
    ));
    let (resumed, _) = h
        .start(RunBudget {
            tokens: None,
            money_usd: Some(0.02),
        })
        .await;
    assert!(matches!(
        resumed.acquire(&request(4), &l, GateReason::Fresh).await,
        GateDecision::Skip
    ));
    assert!(
        std::iter::from_fn(|| events.0.try_recv().ok()).any(|e| matches!(
            e.event,
            SchedulerEvent::BudgetReached {
                kind: BudgetKind::Money,
                ..
            }
        ))
    );
}
