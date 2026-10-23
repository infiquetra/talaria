fn main() {
    async fn accepted(client: &hermes_client::Client) {
        let params = hermes_contract::schemas::CanonicalListParams::default();
        let _ = client
            .call::<hermes_contract::methods::canonical::session_list::Marker>(params)
            .await;
    }
    let _ = accepted;
}
