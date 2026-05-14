use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use directories::ProjectDirs;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Korean,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub theme_dark: bool,
    pub ui_scale: f32,
    pub recent_files: Vec<PathBuf>,
    pub enabled_rules: ReasonerRules,
    pub language: Language,
    pub is_first_run: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ReasonerRules {
    pub equality: bool,
    pub property_chains: bool,
    pub class_expressions: bool,
    pub restrictions: bool,
    pub datatypes: bool,
    pub schema: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_dark: true,
            ui_scale: 1.0,
            recent_files: Vec::new(),
            enabled_rules: ReasonerRules::default(),
            language: Language::English,
            is_first_run: true,
        }
    }
}

impl Default for ReasonerRules {
    fn default() -> Self {
        Self {
            equality: true,
            property_chains: true,
            class_expressions: true,
            restrictions: true,
            datatypes: true,
            schema: true,
        }
    }
}

impl AppSettings {
    pub fn load() -> Self {
        if let Some(proj_dirs) = ProjectDirs::from("com", "darkstar", "ontology") {
            let config_dir = proj_dirs.config_dir();
            let config_file = config_dir.join("settings.json");
            if let Ok(data) = std::fs::read_to_string(config_file) {
                if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&data) {
                    settings.is_first_run = false;
                    return settings;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(proj_dirs) = ProjectDirs::from("com", "darkstar", "ontology") {
            let config_dir = proj_dirs.config_dir();
            let _ = std::fs::create_dir_all(config_dir);
            let config_file = config_dir.join("settings.json");
            if let Ok(data) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(config_file, data);
            }
        }
    }

    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        if self.recent_files.len() > 5 {
            self.recent_files.truncate(5);
        }
        self.save();
    }
}
