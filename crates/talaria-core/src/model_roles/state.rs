//! The model roles page. Inputs in, effects out.

use super::{RolesEffect, RolesInput, RolesView};

pub struct ModelRolesState {
    profile: String,
}

impl ModelRolesState {
    pub fn new(profile: String) -> (Self, Vec<RolesEffect>) {
        let state = Self { profile };
        let _ = state.profile;
        todo!("model roles stub")
    }

    pub fn update(&mut self, input: RolesInput) -> Vec<RolesEffect> {
        let _ = input;
        todo!("model roles stub")
    }

    pub fn view(&self) -> RolesView {
        let _ = self.profile;
        todo!("model roles stub")
    }
}
