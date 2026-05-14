use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::core::settings::Language;

impl DarkstarApp {
    pub fn render_plugin_guide(&mut self, ctx: &egui::Context) {
        if !self.show_plugin_manual { return; }

        let mut open = self.show_plugin_manual;
        let i = self.i18n();
        
        egui::Window::new(i.plugin_manual)
            .open(&mut open)
            .default_size([600.0, 500.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.plugin_manual_lang, Language::Korean, "🇰🇷 한국어");
                    ui.selectable_value(&mut self.plugin_manual_lang, Language::English, "🇺🇸 English");
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.plugin_manual_lang {
                        Language::Korean => self.render_korean_guide(ui),
                        Language::English => self.render_english_guide(ui),
                    }
                });
            });
        
        self.show_plugin_manual = open;
    }

    fn render_korean_guide(&self, ui: &mut egui::Ui) {
        ui.heading("🚀 Darkstar 플러그인 개발 가이드");
        ui.add_space(10.0);
        
        ui.label("Darkstar는 Rust의 트레이트 시스템을 사용하여 플러그인을 확장합니다. 모든 플러그인은 `DarkstarPlugin` 트레이트를 구현해야 합니다.");
        
        ui.collapsing("1. 기본 구조체 정의", |ui| {
            ui.label("플러그인의 상태를 저장할 구조체를 정의합니다.");
            ui.code("pub struct MyPlugin { \n    pub counter: i32, \n}");
        });

        ui.collapsing("2. DarkstarPlugin 트레이트 구현", |ui| {
            ui.label("핵심 메소드들을 구현합니다.");
            ui.code(r#"impl DarkstarPlugin for MyPlugin {
    fn name(&self, _l10n: &L10n) -> String { "내 플러그인".to_string() }
    fn version(&self) -> &str { "0.1.0" }
    
    fn render_tab(&mut self, ui: &mut egui::Ui, manager: &mut DarkstarManager, l10n: &L10n) {
        ui.heading("플러그인 화면");
        if ui.button("트리플 추가").clicked() {
            manager.add_assertion("i:Subject".into(), "i:predicate".into(), "i:Object".into());
        }
    }
}"#);
        });

        ui.collapsing("3. 플러그인 등록", |ui| {
            ui.label("`src/plugins/mod.rs` 파일의 `load_all` 함수에 플러그인을 추가합니다.");
            ui.code("plugin_manager.register_internal(Box::new(MyPlugin::default()), manager);");
        });

        ui.add_space(20.0);
        ui.label("💡 팁: `handle_event`를 구현하여 온톨로지 변경 사항을 실시간으로 감지할 수 있습니다.");
    }

    fn render_english_guide(&self, ui: &mut egui::Ui) {
        ui.heading("🚀 Darkstar Plugin Development Guide");
        ui.add_space(10.0);
        
        ui.label("Darkstar uses Rust's trait system for extensibility. All plugins must implement the `DarkstarPlugin` trait.");
        
        ui.collapsing("1. Define Plugin Struct", |ui| {
            ui.label("Create a struct to hold your plugin's state.");
            ui.code("pub struct MyPlugin { \n    pub counter: i32, \n}");
        });

        ui.collapsing("2. Implement DarkstarPlugin Trait", |ui| {
            ui.label("Implement the required methods.");
            ui.code(r#"impl DarkstarPlugin for MyPlugin {
    fn name(&self, _l10n: &L10n) -> String { "My Plugin".to_string() }
    fn version(&self) -> &str { "0.1.0" }
    
    fn render_tab(&mut self, ui: &mut egui::Ui, manager: &mut DarkstarManager, l10n: &L10n) {
        ui.heading("Plugin View");
        if ui.button("Add Triple").clicked() {
            manager.add_assertion("i:Subject".into(), "i:predicate".into(), "i:Object".into());
        }
    }
}"#);
        });

        ui.collapsing("3. Register Your Plugin", |ui| {
            ui.label("Add your plugin to the `load_all` function in `src/plugins/mod.rs`.");
            ui.code("plugin_manager.register_internal(Box::new(MyPlugin::default()), manager);");
        });

        ui.add_space(20.0);
        ui.label("💡 Tip: Implement `handle_event` to listen for real-time ontology changes.");
    }
}
