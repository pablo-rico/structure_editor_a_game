//! Diálogo «Importar de MagicaVoxel» (docs/diseno-gui.md §10): vista previa, frente, colores → materiales (con el más
//! parecido propuesto), cimientos y avisos. Devuelve la estructura lista para abrir en Editar.

use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Sense, TextureHandle, vec2};
use egui_phosphor::regular as icon;

use super::widgets::{self, dim};
use crate::bridge::proto::Catalog;
use crate::edit::ops::Clip;
use crate::labels::{self, Family};
use crate::model::{SUB, Structure, VoxelGrid};
use crate::theme;
use crate::vox::{self, VoxFile};

pub struct VoxDialog {
    pub path: PathBuf,
    pub file: VoxFile,
    pub map: BTreeMap<u8, Option<u8>>,
    /// Cuartos de vuelta para que el frente elegido mire a la calle (sur).
    pub turns: i32,
    pub foundation: bool,
    preview: Option<TextureHandle>,
    dirty: bool,
}

pub enum VoxOutcome {
    Import { name: String, s: Box<Structure>, map: BTreeMap<u8, Option<u8>> },
    Cancel,
}

/// Materiales que se proponen solos: sin aire, suelo invisible, colores variables ni los de daño (un gris de piedra
/// no debe acabar en escombro por parecido de color). A mano se puede elegir cualquiera.
pub fn usable(cat: &Catalog) -> Vec<bool> {
    cat.materials.iter().map(|m| labels::material(m).is_some_and(|(_, f)| !matches!(f, Family::Variable | Family::FireDamage))).collect()
}

impl VoxDialog {
    pub fn new(path: PathBuf, file: VoxFile, cat: &Catalog, remembered: Option<&BTreeMap<u8, Option<u8>>>) -> Self {
        let mut map = vox::auto_map(&file, &cat.palette, &usable(cat));
        if let Some(r) = remembered {
            for (k, v) in r {
                if map.contains_key(k) {
                    map.insert(*k, *v);
                }
            }
        }
        Self { path, file, map, turns: 0, foundation: true, preview: None, dirty: true }
    }

    /// La estructura con la asociación, el frente y los cimientos elegidos.
    pub fn structure(&self, cat: &Catalog) -> Structure {
        let stone = cat.materials.iter().position(|m| m == "Stone").map(|i| i as u8);
        let mut s = vox::to_structure(&self.file, &self.map, self.foundation.then_some(stone).flatten());
        if self.turns != 0 {
            let [sx, sy, sz] = s.grid.size;
            let mut clip = Clip::copy(&s.grid, [0, 0, 0], [sx - 1, sy - 1, sz - 1]);
            for _ in 0..self.turns {
                clip = clip.rotate_quarter();
            }
            let mut g = VoxelGrid::new(clip.size);
            for (v, m, _) in clip.solids() {
                g.set(v[0], v[1], v[2], m);
            }
            s.entrance = [g.size[0] / SUB / 2, -1];
            s.grid = g;
        }
        s
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        cat: &Catalog,
        render: &mut dyn FnMut(&Structure) -> Option<TextureHandle>,
    ) -> Option<VoxOutcome> {
        if self.dirty {
            self.preview = render(&self.structure(cat));
            self.dirty = false;
        }
        let mut out = None;
        let name = self.path.file_stem().map_or_else(|| "importado".into(), |s| s.to_string_lossy().into_owned());
        egui::Modal::new(egui::Id::new("importar-vox")).show(ctx, |ui| {
            ui.set_width(680.0);
            ui.label(RichText::new(format!("Importar de MagicaVoxel · {name}.vox")).font(theme::semibold(18.0)).color(theme::TITLE));
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(260.0, 200.0), Sense::hover());
                ui.painter().rect_filled(rect, 2.0, theme::VIEW_BG);
                theme::bevel(ui.painter(), rect, false);
                if let Some(t) = &self.preview {
                    let s = t.size_vec2();
                    let k = (rect.width() / s.x).min(rect.height() / s.y);
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    ui.painter().image(t.id(), egui::Rect::from_center_size(rect.center(), s * k), uv, Color32::WHITE);
                }
                ui.vertical(|ui| {
                    let [x, y, z] = self.file.size;
                    ui.label(RichText::new(format!("{x}×{z}×{y} sub-cubos")).font(theme::mono(13.0)));
                    ui.label(dim(format!("≈ {}×{} celdas, {} niveles", (x + SUB - 1) / SUB, (z + SUB - 1) / SUB, (y + SUB - 1) / SUB)));
                    if self.file.models > 1 {
                        ui.label(dim(format!("{} modelos, todos juntos", self.file.models)));
                    }
                    ui.add_space(6.0);
                    ui.label("Frente (lo que da a la calle)");
                    let mut t = self.turns;
                    if widgets::segmented(ui, &mut t, &[(0, "Sur".into()), (1, "Este".into()), (2, "Norte".into()), (3, "Oeste".into())]) {
                        self.turns = t;
                        self.dirty = true;
                    }
                    if ui.checkbox(&mut self.foundation, "Añadir cimientos (1 nivel de piedra debajo)").changed() {
                        self.dirty = true;
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Colores ({})", self.file.colors.len())).font(theme::semibold(13.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Asociar de nuevo").on_hover_text("Volver a proponer el material más parecido").clicked() {
                        self.map = vox::auto_map(&self.file, &cat.palette, &usable(cat));
                        self.dirty = true;
                    }
                });
            });
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for c in self.file.colors.clone() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, Color32::from_rgb(c.rgb[0], c.rgb[1], c.rgb[2]));
                        theme::bevel(ui.painter(), r, false);
                        ui.label(RichText::new(format!("{:>7} sub-cubos", c.count)).font(theme::mono(12.0)));
                        ui.label(icon::ARROW_RIGHT);
                        let chosen = self.map.get(&c.index).copied().flatten();
                        let text = chosen
                            .and_then(|m| cat.materials.get(m as usize))
                            .map_or("No importar".to_string(), |n| labels::material_label(n).to_string());
                        let mut pick = None;
                        egui::ComboBox::from_id_salt(("vox-color", c.index)).selected_text(text).width(200.0).show_ui(ui, |ui| {
                            if ui.selectable_label(chosen.is_none(), "No importar este color").clicked() {
                                pick = Some(None);
                            }
                            for fam in Family::ALL {
                                let ms: Vec<(usize, &String)> = cat
                                    .materials
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, n)| labels::material(n).is_some_and(|(_, f)| f == fam))
                                    .collect();
                                if ms.is_empty() {
                                    continue;
                                }
                                ui.label(dim(fam.label()));
                                for (i, n) in ms {
                                    if ui.selectable_label(chosen == Some(i as u8), labels::material_label(n)).clicked() {
                                        pick = Some(Some(i as u8));
                                    }
                                }
                            }
                        });
                        if let Some(p) = pick {
                            self.map.insert(c.index, p);
                            self.dirty = true;
                        }
                        if let Some(m) = chosen
                            && let Some(rgb) = cat.palette.get(m as usize)
                        {
                            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                            theme::bevel(ui.painter(), r, false);
                            if vox::distance(c.rgb, *rgb) > vox::FAR {
                                ui.label(RichText::new(format!("{} ninguno se parece", icon::WARNING)).color(theme::WARN));
                            }
                        }
                    });
                }
            });
            if self.file.rotated {
                ui.label(RichText::new(format!("{} Algún modelo estaba girado en la escena: se importa sin el giro.", icon::WARNING)).color(theme::WARN));
            }
            ui.label(
                RichText::new(format!(
                    "{} Después, pon la puerta con la herramienta Puerta. Se marca una planta cada 2 niveles (cámbialas en Propiedades › Plantas).",
                    icon::INFO
                ))
                .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if widgets::primary(ui, "IMPORTAR COMO ESTRUCTURA NUEVA").clicked() {
                    out = Some(VoxOutcome::Import { name: name.clone(), s: Box::new(self.structure(cat)), map: self.map.clone() });
                }
                if ui.button("Cancelar").clicked() {
                    out = Some(VoxOutcome::Cancel);
                }
            });
        });
        out
    }
}
