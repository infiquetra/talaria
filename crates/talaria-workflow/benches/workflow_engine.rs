use std::{sync::Arc, time::Instant};

use talaria_workflow::{
    engine::{EngineConfig, MapResolver, parse_script, run},
    mock::{MockBackend, MockOutcome},
};
use tokio_util::sync::CancellationToken;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "export const meta = {name:'benchmark',description:'engine throughput'}; return await parallel(Array.from({length:100}, (_,i)=>()=>agent('item '+i)));";
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let parsed = parse_script(source)?;
    let start = Instant::now();
    for _ in 0..20 {
        let backend = Arc::new(
            MockBackend::builder()
                .default_outcome(MockOutcome::Text("ok".into()))
                .build(),
        );
        let (events, _) = tokio::sync::mpsc::unbounded_channel();
        let outcome = runtime.block_on(run(
            parsed.clone(),
            EngineConfig::new("benchmark", 0, 0),
            backend,
            Arc::new(MapResolver::new()),
            events,
            CancellationToken::new(),
        ));
        assert!(
            matches!(
                outcome,
                talaria_workflow::engine::RunOutcome::Completed { .. }
            ),
            "{outcome:?}"
        );
    }
    println!("100 concurrent calls: {:?} per run", start.elapsed() / 20);
    Ok(())
}
