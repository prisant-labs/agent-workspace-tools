pub mod claude_history;
pub mod claude_json;
pub mod claude_projects;
pub mod plugin_state;
pub mod sweep;

use crate::model::Store;

pub fn registry() -> Vec<Box<dyn Store>> {
    vec![
        Box::new(claude_projects::ClaudeProjects),
        Box::new(claude_json::ClaudeJson),
        Box::new(claude_history::ClaudeHistory),
        Box::new(plugin_state::PluginState),
        Box::new(sweep::Sweep),
    ]
}
