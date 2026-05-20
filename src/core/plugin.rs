use crate::core::manager::DarkstarManager;
use crate::core::history::DarkstarEvent;
use eframe::egui;
use std::path::Path;

use crate::core::l10n::L10n;

/// The core trait that all Darkstar plugins must implement.
pub trait DarkstarPlugin {
    fn name(&self, l10n: &L10n) -> String;
    fn description(&self, l10n: &L10n) -> String;
    fn version(&self) -> &str;
    fn init(&mut self, _manager: &mut DarkstarManager) {}
    fn render_tab(&mut self, ui: &mut egui::Ui, manager: &mut DarkstarManager, l10n: &L10n);
    fn handle_event(&mut self, _event: &DarkstarEvent, _manager: &mut DarkstarManager) {}
}

pub struct PluginManager {
    pub plugins: Vec<Box<dyn DarkstarPlugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    /// Register a built-in plugin (bundled with the app)
    pub fn register_internal(&mut self, plugin: Box<dyn DarkstarPlugin>, manager: &mut DarkstarManager) {
        let mut p = plugin;
        p.init(manager);
        self.plugins.push(p);
    }

    /// Scans a directory for external plugins (.so, .dll, .dylib)
    /// This is where the "Protégé-like" magic happens.
    pub fn discover_external(&mut self, _plugins_dir: &Path, _manager: &mut DarkstarManager) {
        // Implementation for dynamic loading using libloading would go here.
        // For now, this ensures main.rs doesn't need to know about specific plugins.
        if !_plugins_dir.exists() {
            let _ = std::fs::create_dir_all(_plugins_dir);
        }
        
        // TODO: Iterate over files and load using libloading
        // let lib = libloading::Library::new(path)?;
        // let constructor: Symbol<fn() -> Box<dyn DarkstarPlugin>> = lib.get(b"create_plugin")?;
        // self.register_internal(constructor(), manager);
    }
}
