pub mod app;
pub mod dock;
pub mod tabs;
pub mod components;

use eframe::egui;
use egui::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Splash,
    Onboarding,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    ForceDirected,
    Hierarchical,
    Radial,
    Grid,
    Circular,
    Concentric,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeType {
    Class,
    Property,
    Individual,
    Literal,
    Blank,
}

pub struct GraphNode {
    pub pos: Vec2,
    pub vel: Vec2,
    pub label: String,
    pub node_type: NodeType,
}

pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub is_inferred: bool,
}

pub struct GraphFilters {
    pub show_individuals: bool,
    pub show_schema: bool,
    pub show_inferred: bool,
    pub show_literals: bool,
    pub layout_mode: LayoutMode,
    pub focus_mode: bool,
    pub expansion_depth: usize,
    pub show_system: bool,
    pub spacing_multiplier: f32,
}

impl Default for GraphFilters {
    fn default() -> Self {
        Self {
            show_individuals: true,
            show_schema: true,
            show_inferred: true,
            show_literals: true,
            layout_mode: LayoutMode::ForceDirected,
            focus_mode: true,
            expansion_depth: 1,
            show_system: false,
            spacing_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DarkstarTab {
    Graph,
    Classes,
    ObjectProperties,
    DataProperties,
    Individuals,
    History,
    EntityEditor,
    SourceEditor,
    Plugin(String),
}
