mod staffing_support;
use std::{sync::Arc, time::Duration};
use talaria_workflow::staffing::{FixtureRosterSource, ManualClock, RosterCache, RosterError};
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn roster_cache_reuses_and_refreshes() {
    let clock = Arc::new(ManualClock::new(staffing_support::now()));
    let source = Arc::new(FixtureRosterSource::with_clock(clock.clone()));
    source.push("fixture", Ok(staffing_support::data()));
    let mut changed = staffing_support::data();
    changed["model"] = "m-large".into();
    source.push("fixture", Ok(changed));
    source.push("default", Ok(staffing_support::data()));
    let cache = RosterCache::new(source.clone(), clock.clone(), Duration::from_secs(30));
    let first = cache.get_or_load("fixture").await.unwrap();
    let reused = cache.get_or_load("fixture").await.unwrap();
    assert!(Arc::ptr_eq(&first, &reused));
    assert_eq!(source.fetches().len(), 1);
    let before = cache.refresh("fixture", false).await.unwrap();
    assert!(Arc::ptr_eq(&first, &before));
    assert_eq!(source.fetches().len(), 1);
    clock.advance(chrono::Duration::seconds(30));
    let refreshed = cache.refresh("fixture", false).await.unwrap();
    assert!(!Arc::ptr_eq(&first, &refreshed));
    assert_eq!(refreshed.current().0, "m-large");
    let other = cache.load("default").await.unwrap();
    assert_eq!(other.profile(), "default");
    assert_eq!(source.fetches().len(), 3);
    source.push("fixture", Err(RosterError::Timeout));
    assert!(cache.refresh("fixture", true).await.is_err());
    assert!(Arc::ptr_eq(&refreshed, &cache.cached("fixture").unwrap()));
}

#[tokio::test(flavor = "current_thread")]
async fn each_profile_has_its_own_connection() {
    use futures_util::future::BoxFuture;
    use serde_json::{Map, Value, json};
    use std::sync::Mutex;
    use talaria_workflow::{
        hermes::{Wire, WireError, WireEvent, WireFactory, wire::Cursor},
        staffing::{LinkRosterSource, RosterSource},
    };
    #[derive(Default)]
    struct Factory {
        opened: Arc<Mutex<Vec<String>>>,
        calls: Arc<Mutex<Vec<(String, String, Value)>>>,
    }
    struct TestWire {
        profile: String,
        calls: Arc<Mutex<Vec<(String, String, Value)>>>,
    }
    impl WireFactory for Factory {
        fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
            self.opened.lock().unwrap().push(profile.into());
            let wire = TestWire {
                profile: profile.into(),
                calls: self.calls.clone(),
            };
            Box::pin(async move { Ok(Arc::new(wire) as Arc<dyn Wire>) })
        }
    }
    impl Wire for TestWire {
        fn call(
            &self,
            method: &'static str,
            params: Map<String, Value>,
            timeout: Duration,
        ) -> BoxFuture<'static, Result<Value, WireError>> {
            assert_eq!(timeout, Duration::from_secs(15));
            let mut calls = self.calls.lock().unwrap();
            calls.push((self.profile.clone(), method.into(), Value::Object(params)));
            let error = calls.len() == 4;
            drop(calls);
            Box::pin(async move {
                if error {
                    Err(WireError::Refused {
                        code: 4001,
                        message: "not authenticated".into(),
                        data: json!({}),
                    })
                } else {
                    Ok(staffing_support::data())
                }
            })
        }
        fn generation(&self) -> u64 {
            1
        }
        fn track(&self, _: &str, _: Cursor) {}
        fn resync(&self, _: &str, _: Cursor) {}
        fn take_events(&self) -> Option<tokio::sync::mpsc::Receiver<WireEvent>> {
            None
        }
    }
    let factory = Arc::new(Factory::default());
    let source = LinkRosterSource::new(factory.clone());
    for profile in ["default", "fixture", "default"] {
        source.fetch(profile).await.unwrap();
    }
    assert_eq!(*factory.opened.lock().unwrap(), vec!["default", "fixture"]);
    for (profile, method, params) in factory.calls.lock().unwrap().iter() {
        assert!(["default", "fixture"].contains(&profile.as_str()));
        assert_eq!(method, "model.options");
        assert_eq!(*params, json!({}));
    }
    assert!(matches!(
        source.fetch("default").await,
        Err(RosterError::Rpc { code: 4001, .. })
    ));
}
