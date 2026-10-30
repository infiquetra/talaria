//! Compile orchestration before terminal initialization.
use super::CompileArgs;
use hermes_client::{Ticket, TicketError, TicketMinter, kanban::KanbanClient};
use serde_json::json;
use std::{future::Future, pin::Pin, sync::Arc};
use talaria_workflow::{
    hermes::LinkWireFactory,
    kanban_compile::{self, CompileFailure, CompileRequest},
    staffing::{self, LinkRosterSource, Roster, RosterSource, StaffingResolver, StaffingTable},
};
struct NoTicket;
impl TicketMinter for NoTicket {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async { Ticket::parse(String::new()) })
    }
}
pub async fn execute(args: CompileArgs) -> u8 {
    match run(args).await {
        Ok(report) => {
            print!("{}", report.stdout);
            if !report.stderr.is_empty() {
                eprintln!("{}", report.stderr);
            }
            report.exit_code
        }
        Err(error) => {
            eprintln!("{}", error.stderr);
            error.exit_code
        }
    }
}
async fn run(args: CompileArgs) -> Result<kanban_compile::CompileReport, CompileFailure> {
    let error = |code, e: String| CompileFailure {
        exit_code: code,
        stderr: e,
    };
    let source = std::fs::read_to_string(&args.script)
        .map_err(|e| error(2, format!("cannot read workflow script: {e}")))?;
    let input = serde_json::from_str(args.args.as_deref().unwrap_or("{}"))
        .map_err(|e| error(2, format!("invalid workflow arguments JSON: {e}")))?;
    if !serde_json::Value::is_object(&input) {
        return Err(error(2, "--args must be a JSON object".into()));
    }
    let base = args
        .script
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let captured = kanban_compile::inspect_calls(&source, &input, base)
        .await
        .map_err(|e| error(2, e))?;
    if captured.is_empty() {
        return Ok(kanban_compile::CompileReport::nothing_to_compile());
    }
    let profile = args
        .profile
        .as_ref()
        .or_else(|| captured.iter().find_map(|c| c.profile.as_ref()))
        .map(String::as_str)
        .unwrap_or("default")
        .trim()
        .to_lowercase();
    let needs_roster = captured
        .iter()
        .any(|c| c.tier.is_some() || c.model.is_some() || c.provider.is_some());
    let home =
        staffing::talaria_home().ok_or_else(|| error(1, "no home folder for Talaria".into()))?;
    let table = StaffingTable::load(&StaffingTable::default_path(&home))
        .map_err(|e| error(2, e.to_string()))?;
    let (origin, minter, roster): (String, Arc<dyn TicketMinter>, Roster) = if args.dry_run
        && !needs_roster
    {
        let roster =
            Roster::from_model_options(&json!({"providers":[]}), &profile, chrono::Utc::now())
                .map_err(|e| error(1, e.to_string()))?;
        ("http://127.0.0.1:1".into(), Arc::new(NoTicket), roster)
    } else {
        let root = hermes_client::default_home();
        let endpoint = Arc::new(
            hermes_client::discover_with(
                &root,
                &hermes_client::DiscoverOptions {
                    ensure: hermes_client::EnsurePolicy::Never,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                error(
                    1,
                    format!("{e}; start the gateway, then re-run; nothing was created"),
                )
            })?,
        );
        let id = endpoint
            .resolve_profile(&profile)
            .map_err(|e| error(2, e.to_string()))?;
        let minter = Arc::new(hermes_client::NativeHttpMinter::new(
            root.clone(),
            endpoint.clone(),
            id,
        ));
        if !args.dry_run {
            let board = KanbanClient::new(endpoint.api_origin(), &args.board, minter.clone())
                .map_err(|e| error(1, e.to_string()))?;
            let settings = board.orchestration().await.map_err(|e| {
                error(
                    1,
                    format!("cannot read orchestration settings: {e}; nothing was created"),
                )
            })?;
            if !settings.default_assignee.trim().is_empty() {
                return Err(error(
                    1,
                    format!(
                        "kanban.default_assignee is set to {}; unset it for the compile, then re-run",
                        settings.default_assignee
                    ),
                ));
            }
        }
        let snapshot = LinkRosterSource::new(Arc::new(LinkWireFactory::new(root)))
            .fetch(&profile)
            .await
            .map_err(|e| error(1, format!("cannot read roster: {e}; nothing was created")))?;
        let roster = Roster::from_model_options(&snapshot, &profile, chrono::Utc::now())
            .map_err(|e| error(1, e.to_string()))?;
        (endpoint.api_origin().into(), minter, roster)
    };
    let sink = KanbanClient::new(&origin, &args.board, minter.clone())
        .map_err(|e| error(2, e.to_string()))?;
    let request = CompileRequest {
        source,
        script_name: args.script.to_string_lossy().into_owned(),
        board: args.board,
        tenant: args.tenant,
        args: input,
        profile: args.profile,
        repo: args.repo,
        dry_run: args.dry_run,
        no_dispatch: args.no_dispatch,
        resolver: StaffingResolver::new(Arc::new(table), Arc::new(roster)),
        minter,
        origin,
    };
    kanban_compile::compile(request, &sink).await
}
