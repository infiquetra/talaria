mod staffing_support;
use serde_json::json;
use staffing_support::{data, request, resolver, table};
use talaria_workflow::staffing::RouteSource;
#[test]
fn resolution_table() {
    let t = table();
    let r = resolver(t.clone(), &data());
    let mut q = request();
    q.model = Some("m-small".into());
    q.provider = Some("alpha-cloud".into());
    q.tier = Some("survey".into());
    let result = r.route(&q).unwrap();
    assert_eq!(result.source, RouteSource::Explicit);
    assert_eq!(result.ignored_tier.as_deref(), Some("survey"));
    q.provider = None;
    q.tier = None;
    assert_eq!(r.route(&q).unwrap().provider, "alpha-cloud");
    q.model = None;
    q.provider = Some("alpha-cloud".into());
    assert_eq!(
        r.route(&q).unwrap_err().0,
        "provider 'alpha-cloud' needs a model; to move a tier to another provider, edit staffing.toml"
    );
    q.provider = None;
    q.tier = Some("survey".into());
    assert_eq!(
        r.route(&q).unwrap().source,
        RouteSource::Tier("survey".into())
    );
    q.tier = Some("missing".into());
    assert!(r.route(&q).unwrap_err().0.contains("tier 'missing'"));
    let empty = talaria_workflow::staffing::StaffingTable {
        tiers: Default::default(),
        ..table()
    };
    let no_table = resolver(empty, &data()).with_path("staffing.toml".into());
    assert_eq!(
        no_table.route(&q).unwrap_err().0,
        "tier 'missing' needs a staffing table; create staffing.toml"
    );
    let configured = resolver(table(), &data()).with_path("staffing.toml".into());
    assert_eq!(
        configured.route(&q).unwrap_err().0,
        "tier 'missing' is not in staffing.toml (staffing.toml); defined tiers: survey"
    );
    q.tier = None;
    assert_eq!(r.route(&q).unwrap().source, RouteSource::ProfileDefault);
    let mut default_table = table();
    default_table.default_tier = Some("survey".into());
    assert_eq!(
        resolver(default_table, &data()).route(&q).unwrap().source,
        RouteSource::Tier("survey".into())
    );
    q.profile = Some("default".into());
    assert_eq!(
        r.route(&q).unwrap_err().0,
        "profile 'default' was not read at run start; name a model and provider, or a tier"
    );
    q.model = Some("m-small".into());
    q.profile = Some("default".into());
    assert_eq!(
        r.route(&q).unwrap_err().0,
        "profile 'default' was not read at run start; name a model and provider, or a tier"
    );
    let mut ambiguous = data();
    ambiguous["providers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"slug":"beta-cloud","name":"Beta","authenticated":true,"models":["m-small"]}));
    let rr = resolver(t, &ambiguous);
    q.profile = None;
    assert_eq!(
        rr.route(&q).unwrap_err().0,
        "model 'm-small' is offered by alpha-cloud, beta-cloud; name a provider"
    );
}
#[test]
fn route_ignores_limits() {
    let mut limited = data();
    limited["providers"][0]["limit"] = json!({"scope":"account","resets_at":null});
    let ready = resolver(table(), &data());
    let waiting = resolver(table(), &limited);
    let mut q = request();
    q.tier = Some("survey".into());
    assert_eq!(ready.route(&q).unwrap(), waiting.route(&q).unwrap());
}
