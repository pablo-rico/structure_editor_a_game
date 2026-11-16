//! Nombres en español para artistas (docs/diseno-gui.md §6). Se busca por el *nombre* del enum del juego, nunca por
//! su valor: un material nuevo que aún no esté aquí cae en «Otros» con su nombre tal cual.

/// Familias de la paleta, en orden de presentación.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Stone,
    Walls,
    Wood,
    Roofs,
    GlassTile,
    Metal,
    Cloth,
    Nature,
    FireDamage,
    Variable,
    Other,
}

impl Family {
    pub const ALL: [Family; 11] = [
        Family::Stone,
        Family::Walls,
        Family::Wood,
        Family::Roofs,
        Family::GlassTile,
        Family::Metal,
        Family::Cloth,
        Family::Nature,
        Family::FireDamage,
        Family::Variable,
        Family::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Family::Stone => "Piedra",
            Family::Walls => "Muros y revoques",
            Family::Wood => "Madera",
            Family::Roofs => "Tejados",
            Family::GlassTile => "Vidrio y azulejo",
            Family::Metal => "Metal",
            Family::Cloth => "Telas y cabos",
            Family::Nature => "Naturaleza",
            Family::FireDamage => "Fuego y daño",
            Family::Variable => "Color variable",
            Family::Other => "Otros",
        }
    }
}

/// Etiqueta y familia de un material por su nombre en el enum. `None` = no se ofrece en la paleta (aire, suelo
/// invisible de las bases).
pub fn material(name: &str) -> Option<(&'static str, Family)> {
    use Family::*;
    Some(match name {
        "Air" | "Hidden" => return None,
        "Stone" => ("Piedra", Stone),
        "Cobble" => ("Mampostería", Stone),
        "Ashlar" => ("Sillería", Stone),
        "Granite" => ("Granito", Stone),
        "Sandstone" => ("Arenisca", Stone),
        "WornStone" => ("Piedra gastada", Stone),
        "Marble" => ("Mármol", Stone),
        "RedMarble" => ("Mármol rojo", Stone),
        "DarkMarble" => ("Mármol oscuro", Stone),
        "Plaster" => ("Enlucido", Walls),
        "Whitewash" => ("Cal", Walls),
        "Ochre" => ("Estuco ocre", Walls),
        "Adobe" => ("Adobe", Walls),
        "Brick" => ("Ladrillo", Walls),
        "Terracotta" => ("Terracota", Walls),
        "Fresco" => ("Fresco", Walls),
        "Plank" => ("Tablón", Wood),
        "Log" => ("Tronco", Wood),
        "DarkTimber" => ("Entramado", Wood),
        "Cedar" => ("Cedro", Wood),
        "Shutter" => ("Contraventana", Wood),
        "Scaffold" => ("Andamio", Wood),
        "HullPlank" => ("Casco", Wood),
        "DeckPlank" => ("Cubierta", Wood),
        "Tar" => ("Brea", Wood),
        "RoofTile" => ("Teja", Roofs),
        "BrownTile" => ("Teja parda", Roofs),
        "GlazedTile" => ("Teja vidriada", Roofs),
        "GreenGlaze" => ("Teja verde", Roofs),
        "Slate" => ("Pizarra", Roofs),
        "Thatch" => ("Bálago", Roofs),
        "WoodShingle" => ("Tablilla", Roofs),
        "TarShingle" => ("Tablilla alquitranada", Roofs),
        "Lead" => ("Plomo", Roofs),
        "Glass" => ("Vidrio", GlassTile),
        "StainedGlass" => ("Vidriera azul", GlassTile),
        "StainedRed" => ("Vidriera roja", GlassTile),
        "StainedGold" => ("Vidriera dorada", GlassTile),
        "Crystal" => ("Cristal", GlassTile),
        "Azulejo" => ("Azulejo", GlassTile),
        "Zellige" => ("Zellige", GlassTile),
        "Iron" => ("Hierro", Metal),
        "Gold" => ("Oro", Metal),
        "Bronze" => ("Bronce", Metal),
        "Verdigris" => ("Cobre (pátina)", Metal),
        "ClothRed" => ("Tela roja", Cloth),
        "ClothBlue" => ("Tela azul", Cloth),
        "ClothGreen" => ("Tela verde", Cloth),
        "ClothYellow" => ("Tela amarilla", Cloth),
        "ClothPurple" => ("Tela morada", Cloth),
        "Wool" => ("Lana", Cloth),
        "Canvas" => ("Lona", Cloth),
        "SailCloth" => ("Vela", Cloth),
        "Rope" => ("Cabo", Cloth),
        "Straw" => ("Paja", Cloth),
        "Grass" => ("Césped", Nature),
        "Gravel" => ("Grava", Nature),
        "Moss" => ("Musgo", Nature),
        "Leaves" => ("Hojas", Nature),
        "Ivy" => ("Hiedra", Nature),
        "Blossom" => ("Flores", Nature),
        "Water" => ("Agua", Nature),
        "Ember" => ("Brasa", FireDamage),
        "Soot" => ("Hollín", FireDamage),
        "CharredWood" => ("Madera quemada", FireDamage),
        "Rubble" => ("Escombro", FireDamage),
        "ThemeStone" => ("Piedra del tema", Variable),
        "ThemeTrim" => ("Moldura del tema", Variable),
        "ThemeRoof" => ("Tejado del tema", Variable),
        "ThemeCloth" => ("Tela del tema", Variable),
        "ThemeGlow" => ("Brillo del tema", Variable),
        "TeamCloth" => ("Tela del reino", Variable),
        _ => ("", Other),
    })
}

/// Etiqueta visible de un material (el nombre del enum si aún no tiene traducción).
pub fn material_label(name: &str) -> &str {
    match material(name) {
        Some((label, _)) if !label.is_empty() => label,
        _ => name,
    }
}

/// Material cuyo color depende del tema o del reino (muestra con esquina partida).
pub fn is_variable(name: &str) -> bool {
    name == "SailCloth" || matches!(material(name), Some((_, Family::Variable)))
}

pub fn part_kind(name: &str) -> &str {
    match name {
        "Door" => "Puerta",
        "Breakable" => "Muro rompible",
        "Portcullis" => "Rastrillo",
        "Bascule" => "Levadizo",
        "GateLeaves" => "Hojas de puerta",
        other => other,
    }
}

pub fn interactable_kind(name: &str) -> &str {
    match name {
        "Door" => "Puerta",
        "Bed" => "Cama",
        "Chair" => "Silla",
        "Chest" => "Cofre",
        "Counter" => "Mostrador",
        "HarbourDesk" => "Mesa del puerto",
        "Helm" => "Timón",
        "Cannon" => "Cañón",
        "CargoHatch" => "Escotilla",
        "Anchor" => "Ancla",
        "Gangway" => "Pasarela",
        "Mooring" => "Amarre",
        other => other,
    }
}

/// Icono Phosphor de cada grupo del catálogo (§4).
pub fn group_icon(key: &str) -> &'static str {
    use egui_phosphor::regular as i;
    match key {
        "city" => i::BUILDINGS,
        "fort" => i::CASTLE_TURRET,
        "bridge" => i::BRIDGE,
        "harbour" => i::ANCHOR,
        "zone" => i::AXE,
        "school" => i::MAGIC_WAND,
        "wild" => i::SKULL,
        "boat" => i::SAILBOAT,
        "legacy" => i::ARCHIVE,
        _ => i::CUBE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquetas_y_familias() {
        assert_eq!(material("Air"), None);
        assert_eq!(material("Hidden"), None);
        assert_eq!(material_label("Ashlar"), "Sillería");
        assert_eq!(material_label("MaterialNuevo"), "MaterialNuevo");
        assert_eq!(material("MaterialNuevo").map(|m| m.1), Some(Family::Other));
        assert!(is_variable("ThemeGlow") && is_variable("SailCloth") && !is_variable("Stone"));
    }
}
