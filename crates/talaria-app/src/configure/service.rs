use std::marker::PhantomData;

use talaria_core::configure::{ConfigureEffect, ConfigureInput};

use super::ConfigureBackends;

pub struct ConfigureService<B: ConfigureBackends> {
    marker: PhantomData<B>,
}

impl<B: ConfigureBackends> ConfigureService<B> {
    pub fn new(_client: Option<hermes_client::Client>, _backends: B, _profile: String) -> Self {
        todo!()
    }

    pub async fn execute(&self, _effect: ConfigureEffect) -> Vec<ConfigureInput> {
        todo!()
    }

    pub async fn close(&self) {
        todo!()
    }
}
