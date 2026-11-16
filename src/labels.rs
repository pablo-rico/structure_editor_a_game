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

/// Grupos de la Biblioteca › Modelos: los modelos de vóxeles del juego, según su carpeta bajo `Assets/_Game/Art`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelGroup {
    Trees,
    Nature,
    Ruins,
    Furniture,
    Shrine,
    Intro,
    Book,
    Other,
}

impl ModelGroup {
    pub const ALL: [ModelGroup; 8] = [
        ModelGroup::Trees,
        ModelGroup::Nature,
        ModelGroup::Ruins,
        ModelGroup::Furniture,
        ModelGroup::Shrine,
        ModelGroup::Intro,
        ModelGroup::Book,
        ModelGroup::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ModelGroup::Trees => "Árboles",
            ModelGroup::Nature => "Naturaleza y objetos",
            ModelGroup::Ruins => "Ruinas y caminos",
            ModelGroup::Furniture => "Muebles y cofres",
            ModelGroup::Shrine => "Pilar de llegada",
            ModelGroup::Intro => "Intro",
            ModelGroup::Book => "Libro de política",
            ModelGroup::Other => "Otros",
        }
    }

    /// Grupo de un modelo por su ruta relativa a la carpeta de arte del juego.
    pub fn of(rel_path: &str) -> Self {
        let p = rel_path.replace('\\', "/");
        let name = p.rsplit('/').next().unwrap_or("");
        match () {
            _ if p.starts_with("Objects/Trees3D/") => ModelGroup::Trees,
            _ if p.starts_with("Objects/Voxel/") => ModelGroup::Nature,
            _ if p.starts_with("Objects/Decor/") => ModelGroup::Ruins,
            _ if p.starts_with("Objects/Storage/") || p.starts_with("Objects/Furniture/") => ModelGroup::Furniture,
            _ if p.starts_with("Objects/Lights/") => ModelGroup::Shrine,
            _ if p.starts_with("Intro/") && name.starts_with("pbk_") => ModelGroup::Book,
            _ if p.starts_with("Intro/") => ModelGroup::Intro,
            _ => ModelGroup::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grupos_de_modelos_por_carpeta() {
        use ModelGroup::*;
        for (path, group) in [
            ("Objects/Trees3D/voxel/oak_1_model.json", Trees),
            ("Objects/Trees3D/palm/palm_v1_trunk_model.json", Trees),
            ("Objects/Voxel/cactus_model.json", Nature),
            ("Objects/Decor/RuinBlock/ruin_block_0_model.json", Ruins),
            ("Objects/Decor/StonePath/stone_path_3_model.json", Ruins),
            ("Objects/Storage/Chest/chest_0_model.json", Furniture),
            ("Objects/Furniture/SleepingBag/sleeping_bag_model.json", Furniture),
            ("Objects/Lights/ArrivalShrine/arrival_shrine_model.json", Shrine),
            ("Intro/Resources/Intro/Models/intro_army_model.json", Intro),
            ("Intro/Resources/Intro/Models/pbk_table_model.json", Book),
            ("Otra/cosa_model.json", Other),
        ] {
            assert_eq!(ModelGroup::of(path), group, "{path}");
            assert_eq!(ModelGroup::of(&path.replace('/', "\\")), group, "{path} con barras de Windows");
        }
        assert!(ModelGroup::ALL.iter().all(|g| !g.label().is_empty()));
    }

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
