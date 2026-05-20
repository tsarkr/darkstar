use egui_dock::TabViewer;
use eframe::egui;
use crate::core::settings::Language;
use crate::core::l10n::L10n;
use crate::ui::DarkstarTab;
use crate::ui::app::DarkstarApp;

pub struct DarkstarTabViewer<'a> {
    pub app: &'a mut DarkstarApp,
}

impl<'a> TabViewer for DarkstarTabViewer<'a> {
    type Tab = DarkstarTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let i = self.app.i18n();
        match tab {
            DarkstarTab::Classes => i.classes.into(),
            DarkstarTab::ObjectProperties => i.obj_props.into(),
            DarkstarTab::DataProperties => i.data_props.into(),
            DarkstarTab::Individuals => i.individuals.into(),
            DarkstarTab::Graph => i.graph_view.into(),
            DarkstarTab::SourceEditor => i.source_editor.into(),
            DarkstarTab::EntityEditor => i.property_editor.into(),
            DarkstarTab::History => i.history.into(),
            DarkstarTab::Plugin(name) => {
                if let Some(plugin) = self.app.plugin_manager.plugins.iter().find(|p| p.name(&L10n::get(Language::English)) == *name) {
                    plugin.name(&i).into()
                } else {
                    name.clone().into()
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            DarkstarTab::Classes => self.app.render_classes_tab(ui),
            DarkstarTab::ObjectProperties => self.app.render_properties_tab(ui, true),
            DarkstarTab::DataProperties => self.app.render_properties_tab(ui, false),
            DarkstarTab::Individuals => self.app.render_individuals_tab(ui),
            DarkstarTab::Graph => {
                self.app.render_graph_tab(ui);
            }
            DarkstarTab::SourceEditor => {
                self.app.render_source_editor_tab(ui);
            }
            DarkstarTab::EntityEditor => {
                self.app.render_entity_editor_tab(ui);
            }
            DarkstarTab::History => {
                self.app.render_history_tab(ui);
            }
            DarkstarTab::Plugin(name) => {
                let i = self.app.i18n();
                if let Some(plugin) = self.app.plugin_manager.plugins.iter_mut().find(|p| p.name(&L10n::get(Language::English)) == *name) {
                    plugin.render_tab(ui, &mut self.app.manager, &i);
                } else {
                    ui.label("Plugin not found");
                }
            }
        }
    }
}
