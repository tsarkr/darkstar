# Darkstar External Plugin Development Guide

Darkstar supports dynamic external plugins built as dynamic libraries (`.dylib` on macOS, `.so` on Linux, `.dll` on Windows).

## Plugin Requirements

1. **Crate Type**: Set `crate-type = ["cdylib"]` in your plugin's `Cargo.toml`.
2. **Implement Trait**: Implement `DarkstarPlugin`.
3. **Export Symbol**: Export `_darkstar_create_plugin` (or `create_plugin`) returning `Box<dyn DarkstarPlugin>`.

---

## Sample Plugin Project Setup

### 1. `Cargo.toml`
```toml
[package]
name = "sample_darkstar_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Depend on darkstar or share the same DarkstarPlugin trait definitions
darkstar = { path = "../darkstar" } # or include Darkstar as a dependency
eframe = "0.29"
```

### 2. `src/lib.rs`
```rust
use darkstar::core::plugin::DarkstarPlugin;
use darkstar::core::manager::DarkstarManager;
use darkstar::core::history::DarkstarEvent;
use darkstar::core::l10n::L10n;
use eframe::egui;

pub struct CustomSamplePlugin {
    click_count: usize,
}

impl CustomSamplePlugin {
    pub fn new() -> Self {
        Self { click_count: 0 }
    }
}

impl DarkstarPlugin for CustomSamplePlugin {
    fn name(&self, _l10n: &L10n) -> String {
        "Sample External Plugin".to_string()
    }

    fn description(&self, _l10n: &L10n) -> String {
        "A dynamically loaded sample plugin for Darkstar.".to_string()
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn render_tab(&mut self, ui: &mut egui::Ui, _manager: &mut DarkstarManager, _l10n: &L10n) {
        ui.heading("🔌 Sample External Plugin");
        ui.label("This plugin was loaded dynamically from a .dylib/.so/.dll library!");
        ui.separator();

        if ui.button(format!("Click count: {}", self.click_count)).clicked() {
            self.click_count += 1;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn _darkstar_create_plugin() -> Box<dyn DarkstarPlugin> {
    Box::new(CustomSamplePlugin::new())
}
```

---

## Installation & Deployment

Build your plugin:
```bash
cargo build --release
```

Copy the generated shared library to either:
1. Local project directory: `./plugins/`
2. System config directory: `~/Library/Application Support/com.darkstar.ontology/plugins/` (macOS) or `%APPDATA%\com.darkstar.ontology\plugins\` (Windows)

When Darkstar launches, it will automatically discover and present your plugin tab!
