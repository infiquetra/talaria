//! Secondary links leave server requests for the foreground viewer.

use crate::mock_gateway;

use std::time::Duration;

use mock_gateway::StandaloneGateway;
use talaria_app::side_connection::SideConnection;

#[tokio::test]
async fn side_link_ignores_server_requests() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let connection = SideConnection::open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    let id = gateway.server_request("sudo").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let answers = gateway.answers_for(&id);
    assert!(
        answers.is_empty(),
        "the side link wrote an answer: {answers:?}"
    );
    connection.close().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let answers = gateway.answers_for(&id);
    assert!(
        answers.is_empty(),
        "closing the side link wrote an answer: {answers:?}"
    );
}
