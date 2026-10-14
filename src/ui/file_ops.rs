//! File saving and small UI-wide helpers shared by the menu bar and dialogs.

use std::path::{Path, PathBuf};
use crate::core::io::OntologyFormat;
use crate::core::settings::Language;
use crate::ui::app::DarkstarApp;

impl DarkstarApp {
    pub fn is_korean(&self) -> bool {
        self.settings.language == Language::Korean
    }

    /// Shows a transient message in the status bar.
    pub fn notify(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), std::time::Instant::now()));
    }

    /// Save dialog offering every writable format; the chosen extension decides the format.
    pub fn save_dialog(default_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_file_name(default_name)
            .add_filter("Turtle (.ttl)", &["ttl"])
            .add_filter("RDF/XML (.owl, .rdf)", &["owl", "rdf"])
            .add_filter("N-Triples (.nt)", &["nt"])
            .save_file()
    }

    /// Writes the ontology to `path`; reports success or failure in the status bar.
    fn save_to(&mut self, path: &Path, format: OntologyFormat) -> bool {
        let is_korean = self.is_korean();
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        match self.manager.save_to_file(path, format, false) {
            Ok(()) => {
                self.settings.add_recent(path.to_path_buf());
                self.notify(if is_korean { format!("'{}'에 저장되었습니다 ({}).", name, format.label()) } else { format!("Saved to '{}' ({}).", name, format.label()) });
                true
            }
            Err(e) => {
                eprintln!("Save error: {}", e);
                self.notify(if is_korean { format!("저장 실패 '{}': {}", name, e) } else { format!("Failed to save '{}': {}", name, e) });
                false
            }
        }
    }

    /// "Save": writes back to the opened file in its original format, or asks for a path.
    pub fn save_current(&mut self) -> bool {
        match self.manager.active_file_path.clone() {
            Some(path) => {
                let format = self.manager.active_format;
                self.save_to(&path, format)
            }
            None => self.save_as(),
        }
    }

    /// "Save As": asks for a path; the extension decides the format.
    pub fn save_as(&mut self) -> bool {
        match Self::save_dialog("ontology.ttl") {
            Some(path) => self.save_to(&path, OntologyFormat::from_path(&path)),
            None => false,
        }
    }

    /// Exports asserted + inferred triples to a separate file (the working file is unchanged).
    pub fn export_inferred(&mut self) {
        let Some(path) = Self::save_dialog("inferred.ttl") else { return };
        let is_korean = self.is_korean();
        match self.manager.save_to_file(&path, OntologyFormat::from_path(&path), true) {
            Ok(()) => self.notify(if is_korean { format!("추론 결과를 '{}'로 내보냈습니다.", path.display()) } else { format!("Exported inferences to '{}'.", path.display()) }),
            Err(e) => {
                eprintln!("Export error: {}", e);
                self.notify(if is_korean { format!("내보내기 실패: {}", e) } else { format!("Export failed: {}", e) });
            }
        }
    }
}
