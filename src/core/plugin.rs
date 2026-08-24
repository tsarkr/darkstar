use crate::core::history::DarkstarEvent;
use crate::core::l10n::L10n;
use crate::core::manager::DarkstarManager;
use eframe::egui;
use std::ffi::OsStr;
use std::path::Path;

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
    pub libraries: Vec<libloading::Library>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            libraries: Vec::new(),
        }
    }

    /// Register a built-in plugin (bundled with the app)
    pub fn register_internal(&mut self, plugin: Box<dyn DarkstarPlugin>, manager: &mut DarkstarManager) {
        let mut p = plugin;
        p.init(manager);
        self.plugins.push(p);
    }

    /// Scans a directory for external plugins (.so, .dll, .dylib)
    /// This is where the "Protégé-like" magic happens.
    pub fn discover_external(&mut self, plugins_dir: &Path, manager: &mut DarkstarManager) {
        if !plugins_dir.exists() {
            let _ = std::fs::create_dir_all(plugins_dir);
            return;
        }

        let target_extension = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };

        let entries = match std::fs::read_dir(plugins_dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[PluginManager] Failed to read plugin directory {:?}: {}", plugins_dir, err);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(OsStr::new(target_extension)) {
                println!("[PluginManager] Attempting to load dynamic plugin: {:?}", path);
                unsafe {
                    match libloading::Library::new(&path) {
                        Ok(lib) => {
                            let plugin_box: Option<Box<dyn DarkstarPlugin>> = match lib
                                .get::<unsafe extern "C" fn() -> Box<dyn DarkstarPlugin>>(b"_darkstar_create_plugin")
                            {
                                Ok(symbol) => Some(symbol()),
                                Err(_) => match lib
                                    .get::<unsafe extern "C" fn() -> Box<dyn DarkstarPlugin>>(b"create_plugin")
                                {
                                    Ok(symbol) => Some(symbol()),
                                    Err(_) => match lib
                                        .get::<unsafe extern "C" fn() -> *mut dyn DarkstarPlugin>(b"_darkstar_create_plugin_raw")
                                    {
                                        Ok(symbol) => {
                                            let ptr = symbol();
                                            if !ptr.is_null() {
                                                Some(Box::from_raw(ptr))
                                            } else {
                                                None
                                            }
                                        }
                                        Err(_) => None,
                                    },
                                },
                            };

                            if let Some(mut plugin) = plugin_box {
                                println!("[PluginManager] Successfully loaded plugin: {:?}", path);
                                plugin.init(manager);
                                self.plugins.push(plugin);
                                self.libraries.push(lib);
                            } else {
                                eprintln!(
                                    "[PluginManager] Missing valid plugin creation symbol (_darkstar_create_plugin or create_plugin) in {:?}",
                                    path
                                );
                            }
                        }
                        Err(err) => {
                            eprintln!("[PluginManager] Failed to load library {:?}: {}", path, err);
                        }
                    }
                }
            }
        }
    }
}
