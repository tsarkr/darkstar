use eframe::egui;
use egui::Vec2;
use std::collections::HashMap;
use crate::ui::app::DarkstarApp;
use crate::ui::{LayoutMode, NodeType};

impl DarkstarApp {
    pub fn render_graph_tab(&mut self, ui: &mut egui::Ui) {
        let i = self.i18n();
        
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{}:", i.layout)).size(11.0));
                let old_layout = self.filters.layout_mode;
                egui::ComboBox::from_id_salt("layout_mode")
                    .selected_text(egui::RichText::new(format!("{:?}", self.filters.layout_mode)).size(11.0))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.filters.layout_mode, LayoutMode::ForceDirected, "Force Directed");
                        ui.selectable_value(&mut self.filters.layout_mode, LayoutMode::Hierarchical, "Hierarchical");
                        ui.selectable_value(&mut self.filters.layout_mode, LayoutMode::Radial, "Radial");
                        ui.selectable_value(&mut self.filters.layout_mode, LayoutMode::Grid, "Grid");
                        ui.selectable_value(&mut self.filters.layout_mode, LayoutMode::Circular, "Circular");
                        ui.selectable_value(&mut self.filters.layout_mode, LayoutMode::Concentric, "Concentric");
                    });
                if self.filters.layout_mode != old_layout {
                    self.graph_needs_sync = true;
                    if self.filters.layout_mode == LayoutMode::ForceDirected { self.simulation_alpha = 20.0; }
                }
                
                ui.separator();
                ui.label(egui::RichText::new(format!("{}:", i.graph_spacing)).size(11.0));
                if ui.add_sized([100.0, 20.0], egui::Slider::new(&mut self.filters.spacing_multiplier, 0.5..=3.0)).changed() {
                     self.simulation_alpha = 20.0;
                }
                // Correct way to detect change for simulation_alpha
                
                ui.separator();
                if ui.button(egui::RichText::new("⛶").size(11.0)).on_hover_text(i.fit_screen).clicked() { self.trigger_fit = true; }
                if ui.button(egui::RichText::new("⟲").size(11.0)).on_hover_text(i.sync_reasoner).clicked() { self.graph_needs_sync = true; }
                ui.separator();
                if ui.button(egui::RichText::new("📷").size(11.0)).on_hover_text(i.screenshot).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot);
                }
            });

            ui.horizontal(|ui| {
                let mut changed = false;
                changed |= ui.checkbox(&mut self.filters.show_individuals, egui::RichText::new(i.individuals).size(11.0)).changed();
                changed |= ui.checkbox(&mut self.filters.show_schema, egui::RichText::new("Schema").size(11.0)).changed();
                changed |= ui.checkbox(&mut self.filters.show_inferred, egui::RichText::new(i.show_inferred).size(11.0)).changed();
                changed |= ui.checkbox(&mut self.filters.show_system, egui::RichText::new(i.show_system).size(11.0)).changed();
                if changed { 
                    self.graph_needs_sync = true; 
                    self.simulation_alpha = 20.0;
                }
                
                ui.separator();
                if ui.checkbox(&mut self.filters.focus_mode, egui::RichText::new(i.focus_mode).size(11.0)).changed() { 
                    self.graph_needs_sync = true; 
                    self.simulation_alpha = 20.0;
                }

                if self.filters.focus_mode {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("{}:", i.expansion)).size(11.0));
                    let old_depth = self.filters.expansion_depth;
                    ui.add_sized([80.0, 20.0], egui::Slider::new(&mut self.filters.expansion_depth, 1..=5));
                    if self.filters.expansion_depth != old_depth { 
                        self.graph_needs_sync = true; 
                        self.simulation_alpha = 20.0; 
                    }
                }
            });
        });
        ui.separator();

        if self.filters.focus_mode && self.selected_uri.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(i.select_entity).size(11.0).weak());
            });
        } else {
            if self.graph_needs_sync {
                self.sync_graph();
            }
            
            if self.nodes.len() > 120 {
                let mut frame = egui::Frame::group(ui.style());
                frame.fill = if self.settings.theme_dark { egui::Color32::from_rgb(45, 35, 15) } else { egui::Color32::from_rgb(255, 245, 220) };
                frame.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 140, 0));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("⚠️").size(14.0));
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(format!(
                                "그래프 개체 수(현재 {}개)가 많아 로딩 및 연산 속도가 느려질 수 있습니다. 개체 수를 줄이기 위한 아래 설정을 제안합니다:",
                                self.nodes.len()
                            )).size(11.0).strong());
                            
                            ui.horizontal(|ui| {
                                if !self.filters.focus_mode {
                                    if ui.button("🎯 포커스 모드 활성화").on_hover_text("선택한 개체 주변만 집중 탐색하여 렌더링할 개체 수를 크게 줄입니다.").clicked() {
                                        self.filters.focus_mode = true;
                                        self.graph_needs_sync = true;
                                        self.simulation_alpha = 20.0;
                                    }
                                } else if self.filters.expansion_depth > 1 {
                                    if ui.button("🔍 탐색 깊이 축소 (1단계)").on_hover_text("탐색 반경을 1단계로 줄여 주변 개체 수를 최소화합니다.").clicked() {
                                        self.filters.expansion_depth = 1;
                                        self.graph_needs_sync = true;
                                        self.simulation_alpha = 20.0;
                                    }
                                }
                                
                                if self.filters.show_schema {
                                    if ui.button("🏢 클래스/프로퍼티 숨기기").on_hover_text("클래스와 프로퍼티 노드를 숨기고 개별 인디비주얼만 표시합니다.").clicked() {
                                        self.filters.show_schema = false;
                                        self.graph_needs_sync = true;
                                        self.simulation_alpha = 20.0;
                                    }
                                }
                                
                                if self.filters.show_individuals && self.nodes.values().any(|n| n.node_type == NodeType::Individual) {
                                    if ui.button("👥 인디비주얼 숨기기").on_hover_text("개별 인스턴스(인디비주얼) 노드를 숨기고 스키마 중심 구조만 표시합니다.").clicked() {
                                        self.filters.show_individuals = false;
                                        self.graph_needs_sync = true;
                                        self.simulation_alpha = 20.0;
                                    }
                                }
                            });
                        });
                    });
                });
            }

            self.render_custom_graph(ui);
        }
    }

    pub fn render_custom_graph(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none().inner_margin(0.0).show(ui, |ui| {
            let (response, mut painter) = ui.allocate_painter(ui.available_size(), egui::Sense::drag().union(egui::Sense::click()));
            let rect = response.rect;
            self.last_graph_rect = Some(rect);
            painter.set_clip_rect(rect);

        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta != 0.0 {
                let zoom_factor = (zoom_delta * 0.002).exp();
                let old_scale = self.graph_scale;
                self.graph_scale = (self.graph_scale * zoom_factor).clamp(0.05, 10.0);
                if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let local_mouse = mouse_pos - rect.min;
                    let graph_mouse = (local_mouse - self.graph_offset) / old_scale;
                    self.graph_offset = local_mouse - graph_mouse * self.graph_scale;
                }
            }
        }

        if response.dragged() && self.dragging_node.is_none() { self.graph_offset += response.drag_delta(); }

        let to_screen_pos = |p: Vec2, offset: Vec2, scale: f32| -> egui::Pos2 { rect.min + offset + p * scale };

        // Auto-Fit Logic
        if !self.nodes.is_empty() && (self.trigger_fit || self.graph_needs_sync) {
            let mut min = Vec2::new(f32::MAX, f32::MAX);
            let mut max = Vec2::new(f32::MIN, f32::MIN);
            for node in self.nodes.values() {
                min.x = min.x.min(node.pos.x); min.y = min.y.min(node.pos.y);
                max.x = max.x.max(node.pos.x); max.y = max.y.max(node.pos.y);
            }
            let size = max - min;
            let center = (min + max) * 0.5;
            if size.x > 0.0 && size.y > 0.0 {
                let padding = 100.0;
                let scale_x = (rect.width() - padding) / size.x;
                let scale_y = (rect.height() - padding) / size.y;
                self.graph_scale = scale_x.min(scale_y).clamp(0.1, 2.0);
                self.graph_offset = (rect.size() * 0.5) - (center * self.graph_scale);
            }
            self.trigger_fit = false;
        }

        // Physics Update (Force-Directed)
        if self.filters.layout_mode == LayoutMode::ForceDirected && self.simulation_alpha > 0.01 {
            let mut keys: Vec<String> = Vec::with_capacity(self.nodes.len());
            let mut positions: Vec<Vec2> = Vec::with_capacity(self.nodes.len());
            let mut velocities: Vec<Vec2> = Vec::with_capacity(self.nodes.len());
            let mut forces: Vec<Vec2> = vec![Vec2::ZERO; self.nodes.len()];
            
            // Map string key to index in the vectors
            let mut key_to_idx: HashMap<String, usize> = HashMap::with_capacity(self.nodes.len());
            
            for (idx, (key, node)) in self.nodes.iter().enumerate() {
                keys.push(key.clone());
                positions.push(node.pos);
                velocities.push(node.vel);
                key_to_idx.insert(key.clone(), idx);
            }

            let len = keys.len();
            for i in 0..len {
                for j in i+1..len {
                    let diff = positions[i] - positions[j];
                    let dist_sq = diff.length_sq().max(1000.0);
                    let dist = dist_sq.sqrt();
                    let force_mag = (75000.0 * self.filters.spacing_multiplier * self.simulation_alpha) / (dist_sq * dist);
                    let force = diff * force_mag;
                    forces[i] += force;
                    forces[j] -= force;
                }
            }

            let mut center = Vec2::ZERO;
            if !positions.is_empty() {
                for pos in &positions { center += *pos; }
                center /= positions.len() as f32;
            }

            for i in 0..len {
                let diff = center - positions[i];
                let gravity = diff * 0.15; 
                forces[i] += gravity;
            }

            for edge in &self.edges {
                if let (Some(&idx1), Some(&idx2)) = (key_to_idx.get(&edge.from), key_to_idx.get(&edge.to)) {
                    let diff = positions[idx1] - positions[idx2];
                    let dist = diff.length().max(1.0);
                    let force_mag = (dist - 250.0 * self.filters.spacing_multiplier) * -0.25 * self.simulation_alpha.sqrt() / dist;
                    let force = diff * force_mag;
                    forces[idx1] += force;
                    forces[idx2] -= force;
                }
            }

            for i in 0..len {
                let key = &keys[i];
                let mut vel = velocities[i] + forces[i];
                vel *= 0.6;
                let mut pos = positions[i];
                if self.dragging_node.as_ref() != Some(key) {
                    pos += vel * 0.1;
                }
                
                if let Some(node) = self.nodes.get_mut(key) {
                    node.vel = vel;
                    node.pos = pos;
                }
            }

            self.simulation_alpha *= 0.95;
            ui.ctx().request_repaint();
        } else if self.filters.layout_mode == LayoutMode::ForceDirected && self.simulation_alpha > 0.0 {
            // Cool down complete, stop repaint
            self.simulation_alpha = 0.0;
            for node in self.nodes.values_mut() {
                node.vel = Vec2::ZERO;
            }
        }

        // Interaction
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let graph_pointer = (pointer_pos - rect.min - self.graph_offset) / self.graph_scale;
            if response.drag_started() {
                for (uri, node) in &self.nodes {
                    if (node.pos - graph_pointer).length() < (30.0 / self.graph_scale) { self.dragging_node = Some(uri.clone()); break; }
                }
            }
            if response.double_clicked() {
                for (uri, node) in &self.nodes {
                    if (node.pos - graph_pointer).length() < (30.0 / self.graph_scale) {
                        if self.expanded_nodes.contains(uri) {
                            self.expanded_nodes.remove(uri);
                        } else {
                            self.expanded_nodes.insert(uri.clone());
                        }
                        self.graph_needs_sync = true;
                        self.simulation_alpha = 20.0;
                        break;
                    }
                }
            } else if response.clicked() {
                for (uri, node) in &self.nodes {
                    if (node.pos - graph_pointer).length() < (30.0 / self.graph_scale) { 
                        self.selected_uri = Some(uri.clone()); 
                        break; 
                    }
                }
            }
        }
        if response.drag_stopped() { self.dragging_node = None; }
        if let Some(uri) = &self.dragging_node {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let graph_pointer = (pointer_pos - rect.min - self.graph_offset) / self.graph_scale;
                if let Some(node) = self.nodes.get_mut(uri) { 
                    node.pos = graph_pointer; 
                    self.simulation_alpha = self.simulation_alpha.max(5.0);
                    ui.ctx().request_repaint();
                }
            }
        }

        // --- LAYERED DRAWING ---

        // 1. Draw Edges and Assertion Labels
        let edge_groups = self.group_edges();
        for ((u_uri, v_uri), group) in edge_groups {
            if let (Some(u), Some(v)) = (self.nodes.get(&u_uri), self.nodes.get(&v_uri)) {
                let pu = to_screen_pos(u.pos, self.graph_offset, self.graph_scale);
                let pv = to_screen_pos(v.pos, self.graph_offset, self.graph_scale);
                
                let radius = 20.0 * self.graph_scale;
                let uv_dir = (pv - pu).normalized();
                let perp = uv_dir.rot90();

                let mut to_display = Vec::new();
                let mut processed = vec![false; group.len()];
                for i in 0..group.len() {
                    if processed[i] { continue; }
                    let edge = group[i];
                    let mut reverse_idx = None;
                    for j in (i + 1)..group.len() {
                        if !processed[j] 
                           && group[j].from == edge.to 
                           && group[j].to == edge.from 
                           && group[j].label == edge.label 
                           && group[j].is_inferred == edge.is_inferred 
                        {
                            reverse_idx = Some(j);
                            break;
                        }
                    }
                    if let Some(ridx) = reverse_idx { processed[ridx] = true; }
                    processed[i] = true;
                    to_display.push((edge, reverse_idx.is_some()));
                }

                for (idx, (edge, is_bidirectional)) in to_display.iter().enumerate() {
                    let (p_start_orig, p_end_orig, current_dir) = if edge.from == u_uri {
                        (pu, pv, uv_dir)
                    } else {
                        (pv, pu, -uv_dir)
                    };

                    let start_node = self.nodes.get(&edge.from).unwrap();
                    let end_node = self.nodes.get(&edge.to).unwrap();
                    
                    let get_boundary_offset = |dir: Vec2, node_type: NodeType| -> f32 {
                        match node_type {
                            NodeType::Individual => radius / (dir.x.abs() + dir.y.abs()).max(0.01),
                            NodeType::Property | NodeType::Literal => {
                                let aspect = 1.5;
                                let x_limit = radius * aspect;
                                let y_limit = radius * 0.7;
                                let tx = x_limit / dir.x.abs().max(0.01);
                                let ty = y_limit / dir.y.abs().max(0.01);
                                tx.min(ty)
                            }
                            _ => radius
                        }
                    };

                    let d1 = get_boundary_offset(current_dir, start_node.node_type);
                    let d2 = get_boundary_offset(-current_dir, end_node.node_type);

                    let p_start = p_start_orig + current_dir * d1;
                    let p_end = p_end_orig - current_dir * d2;

                    let color = if edge.is_inferred { 
                        if self.settings.theme_dark { egui::Color32::from_rgb(100, 200, 100) } else { egui::Color32::from_rgb(0, 150, 0) }
                    } else { 
                        if self.settings.theme_dark { egui::Color32::from_gray(160) } else { egui::Color32::from_gray(60) }
                    };
                    let alpha = if edge.is_inferred { 180 } else { 255 };
                    let stroke = egui::Stroke::new(1.5 * self.graph_scale, color.linear_multiply(alpha as f32 / 255.0));
                    
                    let offset_mag = (idx as f32 - (to_display.len() as f32 - 1.0) / 2.0) * 20.0 * self.graph_scale;
                    let p1_off = p_start + perp * offset_mag;
                    let p2_off = p_end + perp * offset_mag;

                    painter.line_segment([p1_off, p2_off], stroke);
                    
                    let head_size = 10.0 * self.graph_scale;
                    let head_dir = current_dir;
                    let head_perp = head_dir.rot90();

                    // Arrowhead at end
                    let head_end = p2_off;
                    painter.line_segment([head_end, head_end - (head_dir + head_perp * 0.6).normalized() * head_size], stroke);
                    painter.line_segment([head_end, head_end - (head_dir - head_perp * 0.6).normalized() * head_size], stroke);

                    // Arrowhead at start for bidirectional
                    if *is_bidirectional {
                        let head_start = p1_off;
                        painter.line_segment([head_start, head_start + (head_dir + head_perp * 0.6).normalized() * head_size], stroke);
                        painter.line_segment([head_start, head_start + (head_dir - head_perp * 0.6).normalized() * head_size], stroke);
                    }

                    // --- IMPROVED ASSERTION LABEL PLACEMENT ---
                    if self.graph_scale > 0.6 {
                        let mid = p1_off + (p2_off - p1_off) * 0.5;
                        let font = egui::FontId::proportional(9.0 * self.graph_scale);
                        
                        let text_pos = mid + perp * (if offset_mag >= 0.0 { 10.0 } else { -10.0 } * self.graph_scale);
                        
                        let dist_to_start = (text_pos - p_start_orig).length();
                        let dist_to_end = (text_pos - p_end_orig).length();
                        let safe_dist = radius * 1.5;

                        if dist_to_start > safe_dist && dist_to_end > safe_dist {
                            let label = edge.label.clone();
                            painter.text(text_pos, egui::Align2::CENTER_CENTER, &label, font, if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::BLACK });
                        }
                    }
                }
            }
        }

        // 2. Draw Node Shapes
        for (uri, node) in &self.nodes {
            let pos = to_screen_pos(node.pos, self.graph_offset, self.graph_scale);
            let is_selected = self.selected_uri.as_deref() == Some(uri);
            let matches_search = !self.search_query.is_empty() && 
                (node.label.to_lowercase().contains(&self.search_query.to_lowercase()) || 
                 uri.to_lowercase().contains(&self.search_query.to_lowercase()));
            
            let radius = 20.0 * self.graph_scale;
            let is_expanded = self.expanded_nodes.contains(uri);
            if is_expanded {
                painter.circle(pos, radius + 5.0 * self.graph_scale, egui::Color32::TRANSPARENT, egui::Stroke::new(1.5 * self.graph_scale, egui::Color32::from_rgb(100, 200, 255)));
            }

            let stroke = egui::Stroke::new(
                if matches_search { 4.0 } else if is_selected { 3.0 } else { 1.0 } * self.graph_scale, 
                if matches_search { egui::Color32::from_rgb(255, 165, 0) } else if is_selected { egui::Color32::WHITE } else { egui::Color32::BLACK }
            );

            match node.node_type {
                NodeType::Class => { painter.circle(pos, radius, if self.settings.theme_dark { egui::Color32::from_rgb(249, 232, 88) } else { egui::Color32::from_rgb(255, 215, 0) }, stroke); },
                NodeType::Property => { painter.rect(egui::Rect::from_center_size(pos, Vec2::new(radius * 3.0, radius * 1.5)), 5.0 * self.graph_scale, if self.settings.theme_dark { egui::Color32::from_rgb(170, 204, 255) } else { egui::Color32::from_rgb(100, 149, 237) }, stroke); },
                NodeType::Individual => {
                    let p1 = pos + Vec2::new(0.0, -radius); let p2 = pos + Vec2::new(radius, 0.0);
                    let p3 = pos + Vec2::new(0.0, radius); let p4 = pos + Vec2::new(-radius, 0.0);
                    painter.add(egui::Shape::convex_polygon(vec![p1, p2, p3, p4], if self.settings.theme_dark { egui::Color32::from_rgb(194, 174, 255) } else { egui::Color32::from_rgb(138, 43, 226) }, stroke));
                }
                NodeType::Literal => { painter.rect(egui::Rect::from_center_size(pos, Vec2::new(radius * 2.5, radius * 1.2)), 0.0, if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::from_gray(240) }, stroke); },
                NodeType::Blank => { painter.circle(pos, radius, egui::Color32::GRAY, stroke); },
            }
        }

        // 3. Draw Node Labels (TOP LAYER with Solid Background)
        for node in self.nodes.values() {
            let pos = to_screen_pos(node.pos, self.graph_offset, self.graph_scale);
            let radius = 20.0 * self.graph_scale;
            if self.graph_scale > 0.5 {
                let font = egui::FontId::proportional(11.0 * self.graph_scale);
                let text_pos = pos + Vec2::new(0.0, radius + 10.0 * self.graph_scale);
                
                // Removed background rect for transparency
                painter.text(text_pos, egui::Align2::CENTER_TOP, &node.label, font, if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::BLACK });
            }
        }
        });
    }

    fn group_edges(&self) -> HashMap<(String, String), Vec<&crate::ui::GraphEdge>> {
        let mut groups = HashMap::new();
        for edge in &self.edges {
            let pair = if edge.from < edge.to { (edge.from.clone(), edge.to.clone()) } else { (edge.to.clone(), edge.from.clone()) };
            groups.entry(pair).or_insert(Vec::new()).push(edge);
        }
        groups
    }
}
