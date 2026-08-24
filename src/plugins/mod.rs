pub mod stats;
pub mod health;

use crate::core::plugin::PluginManager;
use crate::core::manager::DarkstarManager;

pub fn load_all(plugin_manager: &mut PluginManager, manager: &mut DarkstarManager) {
    // 1. Load Bundled (Internal) Plugins
    plugin_manager.register_internal(Box::new(stats::EntityStatsPlugin::new()), manager);
    plugin_manager.register_internal(Box::new(health::HealthCheckerPlugin::new()), manager);

    // 2. Discover External Plugins (from local ./plugins and system config plugins/ directory)
    if let Ok(curr_dir) = std::env::current_dir() {
        let local_plugins_dir = curr_dir.join("plugins");
        plugin_manager.discover_external(&local_plugins_dir, manager);
    }

    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "darkstar", "ontology") {
        let plugins_dir = proj_dirs.config_dir().join("plugins");
        plugin_manager.discover_external(&plugins_dir, manager);
    }
}
