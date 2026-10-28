use talaria_workflow::dry_run;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let opts = match dry_run::parse_args(&args) {
        Ok(opts) => opts,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::from(64);
        }
    };
    let cancel = CancellationToken::new();
    let signal_cancel = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_cancel.cancel();
        }
    });
    let code = dry_run::run(&opts, &mut std::io::stdout(), cancel).await;
    std::process::ExitCode::from(code as u8)
}
