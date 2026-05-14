use eframe::egui;
use crate::ui::app::DarkstarApp;
use crate::ui::AppState;
use crate::core::settings::Language;

impl DarkstarApp {
    pub fn render_splash(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(200.0);
                ui.heading(egui::RichText::new("DARKSTAR").size(60.0).strong().extra_letter_spacing(10.0));
                ui.label(egui::RichText::new("Professional Ontology Engineering Platform").weak());
                ui.add_space(20.0);
                ui.add(egui::Spinner::new().size(32.0));
            });
        });
        if self.splash_start_time.elapsed().as_secs() >= 2 {
            self.state = if self.settings.is_first_run { AppState::Onboarding } else { AppState::Main };
        }
    }

    pub fn render_onboarding(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                match self.onboarding_step {
                    0 => {
                        ui.heading("Welcome to Darkstar");
                        ui.label("Select your preferred language:");
                        ui.horizontal(|ui| {
                            if ui.button("English").clicked() { self.settings.language = Language::English; self.onboarding_step += 1; }
                            if ui.button("한국어").clicked() { self.settings.language = Language::Korean; self.onboarding_step += 1; }
                        });
                    }
                    _ => {
                        self.settings.is_first_run = false;
                        self.settings.save();
                        self.state = AppState::Main;
                    }
                }
            });
        });
    }
}
