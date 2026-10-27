use crossterm::event::KeyEvent;
use talaria_core::mcp::{McpEffect, McpInput, McpView, SecretSlot};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct McpWiring;

impl McpWiring {
    pub fn route_key(&mut self, _view: &McpView, _key: KeyEvent) -> Option<McpInput> {
        todo!("mcp stub")
    }
    pub fn route_paste(&mut self, _view: &McpView, _text: &str) -> Option<McpInput> {
        todo!("mcp stub")
    }
    pub fn secret_for(&self, _effect: &McpEffect) -> Option<Zeroizing<String>> {
        todo!("mcp stub")
    }
    pub fn forget(&mut self) {
        todo!("mcp stub")
    }
    pub fn secret_len(&self, _slot: SecretSlot) -> usize {
        todo!("mcp stub")
    }
}
