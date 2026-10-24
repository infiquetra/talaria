fn main() {
    async fn rejected(client: &hermes_client::Client) {
        let _ = client
            .call::<hermes_contract::methods::legacy::session_compress::Marker>(todo!())
            .await;
    }
    let _ = rejected;
}
