#![allow(dead_code)]

#[path = "mock_gateway.rs"]
pub mod mock_gateway;
#[path = "../../../hermes-client/tests/support/mock_rest.rs"]
pub mod mock_rest;

use hermes_client::{Client, InboundStream};
use mock_gateway::MockGateway;
use mock_rest::{CountingMinter, MockRest};
use std::sync::Arc;
use talaria_app::{capabilities::CapabilitiesService, settings::MinterFactory};

pub struct Harness {
    pub rest: MockRest,
    pub gateway: MockGateway,
    pub socket: Client,
    pub minter: Arc<CountingMinter>,
    pub inbound: InboundStream,
}
impl Harness {
    pub async fn new() -> Self {
        let rest = MockRest::start()
            .await
            .allow_own_profile_selector("fixture")
            .with_capabilities();
        let (gateway, socket, inbound) = MockGateway::start().await;
        gateway.with_capabilities("fixture");
        Self {
            rest,
            gateway,
            socket,
            minter: Arc::new(CountingMinter::default()),
            inbound,
        }
    }
    pub fn service(&self) -> CapabilitiesService {
        let minter = self.minter.clone();
        let factory: MinterFactory = Arc::new(move |_| minter.clone());
        CapabilitiesService::new(
            Some(self.rest.origin()),
            vec!["fixture".into()],
            factory,
            self.socket.clone(),
            "/synthetic/hermes".into(),
            "/synthetic/home".into(),
        )
    }
}
