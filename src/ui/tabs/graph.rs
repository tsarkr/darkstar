use eframe::egui;
use egui::Vec2;
use std::collections::{HashMap, HashSet};
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
                if ui.add_sized([100.0, 20.0], egui::Slider::new(&mut self.filters.spacing_multiplier, 0.3..=2.5).step_by(0.05)).changed() {
                     self.simulation_alpha = 20.0;
                }
                
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
            
            if self.nodes.len() > 150 {
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

            // --- 0. INTERACTIVE PAN & ZOOM ---
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

            if response.dragged() && self.dragging_node.is_none() { 
                self.graph_offset += response.drag_delta(); 
            }

            let to_screen_pos = |p: Vec2, offset: Vec2, scale: f32| -> egui::Pos2 { rect.min + offset + p * scale };

            // --- 1. BACKGROUND TECHNICAL GRID (Subtle dot matrix for fine precision) ---
            let base_grid_step = 25.0;
            let mut grid_spacing = base_grid_step * self.graph_scale;
            while grid_spacing < 14.0 {
                grid_spacing *= 2.0;
            }
            while grid_spacing > 50.0 {
                grid_spacing /= 2.0;
            }

            let start_x = rect.min.x + ((self.graph_offset.x % grid_spacing) + grid_spacing) % grid_spacing;
            let start_y = rect.min.y + ((self.graph_offset.y % grid_spacing) + grid_spacing) % grid_spacing;

            let dot_color = if self.settings.theme_dark {
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 16)
            } else {
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 18)
            };
            let dot_radius = (0.9 * self.graph_scale).clamp(0.7, 1.4);

            let mut gx = start_x;
            while gx <= rect.max.x {
                let mut gy = start_y;
                while gy <= rect.max.y {
                    painter.circle_filled(egui::Pos2::new(gx, gy), dot_radius, dot_color);
                    gy += grid_spacing;
                }
                gx += grid_spacing;
            }

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
                    let padding = 80.0;
                    let scale_x = (rect.width() - padding) / size.x;
                    let scale_y = (rect.height() - padding) / size.y;
                    self.graph_scale = scale_x.min(scale_y).clamp(0.1, 2.5);
                    self.graph_offset = (rect.size() * 0.5) - (center * self.graph_scale);
                }
                self.trigger_fit = false;
            }

            // --- 2. PHYSICS UPDATE (Force-Directed: Fine & Dense Tuning) ---
            if self.filters.layout_mode == LayoutMode::ForceDirected && self.simulation_alpha > 0.01 {
                let mut keys: Vec<String> = Vec::with_capacity(self.nodes.len());
                let mut positions: Vec<Vec2> = Vec::with_capacity(self.nodes.len());
                let mut velocities: Vec<Vec2> = Vec::with_capacity(self.nodes.len());
                let mut forces: Vec<Vec2> = vec![Vec2::ZERO; self.nodes.len()];
                
                let mut key_to_idx: HashMap<String, usize> = HashMap::with_capacity(self.nodes.len());
                
                for (idx, (key, node)) in self.nodes.iter().enumerate() {
                    keys.push(key.clone());
                    positions.push(node.pos);
                    velocities.push(node.vel);
                    key_to_idx.insert(key.clone(), idx);
                }

                let len = keys.len();
                // 1. Repulsive forces (Coulomb-like repulsion tuned for dense clustering)
                for i in 0..len {
                    for j in i+1..len {
                        let diff = positions[i] - positions[j];
                        let dist = diff.length().max(12.0);
                        let dist_sq = dist * dist;
                        let k_r = 28000.0 * self.filters.spacing_multiplier.powf(1.4);
                        let force_mag = k_r / (dist_sq * dist);
                        let force = diff * force_mag;
                        forces[i] += force;
                        forces[j] -= force;
                    }
                }

                // 2. Centering Gravity forces (pulls nodes cohesively towards center of mass)
                let mut center = Vec2::ZERO;
                if !positions.is_empty() {
                    for pos in &positions { center += *pos; }
                    center /= positions.len() as f32;
                }
                let k_g = 0.035;
                for i in 0..len {
                    let diff = center - positions[i];
                    let gravity = diff * k_g;
                    forces[i] += gravity;
                }

                // 3. Attractive forces (Hooke's Law springs with compact rest length)
                let rest_len = 80.0 * self.filters.spacing_multiplier;
                let k_a = 0.15;
                for edge in &self.edges {
                    if let (Some(&idx1), Some(&idx2)) = (key_to_idx.get(&edge.from), key_to_idx.get(&edge.to)) {
                        let diff = positions[idx1] - positions[idx2];
                        let dist = diff.length().max(1.0);
                        let force_mag = -k_a * (dist - rest_len) / dist;
                        let force = diff * force_mag;
                        forces[idx1] += force;
                        forces[idx2] -= force;
                    }
                }

                // 4. Update velocity and position with cooling factor
                for i in 0..len {
                    let key = &keys[i];
                    let force = forces[i] * self.simulation_alpha * 0.04;
                    let mut vel = (velocities[i] + force) * 0.72; // Damping
                    
                    let max_displacement = 35.0;
                    let vel_len = vel.length();
                    if vel_len > max_displacement {
                        vel = vel * (max_displacement / vel_len);
                    }

                    let mut pos = positions[i];
                    if self.dragging_node.as_ref() != Some(key) {
                        pos += vel;
                    }
                    
                    if let Some(node) = self.nodes.get_mut(key) {
                        node.vel = vel;
                        node.pos = pos;
                    }
                }

                self.resolve_collisions(3);

                self.simulation_alpha *= 0.95;
                ui.ctx().request_repaint();
            } else if self.filters.layout_mode == LayoutMode::ForceDirected && self.simulation_alpha > 0.0 {
                self.simulation_alpha = 0.0;
                for node in self.nodes.values_mut() {
                    node.vel = Vec2::ZERO;
                }
            }

            // --- 3. INTERACTIVE POINTER DETECTION ---
            let mut hovered_node_uri: Option<String> = None;
            if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pointer_pos) {
                    let graph_pointer = (pointer_pos - rect.min - self.graph_offset) / self.graph_scale;
                    let hit_radius = (20.0 / self.graph_scale).max(14.0);
                    for (uri, node) in &self.nodes {
                        if (node.pos - graph_pointer).length() <= hit_radius {
                            hovered_node_uri = Some(uri.clone());
                            break;
                        }
                    }
                }
            }

            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let graph_pointer = (pointer_pos - rect.min - self.graph_offset) / self.graph_scale;
                let hit_radius = (20.0 / self.graph_scale).max(14.0);
                if response.drag_started() {
                    for (uri, node) in &self.nodes {
                        if (node.pos - graph_pointer).length() < hit_radius { 
                            self.dragging_node = Some(uri.clone()); 
                            break; 
                        }
                    }
                }
                if response.double_clicked() {
                    for (uri, node) in &self.nodes {
                        if (node.pos - graph_pointer).length() < hit_radius {
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
                        if (node.pos - graph_pointer).length() < hit_radius { 
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

            // Determine active node for relationship highlighting
            let active_node_uri = hovered_node_uri.as_ref().or(self.selected_uri.as_ref());
            let mut connected_nodes: HashSet<&str> = HashSet::new();
            let mut connected_edges: HashSet<usize> = HashSet::new();
            if let Some(active) = active_node_uri {
                connected_nodes.insert(active.as_str());
                for (idx, edge) in self.edges.iter().enumerate() {
                    if edge.from == *active {
                        connected_nodes.insert(edge.to.as_str());
                        connected_edges.insert(idx);
                    } else if edge.to == *active {
                        connected_nodes.insert(edge.from.as_str());
                        connected_edges.insert(idx);
                    }
                }
            }

            let base_radius = 12.0 * self.graph_scale;

            // --- 4. LAYERED DRAWING ---

            // 4.1. Draw Edges and Badges
            let edge_groups = self.group_edges();
            for ((u_uri, v_uri), group) in edge_groups {
                if let (Some(u), Some(v)) = (self.nodes.get(&u_uri), self.nodes.get(&v_uri)) {
                    let pu = to_screen_pos(u.pos, self.graph_offset, self.graph_scale);
                    let pv = to_screen_pos(v.pos, self.graph_offset, self.graph_scale);
                    
                    let radius = base_radius;
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
                                    let aspect = 1.4;
                                    let x_limit = radius * aspect;
                                    let y_limit = radius * 0.75;
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

                        let is_connected_to_active = active_node_uri.map_or(false, |a| edge.from == *a || edge.to == *a);

                        let (color, alpha, width) = if edge.is_inferred { 
                            let base_col = if self.settings.theme_dark { egui::Color32::from_rgb(52, 211, 153) } else { egui::Color32::from_rgb(16, 185, 129) };
                            if active_node_uri.is_some() && !is_connected_to_active {
                                (base_col, 50, 0.9 * self.graph_scale)
                            } else if is_connected_to_active {
                                (base_col, 255, 1.8 * self.graph_scale)
                            } else {
                                (base_col, 190, 1.2 * self.graph_scale)
                            }
                        } else { 
                            let base_col = if is_connected_to_active {
                                if self.settings.theme_dark { egui::Color32::from_rgb(96, 165, 250) } else { egui::Color32::from_rgb(37, 99, 235) }
                            } else if self.settings.theme_dark { 
                                egui::Color32::from_rgb(148, 163, 184) 
                            } else { 
                                egui::Color32::from_rgb(100, 116, 139) 
                            };

                            if active_node_uri.is_some() && !is_connected_to_active {
                                (base_col, 40, 0.8 * self.graph_scale)
                            } else if is_connected_to_active {
                                (base_col, 255, 1.8 * self.graph_scale)
                            } else {
                                (base_col, 175, 1.1 * self.graph_scale)
                            }
                        };
                        
                        let stroke = egui::Stroke::new(width, color.linear_multiply(alpha as f32 / 255.0));
                        
                        let offset_mag = (idx as f32 - (to_display.len() as f32 - 1.0) / 2.0) * 12.0 * self.graph_scale;
                        let p1_off = p_start + perp * offset_mag;
                        let p2_off = p_end + perp * offset_mag;

                        painter.line_segment([p1_off, p2_off], stroke);
                        
                        let head_size = 6.5 * self.graph_scale;
                        let head_dir = current_dir;
                        let head_perp = head_dir.rot90();

                        // Arrowhead at target
                        let head_end = p2_off;
                        painter.line_segment([head_end, head_end - (head_dir + head_perp * 0.5).normalized() * head_size], stroke);
                        painter.line_segment([head_end, head_end - (head_dir - head_perp * 0.5).normalized() * head_size], stroke);

                        // Arrowhead at source for bidirectional relations
                        if *is_bidirectional {
                            let head_start = p1_off;
                            painter.line_segment([head_start, head_start + (head_dir + head_perp * 0.5).normalized() * head_size], stroke);
                            painter.line_segment([head_start, head_start + (head_dir - head_perp * 0.5).normalized() * head_size], stroke);
                        }

                        // --- EDGE LABEL PILL BADGE ---
                        if self.graph_scale > 0.55 && (active_node_uri.is_none() || is_connected_to_active) {
                            let mid = p1_off + (p2_off - p1_off) * 0.5;
                            let text_pos = mid + perp * (if offset_mag >= 0.0 { 7.0 } else { -7.0 } * self.graph_scale);
                            
                            let dist_to_start = (text_pos - p_start_orig).length();
                            let dist_to_end = (text_pos - p_end_orig).length();
                            let safe_dist = radius * 1.3;

                            if dist_to_start > safe_dist && dist_to_end > safe_dist {
                                let font_size = (8.0 * self.graph_scale).clamp(6.5, 12.0);
                                let font = egui::FontId::proportional(font_size);
                                let text_color = if self.settings.theme_dark { egui::Color32::from_rgb(226, 232, 240) } else { egui::Color32::from_rgb(30, 41, 59) };
                                
                                let galley = painter.layout_no_wrap(edge.label.clone(), font, text_color);
                                let badge_size = galley.size() + Vec2::new(6.0 * self.graph_scale, 2.5 * self.graph_scale);
                                let badge_rect = egui::Rect::from_center_size(text_pos, badge_size);
                                
                                let badge_bg = if self.settings.theme_dark {
                                    egui::Color32::from_rgba_unmultiplied(15, 20, 28, 220)
                                } else {
                                    egui::Color32::from_rgba_unmultiplied(248, 250, 252, 230)
                                };
                                let badge_border = egui::Stroke::new(
                                    0.6 * self.graph_scale,
                                    if edge.is_inferred {
                                        egui::Color32::from_rgba_unmultiplied(52, 211, 153, 140)
                                    } else if is_connected_to_active {
                                        egui::Color32::from_rgba_unmultiplied(96, 165, 250, 180)
                                    } else {
                                        if self.settings.theme_dark { egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30) } else { egui::Color32::from_rgba_unmultiplied(0, 0, 0, 30) }
                                    }
                                );
                                painter.rect(badge_rect, 2.5 * self.graph_scale, badge_bg, badge_border);
                                painter.galley(badge_rect.min + Vec2::new(3.0 * self.graph_scale, 1.2 * self.graph_scale), galley, text_color);
                            }
                        }
                    }
                }
            }

            // 4.2. Draw Node Shapes and Halos
            for (uri, node) in &self.nodes {
                let pos = to_screen_pos(node.pos, self.graph_offset, self.graph_scale);
                let is_selected = self.selected_uri.as_deref() == Some(uri);
                let is_hovered = hovered_node_uri.as_deref() == Some(uri);
                let is_connected_to_active = connected_nodes.contains(uri.as_str());
                let matches_search = !self.search_query.is_empty() && 
                    (node.label.to_lowercase().contains(&self.search_query.to_lowercase()) || 
                     uri.to_lowercase().contains(&self.search_query.to_lowercase()));
                
                let radius = base_radius;
                let is_expanded = self.expanded_nodes.contains(uri);

                // Halos & Highlight Rings
                if matches_search {
                    painter.circle(pos, radius + 4.0 * self.graph_scale, egui::Color32::TRANSPARENT, egui::Stroke::new(2.5 * self.graph_scale, egui::Color32::from_rgb(245, 158, 11)));
                } else if is_selected {
                    painter.circle(pos, radius + 4.0 * self.graph_scale, egui::Color32::TRANSPARENT, egui::Stroke::new(2.0 * self.graph_scale, if self.settings.theme_dark { egui::Color32::WHITE } else { egui::Color32::from_rgb(30, 41, 59) }));
                } else if is_hovered {
                    painter.circle(pos, radius + 3.0 * self.graph_scale, egui::Color32::TRANSPARENT, egui::Stroke::new(1.5 * self.graph_scale, egui::Color32::from_rgb(96, 165, 250)));
                } else if is_expanded {
                    painter.circle(pos, radius + 3.5 * self.graph_scale, egui::Color32::TRANSPARENT, egui::Stroke::new(1.2 * self.graph_scale, egui::Color32::from_rgb(56, 189, 248)));
                }

                let (base_fill, glyph_char) = match node.node_type {
                    NodeType::Class => (
                        if self.settings.theme_dark { egui::Color32::from_rgb(245, 158, 11) } else { egui::Color32::from_rgb(217, 119, 6) },
                        "C"
                    ),
                    NodeType::Property => (
                        if self.settings.theme_dark { egui::Color32::from_rgb(59, 130, 246) } else { egui::Color32::from_rgb(37, 99, 235) },
                        "P"
                    ),
                    NodeType::Individual => (
                        if self.settings.theme_dark { egui::Color32::from_rgb(168, 85, 247) } else { egui::Color32::from_rgb(126, 34, 206) },
                        "I"
                    ),
                    NodeType::Literal => (
                        if self.settings.theme_dark { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(5, 150, 105) },
                        "L"
                    ),
                    NodeType::Blank => (
                        if self.settings.theme_dark { egui::Color32::from_rgb(100, 116, 139) } else { egui::Color32::from_rgb(148, 163, 184) },
                        "_"
                    ),
                };

                let fill_color = if active_node_uri.is_some() && !is_connected_to_active {
                    base_fill.linear_multiply(0.4)
                } else {
                    base_fill
                };

                let stroke = egui::Stroke::new(
                    if is_selected || is_hovered { 1.8 * self.graph_scale } else { 1.0 * self.graph_scale },
                    if is_selected { 
                        egui::Color32::WHITE 
                    } else if is_hovered {
                        egui::Color32::from_rgb(191, 219, 254)
                    } else if self.settings.theme_dark { 
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 70) 
                    } else { 
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 80) 
                    }
                );

                match node.node_type {
                    NodeType::Class => { 
                        painter.circle(pos, radius, fill_color, stroke); 
                    },
                    NodeType::Property => { 
                        painter.rect(egui::Rect::from_center_size(pos, Vec2::new(radius * 2.3, radius * 1.3)), 3.5 * self.graph_scale, fill_color, stroke); 
                    },
                    NodeType::Individual => {
                        let p1 = pos + Vec2::new(0.0, -radius); let p2 = pos + Vec2::new(radius, 0.0);
                        let p3 = pos + Vec2::new(0.0, radius); let p4 = pos + Vec2::new(-radius, 0.0);
                        painter.add(egui::Shape::convex_polygon(vec![p1, p2, p3, p4], fill_color, stroke));
                    },
                    NodeType::Literal => { 
                        painter.rect(egui::Rect::from_center_size(pos, Vec2::new(radius * 2.0, radius * 1.1)), 2.5 * self.graph_scale, fill_color, stroke); 
                    },
                    NodeType::Blank => { 
                        painter.circle(pos, radius * 0.85, fill_color, stroke); 
                    },
                }

                // Inner glyph indicator for high zoom levels (Detailed technical look)
                if self.graph_scale >= 0.75 && (active_node_uri.is_none() || is_connected_to_active) {
                    let glyph_font = egui::FontId::proportional((8.0 * self.graph_scale).clamp(7.0, 11.0));
                    painter.text(pos, egui::Align2::CENTER_CENTER, glyph_char, glyph_font, egui::Color32::WHITE);
                }
            }

            // 4.3. Draw Node Labels (Compact, High-Precision Badges)
            for (uri, node) in &self.nodes {
                let pos = to_screen_pos(node.pos, self.graph_offset, self.graph_scale);
                let radius = base_radius;
                let is_selected = self.selected_uri.as_deref() == Some(uri);
                let is_hovered = hovered_node_uri.as_deref() == Some(uri);
                let is_connected_to_active = connected_nodes.contains(uri.as_str());

                if self.graph_scale > 0.45 {
                    if active_node_uri.is_some() && !is_connected_to_active && self.graph_scale < 0.8 {
                        continue;
                    }

                    let font_size = (9.0 * self.graph_scale).clamp(7.5, 14.0);
                    let font = egui::FontId::proportional(font_size);
                    let text_pos = pos + Vec2::new(0.0, radius + 3.5 * self.graph_scale);
                    
                    let display_label = if node.label.chars().count() > 22 && self.graph_scale < 1.2 {
                        let truncated: String = node.label.chars().take(20).collect();
                        format!("{}…", truncated)
                    } else {
                        node.label.clone()
                    };

                    let text_color = if self.settings.theme_dark { 
                        if active_node_uri.is_some() && !is_connected_to_active { egui::Color32::from_rgb(148, 163, 184) } else { egui::Color32::from_rgb(241, 245, 249) }
                    } else { 
                        if active_node_uri.is_some() && !is_connected_to_active { egui::Color32::from_rgb(148, 163, 184) } else { egui::Color32::from_rgb(15, 23, 42) }
                    };

                    let galley = painter.layout_no_wrap(display_label, font, text_color);
                    let pill_size = galley.size() + Vec2::new(6.0 * self.graph_scale, 2.5 * self.graph_scale);
                    let pill_rect = egui::Rect::from_center_size(text_pos + Vec2::new(0.0, galley.size().y * 0.5), pill_size);

                    let pill_bg = if self.settings.theme_dark {
                        egui::Color32::from_rgba_unmultiplied(15, 20, 28, 195)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(250, 252, 255, 215)
                    };

                    let pill_stroke = egui::Stroke::new(
                        0.5 * self.graph_scale,
                        if is_selected {
                            if self.settings.theme_dark { egui::Color32::from_rgb(96, 165, 250) } else { egui::Color32::from_rgb(37, 99, 235) }
                        } else if is_hovered {
                            egui::Color32::from_rgb(147, 197, 253)
                        } else {
                            if self.settings.theme_dark { egui::Color32::from_rgba_unmultiplied(255, 255, 255, 25) } else { egui::Color32::from_rgba_unmultiplied(0, 0, 0, 25) }
                        }
                    );

                    painter.rect(pill_rect, 2.5 * self.graph_scale, pill_bg, pill_stroke);
                    painter.galley(pill_rect.min + Vec2::new(3.0 * self.graph_scale, 1.2 * self.graph_scale), galley, text_color);
                }
            }

            // 4.4. Node Hover Tooltip
            if let Some(hovered_uri) = &hovered_node_uri {
                if let Some(node) = self.nodes.get(hovered_uri) {
                    let in_deg = self.edges.iter().filter(|e| e.to == *hovered_uri).count();
                    let out_deg = self.edges.iter().filter(|e| e.from == *hovered_uri).count();
                    let type_str = match node.node_type {
                        NodeType::Class => "Class",
                        NodeType::Property => "Property",
                        NodeType::Individual => "Individual",
                        NodeType::Literal => "Literal",
                        NodeType::Blank => "Blank Node",
                    };
                    let type_color = match node.node_type {
                        NodeType::Class => egui::Color32::from_rgb(245, 158, 11),
                        NodeType::Property => egui::Color32::from_rgb(59, 130, 246),
                        NodeType::Individual => egui::Color32::from_rgb(168, 85, 247),
                        NodeType::Literal => egui::Color32::from_rgb(16, 185, 129),
                        NodeType::Blank => egui::Color32::GRAY,
                    };
                    
                    let layer_id = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("graph_tooltip_layer"));
                    egui::show_tooltip_at_pointer(ui.ctx(), layer_id, egui::Id::new("graph_node_tooltip"), |ui: &mut egui::Ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(type_color, egui::RichText::new(format!("[{}]", type_str)).strong().size(11.0));
                                ui.label(egui::RichText::new(&node.label).strong().size(12.0));
                            });
                            ui.label(egui::RichText::new(hovered_uri).size(10.0).weak());
                            ui.label(egui::RichText::new(format!("연결: In {} / Out {} (총 {})", in_deg, out_deg, in_deg + out_deg)).size(10.0));
                            ui.separator();
                            ui.label(egui::RichText::new("클릭: 선택 | 더블클릭: 확장/축소 | 드래그: 이동").size(9.5).weak());
                        });
                    });
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
