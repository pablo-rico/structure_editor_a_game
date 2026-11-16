//! Parámetros de lo que se explora (tipo, estilo, campos de variante, semilla, tamaño…) y estado de las partes
//! móviles para la vista previa. Puro: la UI solo los edita y los manda al puente.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::bridge::proto::{BuildRequest, Catalog, FieldKind, KindInfo, Part};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Params {
    pub kind: String,
    pub style: String,
    pub fields: BTreeMap<String, i64>,
    pub seed: i32,
    pub width: i32,
    pub depth: i32,
    pub facing: i32,
    pub foundation: i32,
    pub damage: String,
    /// Estado de las partes móviles (2 bits por parte); no forma parte del diseño.
    pub dynamic: u32,
}

/// Cimientos que se ofrecen (niveles enterrados bajo el suelo).
pub const FOUNDATION: (i32, i32) = (1, 4);

impl Params {
    /// Valores de partida de un tipo: primer estilo, campos por defecto, tamaño medio, sin girar, intacto, semilla 0.
    pub fn defaults(kind: &KindInfo, cat: &Catalog) -> Self {
        let mut p = Self {
            kind: kind.name.clone(),
            style: String::new(),
            fields: BTreeMap::new(),
            seed: 0,
            width: 0,
            depth: 0,
            facing: cat.facings.iter().find(|f| f.turns == 0).or(cat.facings.first()).map_or(3, |f| f.value),
            foundation: 1,
            damage: cat.damages.first().map_or_else(|| "Intact".into(), |d| d.name.clone()),
            dynamic: 0,
        };
        if let Some(r) = kind.size {
            p.width = (r.min[0] + r.max[0]) / 2;
            p.depth = (r.min[1] + r.max[1]) / 2;
        }
        p.fit(kind, cat);
        p
    }

    /// Ajusta a lo que permite el tipo (tras recargar el catálogo o cambiar de estilo): estilo válido, campos que
    /// existan dentro de rango, tamaño dentro del rango.
    pub fn fit(&mut self, kind: &KindInfo, cat: &Catalog) {
        let styles: Vec<&String> =
            if kind.styles.is_empty() { cat.styles.iter().map(|s| &s.name).collect() } else { kind.styles.iter().collect() };
        if !styles.contains(&&self.style) {
            self.style = styles.first().map_or_else(String::new, |s| (*s).clone());
        }
        self.fields.retain(|k, _| kind.fields.iter().any(|f| &f.key == k));
        for f in &kind.fields {
            let v = self.fields.entry(f.key.clone()).or_insert(f.default);
            *v = match &f.kind {
                FieldKind::Index { min, .. } | FieldKind::Range { min, .. } => {
                    (*v).clamp(*min, field_max(&f.kind, &self.style).max(*min))
                }
                FieldKind::Choice { options } if !options.iter().any(|o| o.value == *v) => {
                    options.first().map_or(f.default, |o| o.value)
                }
                FieldKind::Element => (*v).clamp(0, cat.elements.len().saturating_sub(1) as i64),
                FieldKind::Bool => (*v != 0) as i64,
                _ => *v,
            };
        }
        if let Some(r) = kind.size {
            self.width = self.width.clamp(r.min[0], r.max[0]);
            self.depth = self.depth.clamp(r.min[1], r.max[1]);
        }
        self.foundation = self.foundation.clamp(FOUNDATION.0, FOUNDATION.1);
    }

    pub fn request(&self) -> BuildRequest {
        BuildRequest {
            kind: self.kind.clone(),
            style: self.style.clone(),
            fields: self.fields.iter().map(|(k, v)| (k.clone(), *v)).collect::<HashMap<_, _>>(),
            seed: self.seed,
            width: self.width,
            depth: self.depth,
            facing: self.facing,
            foundation: self.foundation,
            damage: self.damage.clone(),
            dynamic: self.dynamic,
            party: 0,
            procedural: false,
        }
    }
}

/// Máximo de un campo con índice o rango; los diseños pueden depender del estilo (`maxByStyle`).
pub fn field_max(kind: &FieldKind, style: &str) -> i64 {
    match kind {
        FieldKind::Index { max, max_by_style, .. } => *max_by_style.get(style).unwrap_or(max),
        FieldKind::Range { max, .. } => *max,
        _ => 0,
    }
}

/// Grupo de partes móviles que se previsualiza a la vez (§6, «Estado (vista previa)»).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartGroup {
    pub kind: &'static str,
    pub label: &'static str,
    pub states: &'static [&'static str],
    /// El estado 0/1 se invierte con `StartsOpen` (puertas, rastrillos, levadizos).
    pub toggles: bool,
}

pub const PART_GROUPS: [PartGroup; 5] = [
    PartGroup { kind: "Door", label: "Puertas", states: &["Cerradas", "Abiertas"], toggles: true },
    PartGroup { kind: "Portcullis", label: "Rastrillo", states: &["Bajado", "Subido"], toggles: true },
    PartGroup { kind: "Bascule", label: "Levadizo", states: &["Bajado", "Alzado"], toggles: true },
    PartGroup {
        kind: "Breakable",
        label: "Muros rompibles",
        states: &["Intacto", "Agrietado", "Roto", "Derribado"],
        toggles: false,
    },
    PartGroup {
        kind: "GateLeaves",
        label: "Hojas de la puerta",
        states: &["Abiertas", "Cerradas", "Astilladas", "Reventadas"],
        toggles: false,
    },
];

fn raw(dynamic: u32, i: usize) -> u32 {
    dynamic >> (2 * i) & 3
}

/// Estado visible de un grupo (el de su primera parte), o `None` si la estructura no tiene partes de ese tipo.
pub fn group_state(parts: &[Part], dynamic: u32, group: &PartGroup) -> Option<u32> {
    let (i, p) = parts.iter().enumerate().find(|(_, p)| p.kind == group.kind)?;
    let r = raw(dynamic, i);
    Some(if group.toggles { (r & 1) ^ p.starts_open as u32 } else { r })
}

/// Pone todas las partes del grupo en el estado visible `state` y devuelve el nuevo `dynamic`.
pub fn set_group_state(parts: &[Part], mut dynamic: u32, group: &PartGroup, state: u32) -> u32 {
    for (i, p) in parts.iter().enumerate().filter(|(_, p)| p.kind == group.kind) {
        let r = if group.toggles { (state & 1) ^ p.starts_open as u32 } else { state & 3 };
        dynamic = dynamic & !(3 << (2 * i)) | r << (2 * i);
    }
    dynamic
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: &str, starts_open: bool) -> Part {
        Part {
            kind: kind.into(),
            pattern: "Solid".into(),
            material: "Plank".into(),
            bbox: [0; 6],
            swing: 1,
            detail: true,
            starts_open,
            locked: false,
            cell: None,
        }
    }

    #[test]
    fn estados_de_partes() {
        let parts = [part("Door", false), part("Breakable", false), part("Door", true)];
        let g = &PART_GROUPS[0];
        assert_eq!(group_state(&parts, 0, g), Some(0)); // cerradas
        let d = set_group_state(&parts, 0, g, 1); // abrir todas
        assert_eq!(d & 3, 1); // la normal: bit a 1
        assert_eq!(d >> 4 & 3, 0); // la que empieza abierta: bit a 0
        assert_eq!(group_state(&parts, d, g), Some(1));
        let d = set_group_state(&parts, d, &PART_GROUPS[3], 2);
        assert_eq!(d >> 2 & 3, 2);
        assert_eq!(group_state(&parts, d, &PART_GROUPS[3]), Some(2));
        assert_eq!(group_state(&parts, d, &PART_GROUPS[1]), None);
    }

    #[test]
    fn todos_los_tipos_del_catalogo_real() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let Ok(cat) = crate::bridge::proto::load_catalog_fixture(&dir) else { return };
        for k in &cat.kinds {
            let p = Params::defaults(k, &cat);
            let mut again = p.clone();
            again.fit(k, &cat);
            assert_eq!(again, p, "{}: fit no es estable", k.name);
            assert!(k.styles.is_empty() || k.styles.contains(&p.style), "{}: estilo {}", k.name, p.style);
            assert_eq!(p.fields.len(), k.fields.len(), "{}", k.name);
            assert!(crate::labels::group_icon(&k.group) != egui_phosphor::regular::CUBE, "{}: grupo {}", k.name, k.group);
        }
        // Todo material del juego tiene etiqueta en español (salvo los que no se ofrecen).
        let sin: Vec<&String> = cat
            .materials
            .iter()
            .filter(|m| crate::labels::material(m).is_some_and(|(l, _)| l.is_empty()))
            .collect();
        assert!(sin.is_empty(), "materiales sin etiqueta: {sin:?}");
    }

    #[test]
    fn ajusta_al_tipo() {
        let cat: Catalog = serde_json::from_value(serde_json::json!({
            "materials": ["Air"], "palette": [[0,0,0]], "elements": [],
            "styles": [{"name": "Northern", "label": "Norteño"}, {"name": "Arabic", "label": "Árabe"}],
            "facings": [{"value": 3, "label": "Sur", "turns": 0}], "damages": [{"name": "Intact", "label": "Intacto"}],
            "groups": [], "kinds": []
        }))
        .unwrap();
        let kind: KindInfo = serde_json::from_value(serde_json::json!({
            "id": 4, "name": "Home", "label": "Casa", "group": "city", "styles": ["Northern", "Arabic"],
            "size": {"min": [4, 4], "max": [9, 8]},
            "fields": [{"key": "design", "label": "Diseño", "type": "index", "min": 0, "max": 5, "default": 0,
                        "maxByStyle": {"Arabic": 2}}]
        }))
        .unwrap();
        let mut p = Params::defaults(&kind, &cat);
        assert_eq!((p.width, p.depth, p.style.as_str(), p.facing), (6, 6, "Northern", 3));
        p.fields.insert("design".into(), 5);
        p.style = "Arabic".into();
        p.width = 40;
        p.fit(&kind, &cat);
        assert_eq!(p.fields["design"], 2);
        assert_eq!(p.width, 9);
    }
}
