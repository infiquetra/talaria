//! Start the local gated gateway test double.
#[path = "../tests/support/mock_gated.rs"]
mod mock_gated;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = mock_gated::Options {
        auto_approve: args.iter().any(|a| a == "--auto-approve"),
        wrong_state: args.iter().any(|a| a == "--wrong-state"),
        ungated: args.iter().any(|a| a == "--ungated"),
        no_native: args.iter().any(|a| a == "--no-native"),
        refresh_expired: args.iter().any(|a| a == "--refresh-expired"),
        refresh_down: args.iter().any(|a| a == "--refresh-down"),
        rotating: args.iter().any(|a| a == "--rotating"),
        expire_now: args.iter().any(|a| a == "--expire-now"),
        no_authority: args.iter().any(|a| a == "--no-authority"),
        listen_both: args.iter().any(|a| a == "--listen-both"),
        refuse_upgrade: args
            .windows(2)
            .find(|w| w[0] == "--refuse")
            .and_then(|w| w[1].parse().ok()),
        profiles: args
            .windows(2)
            .find(|w| w[0] == "--profiles")
            .map(|w| w[1].split(',').map(str::to_owned).collect())
            .unwrap_or_default(),
        tls_files: args
            .windows(3)
            .find(|w| w[0] == "--tls")
            .map(|w| (w[1].clone().into(), w[2].clone().into())),
        ..Default::default()
    };
    let gateway = mock_gated::MockGateway::start(options).await;
    println!("{}", gateway.base());
    if let Some(ipv6) = gateway.base_v6() {
        eprintln!("IPv6: {ipv6}");
    }
    std::future::pending::<()>().await;
}
