use std::sync::Arc;

use crate::agent::Agent;
use crate::ids::AppName;
use crate::plugin::Plugin;

#[derive(Clone)]
pub struct App {
    pub name: AppName,
    pub root_agent: Agent,
    pub plugins: Vec<Arc<dyn Plugin>>,
}

impl App {
    pub fn new(name: AppName, root_agent: Agent) -> Self {
        Self {
            name,
            root_agent,
            plugins: Vec::new(),
        }
    }

    pub fn plugin(mut self, plugin: Arc<dyn Plugin>) -> Self {
        self.plugins.push(plugin);
        self
    }
}
