// Catálogo del puente (docs/contrato.md, «catalog»): qué tipos se construyen sueltos, con qué campos de variante y cómo
// se empaquetan en Variant. Las medidas, los bits y los diseños salen siempre de las funciones del juego; aquí solo hay
// etiquetas en español para artistas y el reparto en grupos.

using System;
using System.Collections.Generic;
using System.Linq;
using AGame.Core.Magic;
using AGame.Generation.WorldGen1.Structures;
using AGame.Generation.WorldGen2;
using AGame.Rendering;
using AGame.World.Boats;
using AGame.World.Structures;
using UnityEngine;

namespace AGame.StructureBridge
{
    /// <summary>Un campo de variante del contrato (<c>index | choice | mask | bool | element | range</c>).</summary>
    sealed class FieldDef
    {
        public string Key, Label, Type;
        public long Default, Min, Max;
        public Func<ArchStyle, long> MaxForStyle; // solo index: el máximo depende del estilo
        public (long value, string label)[] Options;
        public (int bit, string label)[] Bits;

        public static FieldDef Index(string key, string label, Func<ArchStyle, long> maxForStyle, long max) =>
            new FieldDef { Key = key, Label = label, Type = "index", Min = 0, Max = max, MaxForStyle = maxForStyle };

        public static FieldDef Choice(string key, string label, long def, params (long, string)[] options) =>
            new FieldDef { Key = key, Label = label, Type = "choice", Default = def, Options = options };

        public static FieldDef Bool(string key, string label) => new FieldDef { Key = key, Label = label, Type = "bool", Max = 1 };

        public static FieldDef Range(string key, string label, long min, long max, long def = 0) =>
            new FieldDef { Key = key, Label = label, Type = "range", Min = min, Max = max, Default = def };

        public static FieldDef Element() =>
            new FieldDef { Key = "element", Label = "Elemento", Type = "element", Max = MagicElements.Count - 1 };

        public static FieldDef Mask(string key, string label, long def, params (int, string)[] bits) =>
            new FieldDef { Key = key, Label = label, Type = "mask", Default = def, Bits = bits };

        /// <summary>Valor válido más cercano a <paramref name="v"/> (o el de por defecto si no es una opción).</summary>
        public long Clamp(long v, ArchStyle style)
        {
            switch (Type)
            {
                case "choice": return Options.Any(o => o.value == v) ? v : Default;
                case "mask":
                    long all = Bits.Aggregate(0L, (m, b) => m | 1L << b.bit);
                    return v & all;
                case "index": return Math.Clamp(v, Min, MaxFor(style));
                default: return Math.Clamp(v, Min, Max);
            }
        }

        public long MaxFor(ArchStyle style) => MaxForStyle != null ? MaxForStyle(style) : Max;

        /// <summary>Valores que recorre el selftest: todos, salvo los deslizadores (sus dos extremos).</summary>
        public IEnumerable<long> Values(ArchStyle style)
        {
            switch (Type)
            {
                case "choice": return Options.Select(o => o.value);
                case "mask":
                    long all = Bits.Aggregate(0L, (m, b) => m | 1L << b.bit);
                    return Enumerable.Range(0, (int)all + 1).Select(i => (long)i).Where(i => (i & ~all) == 0);
                case "range": return Min == Max ? new[] { Min } : new[] { Min, Max };
                case "index": return Enumerable.Range(0, (int)MaxFor(style) + 1).Select(i => (long)i);
                default: return Enumerable.Range((int)Min, (int)(Max - Min) + 1).Select(i => (long)i);
            }
        }
    }

    enum BuildMode
    {
        /// <summary><see cref="StructureBlueprints.Build(in StructureInstance)"/>: huella, giro, daño y partes de la instancia.</summary>
        Instance,

        /// <summary><see cref="StructureBlueprints.Build(StructureKind, int, int)"/>: la semilla decide la huella; sin giro.</summary>
        Legacy,

        /// <summary><see cref="BoatBlueprints.Build"/>: solo el casco (velas aparte), sin giro.</summary>
        Boat,
    }

    /// <summary>Valores de los campos de una petición, ya recortados.</summary>
    sealed class FieldValues
    {
        readonly Dictionary<string, long> _v;
        public FieldValues(Dictionary<string, long> v) => _v = v;
        public int this[string key] => _v.TryGetValue(key, out long x) ? (int)x : 0;
        public bool On(string key) => this[key] != 0;
        public IReadOnlyDictionary<string, long> All => _v;
    }

    sealed class KindDef
    {
        public StructureKind Kind;
        public string Group, Label, Description;
        public bool Legacy;
        public BuildMode Mode = BuildMode.Instance;
        public ArchStyle[] Styles = Array.Empty<ArchStyle>();
        public FieldDef[] Fields = Array.Empty<FieldDef>();

        /// <summary>Variant a partir de los campos y la huella sin girar (la bocamina lo saca del fondo).</summary>
        public Func<FieldValues, Vector2Int, int> Pack = (f, s) => 0;

        /// <summary>Huella mínima y máxima (sin girar) para un Variant; null si la decide la semilla.</summary>
        public Func<int, (Vector2Int min, Vector2Int max)> SizeRange;

        public string Name => Kind.ToString();

        public FieldValues Values(IReadOnlyDictionary<string, long> raw, ArchStyle style)
        {
            var d = new Dictionary<string, long>();
            foreach (var f in Fields)
                d[f.Key] = f.Clamp(raw != null && raw.TryGetValue(f.Key, out long v) ? v : f.Default, style);
            return new FieldValues(d);
        }

        public FieldValues Defaults(ArchStyle style) => Values(null, style);

        /// <summary>Todas las combinaciones de campos que recorre el selftest (y con las que se calcula la huella del catálogo).</summary>
        public IEnumerable<FieldValues> Combos(ArchStyle style)
        {
            IEnumerable<Dictionary<string, long>> acc = new[] { new Dictionary<string, long>() };
            foreach (var f in Fields)
            {
                var vals = f.Values(style).ToArray();
                acc = acc.SelectMany(d => vals.Select(v => new Dictionary<string, long>(d) { [f.Key] = v }));
            }
            return acc.Select(d => new FieldValues(d));
        }

        /// <summary>Huella del catálogo: la unión de las de todos los diseños (la petición se recorta a la del suyo).</summary>
        public (Vector2Int min, Vector2Int max)? CatalogSize()
        {
            if (SizeRange == null) return null;
            Vector2Int min = new Vector2Int(int.MaxValue, int.MaxValue), max = new Vector2Int(int.MinValue, int.MinValue);
            foreach (var style in Styles.Length > 0 ? Styles : new[] { ArchStyle.Northern })
            foreach (var f in Combos(style))
            {
                // La bocamina saca su Variant del fondo: se mira con el fondo mínimo y el máximo de la galería.
                foreach (int depth in new[] { 0, 1000 })
                {
                    var (a, b) = SizeRange(Pack(f, new Vector2Int(0, depth)));
                    min = Vector2Int.Min(min, a);
                    max = Vector2Int.Max(max, b);
                }
            }
            return (min, max);
        }
    }

    static class Catalog
    {
        static readonly ArchStyle[] AllStyles = (ArchStyle[])Enum.GetValues(typeof(ArchStyle));

        public static readonly (string key, string label)[] Groups =
        {
            ("city", "Ciudad"), ("fort", "Fortificación"), ("bridge", "Puentes"), ("harbour", "Puerto"), ("zone", "Zonas"),
            ("school", "Escuelas"), ("wild", "Salvajes"), ("boat", "Barcos"), ("legacy", "Antiguas"),
            ("authored", "Hechas a mano"),
        };

        static readonly Dictionary<string, string> StyleLabels = new Dictionary<string, string>
        {
            ["Northern"] = "Norteño", ["Mediterranean"] = "Mediterráneo", ["Arabic"] = "Árabe",
        };

        static readonly Dictionary<string, string> DamageLabels = new Dictionary<string, string>
        {
            ["Intact"] = "Intacto", ["Damaged"] = "Dañado", ["Ruined"] = "En ruinas", ["Razed"] = "Arrasado",
        };

        // Índices de StructureNav.Directions: +X, −X, +Z, −Z.
        public static readonly string[] DirNames = { "+X", "-X", "+Z", "-Z" };
        static readonly string[] DirLabels = { "Este", "Oeste", "Norte", "Sur" };

        public static string StyleLabel(ArchStyle s) => StyleLabels.TryGetValue(s.ToString(), out var l) ? l : s.ToString();

        /// <summary>Opciones de un enum del juego con sus etiquetas (las que falten, con el nombre del enum).</summary>
        static (long, string)[] EnumOptions<T>(params (T value, string label)[] labels) where T : Enum
        {
            var map = labels.ToDictionary(l => Convert.ToInt64(l.value), l => l.label);
            return Enum.GetValues(typeof(T)).Cast<T>()
                .Select(v => (Convert.ToInt64(v), map.TryGetValue(Convert.ToInt64(v), out var l) ? l : v.ToString())).ToArray();
        }

        static FieldDef Design(StructureKind kind, string label = "Diseño") =>
            FieldDef.Index("design", label, s => CityBlueprints.Variants(kind, s) - 1, CityBlueprints.Variants(kind) - 1);

        static FieldDef District() => FieldDef.Choice("district", "Barrio", 0, EnumOptions(
            (CityDistrict.Residential, "Residencial"), (CityDistrict.Noble, "Noble"), (CityDistrict.Poor, "Arrabal"),
            (CityDistrict.Market, "Mercado"), (CityDistrict.Artisan, "Artesano")));

        static FieldDef Tier() => FieldDef.Choice("tier", "Clase", (long)DistrictTier.Modest, EnumOptions(
            (DistrictTier.Poor, "Pobre"), (DistrictTier.Modest, "Modesta"), (DistrictTier.WellOff, "Acomodada"),
            (DistrictTier.Rich, "Rica")));

        static FieldDef Rise() => FieldDef.Range("rise", "Subida del paso de ronda (niveles)", 0, FortKit.MaxRise);
        static FieldDef Scaffold() => FieldDef.Bool("scaffold", "En obra (con andamios)");

        /// <summary>Bits comunes de las piezas de muralla (<see cref="FortKit"/>): subida y obra.</summary>
        static int WallBits(FieldValues f) => FortKit.WithRise(0, f["rise"]) | (f.On("scaffold") ? FortKit.Scaffold : 0);

        static (Vector2Int, Vector2Int) Fixed(Vector2Int s) => (s, s);

        static readonly List<KindDef> Defs = BuildDefs();

        public static IReadOnlyList<KindDef> Kinds => Defs;

        public static KindDef Find(string name) => Defs.FirstOrDefault(d => d.Name == name);

        static List<KindDef> BuildDefs()
        {
            var list = new List<KindDef>();

            KindDef City(StructureKind k, string label, string desc, params FieldDef[] fields)
            {
                var d = new KindDef
                {
                    Kind = k, Group = "city", Label = label, Description = desc, Styles = AllStyles, Fields = fields,
                    SizeRange = v => CityBlueprints.SizeRange(k, v),
                };
                // Por defecto el Variant es el diseño (CityBlueprints.Variants); los que no tienen campos, 0.
                if (fields.Length == 1 && fields[0].Key == "design") d.Pack = (f, s) => f["design"];
                list.Add(d);
                return d;
            }

            // ------------------------------------------------------------------ ciudad
            City(StructureKind.Home, "Casa", "Casa de ciudad: vivienda de uno a tres pisos; el barrio y la clase cambian la forma y los materiales.",
                    Design(StructureKind.Home),
                    FieldDef.Choice("rank", "Altura", 1, (0, "Baja"), (1, "Corriente"), (2, "Alta")),
                    District(), Tier())
                .Pack = (f, s) => CityHouseVariant.Pack(f["design"], f["rank"], (CityDistrict)f["district"], (DistrictTier)f["tier"]);
            City(StructureKind.Shop, "Tienda", "Tienda de gremio con la vivienda encima: tienda, taller, posada o almacén.",
                    FieldDef.Choice("shop", "Tipo de tienda", 0, EnumOptions((ShopStyle.Store, "Tienda"),
                        (ShopStyle.Workshop, "Taller"), (ShopStyle.Inn, "Posada"), (ShopStyle.Warehouse, "Almacén"))),
                    District(), Tier())
                .Pack = (f, s) => CityHouseVariant.ForShop(f["shop"], (CityDistrict)f["district"], (DistrictTier)f["tier"]);
            City(StructureKind.TownHall, "Ayuntamiento", "Ayuntamiento de la plaza mayor.", Design(StructureKind.TownHall));
            City(StructureKind.Palace, "Palacio", "Palacio del señor en la capital de un reino.");
            City(StructureKind.ImperialPalace, "Palacio imperial", "Palacio del emperador; solo existe en estilo norteño.")
                .Styles = new[] { ArchStyle.Northern };
            City(StructureKind.Bank, "Casa de banca", "Casa de banca, con el cambista tras el mostrador.");
            City(StructureKind.Jail, "Cárcel", "Cárcel con celdas enrejadas; una tiene una grieta por la que se escapa.",
                Design(StructureKind.Jail, "Celda con la grieta"));
            City(StructureKind.Stall, "Puesto de mercado", "Puesto abierto del mercado, con toldo y mostrador.", Design(StructureKind.Stall));
            City(StructureKind.Well, "Pozo", "Pozo de plaza.");
            City(StructureKind.Fountain, "Fuente", "Fuente de plaza.");
            City(StructureKind.Arcade, "Soportal", "Soportal de plaza o tramo de zoco cubierto; abierto por debajo.");
            City(StructureKind.StreetProp, "Mobiliario urbano", "Una pieza suelta de calle: farola, barriles, carro, banco…",
                    FieldDef.Choice("prop", "Objeto", 0, EnumOptions((StreetPropKind.Lamp, "Farola"),
                        (StreetPropKind.Barrel, "Barriles"), (StreetPropKind.Cart, "Carro"), (StreetPropKind.Awning, "Toldo"),
                        (StreetPropKind.Planter, "Macetero"), (StreetPropKind.Bench, "Banco"),
                        (StreetPropKind.Trough, "Abrevadero"), (StreetPropKind.PottedPalm, "Palmera en maceta"))))
                .Pack = (f, s) => f["prop"];
            City(StructureKind.Park, "Parque real", "Jardín que ocupa el solar del palacio mientras no se levanta.");
            City(StructureKind.Chapel, "Capilla", "Templo pequeño de aldea.", Design(StructureKind.Chapel));
            City(StructureKind.Church, "Iglesia", "Templo de villa, con torre o campanario.", Design(StructureKind.Church));
            City(StructureKind.Cathedral, "Catedral", "El templo mayor: el edificio más alto de la ciudad.", Design(StructureKind.Cathedral));
            City(StructureKind.Cemetery, "Camposanto", "Cementerio abierto, con su cerca.");
            City(StructureKind.CityCastle, "Castillo del señor", "Castillo de la ciudad, que hace de palacio.");
            City(StructureKind.Mill, "Molino de agua", "Molino con la rueda en el lado este (+X), junto al río.");
            // CityBlueprints.Variants: «1: arco de una puerta vieja».
            City(StructureKind.Passage, "Pasaje", "Casa-puente sobre un callejón: se pasa por debajo, de sur a norte.",
                    FieldDef.Bool("oldGate", "Arco de una puerta vieja de la cerca"))
                .Pack = (f, s) => f.On("oldGate") ? 1 : 0;

            // ------------------------------------------------------------------ fortificación (FortKit)
            KindDef Fort(StructureKind k, string label, string desc, params FieldDef[] fields)
            {
                var d = City(k, label, desc, fields);
                d.Group = "fort";
                return d;
            }
            Fort(StructureKind.WallSegment, "Lienzo de muralla", "Tramo recto de muralla con paso de ronda; el lado de fuera (almenas) es el sur (−Z).",
                Rise(), Scaffold()).Pack = (f, s) => WallBits(f);
            Fort(StructureKind.WallDiagonal, "Lienzo en diagonal", "Tramo de muralla a 45° entre el lado oeste y el sur; no ocupa toda su caja.",
                    Rise(), Scaffold(), FieldDef.Bool("concave", "Cóncavo (el lado de fuera da a la esquina)"))
                .Pack = (f, s) => WallBits(f) | (f.On("concave") ? FortKit.Concave : 0);
            Fort(StructureKind.WallTower, "Torre de muralla", "Torre de 6×6 con un brazo de muralla hacia cada salida; puerta a pie de calle en el sur.",
                    Rise(), Scaffold(),
                    FieldDef.Mask("ports", "Salidas del paso de ronda", 3,
                        Enumerable.Range(0, 4).Select(i => (i, DirLabels[i])).ToArray()),
                    FieldDef.Bool("doorRight", "Puerta a la derecha"))
                .Pack = (f, s) => FortKit.WithTowerPorts(WallBits(f), f["ports"]) | (f.On("doorRight") ? FortKit.DoorRight : 0);
            Fort(StructureKind.Gate, "Puerta de muralla", "Puerta con rastrillo que se cruza de sur a norte; la ronda pasa por encima.",
                    Rise(), Scaffold(), FieldDef.Bool("grand", "Puerta mayor (la del castillo)"))
                .Pack = (f, s) => WallBits(f) | (f.On("grand") ? FortKit.Grand : 0);
            Fort(StructureKind.WaterGate, "Puerta de agua", "Lienzo de muralla con arcos enrejados sobre el cauce.",
                Rise(), Scaffold()).Pack = (f, s) => WallBits(f);
            Fort(StructureKind.CastleKeep, "Torre del homenaje", "La torre principal del castillo.", Design(StructureKind.CastleKeep, "Tipo de torre"));
            Fort(StructureKind.CastleHall, "Cuartel o gran sala", "Edificio largo del patio de armas.", Design(StructureKind.CastleHall));
            Fort(StructureKind.CastleYard, "Dependencia del patio", "Pieza abierta del patio de armas.",
                    FieldDef.Choice("yard", "Dependencia", 0, EnumOptions((CastleYardKind.Stable, "Cuadra"),
                        (CastleYardKind.Smithy, "Herrería"), (CastleYardKind.Cistern, "Aljibe"),
                        (CastleYardKind.TrainingYard, "Palenque"), (CastleYardKind.Store, "Almacén"))))
                .Pack = (f, s) => f["yard"];

            // ------------------------------------------------------------------ puentes
            // CityBlueprints.Variants: «1: con torre-puerta».
            var cityBridge = City(StructureKind.CityBridge, "Puente de ciudad", "Puente de piedra que se cruza de sur a norte.",
                FieldDef.Bool("gateTower", "Con torre-puerta"));
            cityBridge.Group = "bridge";
            cityBridge.Pack = (f, s) => f.On("gateTower") ? 1 : 0;
            var road = City(StructureKind.RoadBridge, "Puente de carretera",
                "Puente recto o a 45°, de madera o de piedra, con rampas en las cabezas y tramo levadizo en los ríos navegables.",
                FieldDef.Bool("diagonal", "A 45°"), FieldDef.Bool("stone", "De piedra"),
                FieldDef.Bool("gateTower", "Con torre-puerta (de piedra, recto y largo)"),
                FieldDef.Bool("navigable", "Con tramo levadizo (solo recto)"),
                FieldDef.Range("rampA", "Rampa de la cabeza sur (niveles)", 0, FortBridges.MaxRamp),
                FieldDef.Range("rampB", "Rampa de la cabeza norte (niveles)", 0, FortBridges.MaxRamp));
            road.Group = "bridge";
            road.Pack = (f, s) => FortBridges.WithRamps(
                (f.On("diagonal") ? FortBridges.Diagonal : 0) | (f.On("stone") ? FortBridges.Stone : 0) |
                (f.On("gateTower") ? FortBridges.GateTower : 0) | (f.On("navigable") ? FortBridges.Navigable : 0),
                f["rampA"], f["rampB"]);

            // ------------------------------------------------------------------ puerto
            list.Add(new KindDef
            {
                Kind = StructureKind.Harbour, Group = "harbour", Label = "Pieza de puerto",
                Description = "Una pieza del puerto de tres niveles: muelles, almacenes, grúa, astillero, faro…",
                Fields = new[]
                {
                    FieldDef.Choice("piece", "Pieza", 0, EnumOptions((HarbourPiece.WoodPier, "Muelle de madera"),
                        (HarbourPiece.StoneQuay, "Malecón de piedra"), (HarbourPiece.HarbourHut, "Caseta del capitán de puerto"),
                        (HarbourPiece.Warehouse, "Almacén"), (HarbourPiece.Crane, "Grúa"), (HarbourPiece.Shipyard, "Astillero"),
                        (HarbourPiece.Mole, "Espigón"), (HarbourPiece.DryDock, "Dique seco"), (HarbourPiece.Lighthouse, "Faro"),
                        (HarbourPiece.CustomsHouse, "Aduana"), (HarbourPiece.Slipway, "Grada de botadura"))),
                },
                // HarbourLayout: Variant = HarbourPiece (bits 0–3).
                Pack = (f, s) => f["piece"],
                SizeRange = v => CityBlueprints.SizeRange(StructureKind.Harbour, v),
            });
            var dock = City(StructureKind.Dock, "Muelle (antiguo)", "El muelle de las partidas viejas; los trazados nuevos usan las piezas de puerto.");
            dock.Group = "harbour";
            dock.Legacy = true;

            // ------------------------------------------------------------------ zonas (ZoneBlueprints)
            KindDef Zone(StructureKind k, string label, string desc)
            {
                var d = new KindDef
                {
                    Kind = k, Group = "zone", Label = label, Description = desc, SizeRange = v => ZoneBlueprints.SizeRange(k, v),
                };
                list.Add(d);
                return d;
            }
            Zone(StructureKind.Sawmill, "Aserradero", "Aserradero de la zona de leña.");
            Zone(StructureKind.QuarryHut, "Caseta de cantera", "Caseta junto a la cantera.");
            // ZoneBlueprints: en la bocamina Variant = largo de la galería, y la huella es 3 + largo de fondo.
            Zone(StructureKind.MineEntrance, "Bocamina", "Entrada de mina con su galería hacia el norte: cuanto más fondo, más larga.")
                .Pack = (f, s) =>
                Math.Clamp(s.y - 3, ZoneBlueprints.GalleryRange.x, ZoneBlueprints.GalleryRange.y);
            Zone(StructureKind.Barn, "Granero", "Granero de la granja.");
            Zone(StructureKind.ShepherdHut, "Choza de pastor", "Choza del pastor junto al redil.");
            Zone(StructureKind.Fence, "Cerca", "Tramo de cerca de madera; abierta, sin interior.");
            Zone(StructureKind.Shed, "Cobertizo", "Cobertizo de aperos.");
            Zone(StructureKind.FishingHut, "Choza de pesca", "Choza de pescadores en la orilla.");
            Zone(StructureKind.SaltStore, "Almacén de sal", "Almacén de las salinas.");
            Zone(StructureKind.HunterLodge, "Cabaña de cazador", "Cabaña del coto de caza.");

            // ------------------------------------------------------------------ escuelas (SchoolModules)
            void School(StructureKind k, string label, string desc, bool design) => list.Add(new KindDef
            {
                Kind = k, Group = "school", Label = label, Description = desc,
                Fields = design
                    ? new[]
                    {
                        FieldDef.Element(), FieldDef.Choice("wing", "Uso del ala", 0, EnumOptions(
                            (SchoolWingDesign.Classrooms, "Aulas"), (SchoolWingDesign.Library, "Biblioteca"),
                            (SchoolWingDesign.Dormitory, "Dormitorio"), (SchoolWingDesign.Refectory, "Refectorio"),
                            (SchoolWingDesign.Laboratory, "Laboratorio"), (SchoolWingDesign.Observatory, "Observatorio"))),
                    }
                    : new[] { FieldDef.Element() },
                Pack = (f, s) => SchoolModules.Pack((MagicElement)f["element"], f["wing"]),
                SizeRange = v => Fixed(SchoolModules.Size(k)),
            });
            School(StructureKind.SchoolTower, "Torre de escuela", "Torre de esquina del campus de una escuela de magia.", false);
            School(StructureKind.SchoolWing, "Ala de escuela", "Ala lateral del campus: aulas, biblioteca, dormitorio…", true);
            School(StructureKind.SchoolGate, "Puerta de escuela", "Puerta principal del campus.", false);
            School(StructureKind.SchoolCourt, "Patio de escuela", "Patio central del campus, de una planta.", false);

            // ------------------------------------------------------------------ sitios salvajes (WildBlueprints, CampLayout)
            KindDef Wild(StructureKind k, string label, string desc, Func<int, (Vector2Int, Vector2Int)> size)
            {
                var d = new KindDef { Kind = k, Group = "wild", Label = label, Description = desc, SizeRange = size };
                list.Add(d);
                return d;
            }
            var tribe = FieldDef.Choice("tribe", "Tribu", WildBlueprints.OrcPalisade,
                (WildBlueprints.OrcPalisade, "Orca (alta)"), (WildBlueprints.GoblinPalisade, "Goblin (baja)"));
            Wild(StructureKind.Pyramid, "Pirámide", "Pirámide del desierto, con sus sótanos y la cámara del rey.",
                v => Fixed(new Vector2Int(PyramidLayout.Size, PyramidLayout.Size)));
            // CampLayout: tramos de empalizada de una celda de fondo y hasta OrcRun de largo; la puerta, de 4.
            var palisade = Wild(StructureKind.Palisade, "Empalizada", "Tramo de estacas afiladas de un campamento.",
                v => (new Vector2Int(1, 1), new Vector2Int(CampLayout.OrcRun, 1)));
            palisade.Fields = new[] { tribe };
            palisade.Pack = (f, s) => f["tribe"];
            var gate = Wild(StructureKind.PalisadeGate, "Puerta de empalizada", "Puerta del campamento, con dintel de cráneos.",
                v => Fixed(new Vector2Int(4, 1)));
            gate.Fields = new[] { tribe };
            gate.Pack = (f, s) => f["tribe"];
            var hall = Wild(StructureKind.OrcHall, "Casa larga orca", "Casa larga de los orcos; la del jefe es más grande.",
                v => Fixed(v == CampLayout.OrcHallChief ? CampLayout.ChiefHallSize : CampLayout.OrcHallSize));
            hall.Fields = new[]
            {
                FieldDef.Choice("hall", "Casa", CampLayout.OrcHallPlain, (CampLayout.OrcHallPlain, "Casa larga"),
                    (CampLayout.OrcHallChief, "Casa del jefe")),
            };
            hall.Pack = (f, s) => f["hall"];
            Wild(StructureKind.GoblinHut, "Choza goblin", "Choza de los goblins.", v => Fixed(CampLayout.GoblinHutSize));
            Wild(StructureKind.OrcTower, "Torre de vigía orca", "Torre de vigía del campamento orco.", v => Fixed(CampLayout.OrcTowerSize));
            Wild(StructureKind.OrcTent, "Tienda de pieles", "Tienda de pieles del campamento orco.", v => Fixed(CampLayout.OrcTentSize));
            Wild(StructureKind.OrcForge, "Fragua orca", "Fragua abierta del campamento orco.", v => Fixed(CampLayout.OrcForgeSize));
            Wild(StructureKind.GoblinTent, "Tienda de trapos", "Tienda de trapos del campamento goblin.", v => Fixed(CampLayout.GoblinTentSize));
            Wild(StructureKind.GoblinLookout, "Atalaya goblin", "Atalaya sobre zancos.", v => Fixed(CampLayout.GoblinLookoutSize));
            // CampLayout coloca el montón con rng.Next(4): cuatro mezclas.
            var junk = Wild(StructureKind.GoblinJunk, "Montón de chatarra", "Montón de chatarra goblin.", v => Fixed(CampLayout.GoblinJunkSize));
            junk.Fields = new[] { FieldDef.Range("mix", "Mezcla", 0, 3) };
            junk.Pack = (f, s) => f["mix"];

            // ------------------------------------------------------------------ barcos y antiguas
            list.Add(new KindDef
            {
                Kind = StructureKind.Boat, Group = "boat", Label = "Barco", Mode = BuildMode.Boat,
                Description = "Casco de barco (sin mástiles ni velas), con la proa al norte (+Z).",
                Fields = new[]
                {
                    FieldDef.Choice("boat", "Tamaño", 0, EnumOptions((BoatKind.Small, "Chalupa"), (BoatKind.Medium, "Bergantín"),
                        (BoatKind.Large, "Galeón"))),
                },
            });
            void Old(StructureKind k, string label, string desc) => list.Add(new KindDef
            {
                Kind = k, Group = "legacy", Label = label, Description = desc, Legacy = true, Mode = BuildMode.Legacy,
            });
            Old(StructureKind.House, "Casa antigua", "Casa suelta de los primeros mundos; la semilla decide el tamaño.");
            Old(StructureKind.Tower, "Torre antigua", "Torre suelta de los primeros mundos.");
            Old(StructureKind.Castle, "Castillo antiguo", "Castillo suelto de los primeros mundos.");
            Old(StructureKind.Ruin, "Ruina", "Casa o torre derruida, con musgo.");
            Old(StructureKind.Tent, "Tienda de campaña", "Tienda de lona de los asedios.");
#if AGAME_AUTHORED
            // Hechas a mano (las exportadas al compilar el puente): Variant = id.
            if (AuthoredCatalog.All.Count > 0)
                list.Add(new KindDef
                {
                    Kind = StructureKind.Authored, Group = "authored", Label = "Hecha a mano",
                    Description = "Estructura dibujada en el editor y exportada al juego; su tamaño es el suyo.",
                    Fields = new[]
                    {
                        FieldDef.Choice("id", "Estructura", AuthoredCatalog.All[0].Id,
                            AuthoredCatalog.All.Select(a => ((long)a.Id, a.Name)).ToArray()),
                    },
                    Pack = (f, s) => f["id"],
                    SizeRange = v => Fixed(AuthoredBlueprints.Find(v)?.Cells ?? Vector2Int.one),
                });
#endif
            return list;
        }

        // ------------------------------------------------------------------ cabecera del catálogo

        static object Rgb(Color32 c) => new[] { c.r, c.g, c.b }.Select(b => (int)b).ToArray();

        static object _catalog;

        /// <summary>La cabecera de <c>catalog</c> (sin id ni ok); se calcula una vez.</summary>
        public static object Header()
        {
            if (_catalog != null) return _catalog;
            int count = Enum.GetValues(typeof(StructureMaterial)).Cast<byte>().Max() + 1;
            var mats = Enumerable.Range(0, count).Select(i => (StructureMaterial)i).ToArray();
            // Los materiales de equipo son los que cambian de color con un equipo puesto (StructurePalette).
            var team = new Color32(1, 2, 3, 255);
            _catalog = new
            {
                materials = mats.Select(m => Enum.IsDefined(typeof(StructureMaterial), m) ? m.ToString() : "?").ToArray(),
                palette = mats.Select(m => Rgb(StructurePalette.Color(m))).ToArray(),
                elements = Enum.GetValues(typeof(MagicElement)).Cast<MagicElement>().Select(e => new
                {
                    name = e.ToString(), label = AGame.Localization.Lang.Resolve(MagicElements.DisplayName(e)),
                    palette = mats.Select(m => Rgb(StructurePalette.Color(m, (int)e))).ToArray(),
                }).ToArray(),
                teamMaterials = mats.Where(m => !StructurePalette.Color(m, -1, team).Equals(StructurePalette.Color(m)))
                    .Select(m => m.ToString()).ToArray(),
                styles = AllStyles.Select(s => new { name = s.ToString(), label = StyleLabel(s) }).ToArray(),
                facings = Enumerable.Range(0, 4).Select(d => new
                {
                    value = d, label = DirLabels[d], turns = StructureRotation.QuarterTurns(d),
                }).OrderBy(f => f.turns).ToArray(),
                damages = Enum.GetValues(typeof(StructureDamage)).Cast<StructureDamage>().Select(d => new
                {
                    name = d.ToString(), label = DamageLabels.TryGetValue(d.ToString(), out var l) ? l : d.ToString(),
                }).ToArray(),
                // Solo los grupos con algún tipo (las hechas a mano, si se han exportado).
                groups = Groups.Where(g => Defs.Any(d => d.Group == g.key)).Select(g => new { key = g.key, label = g.label }).ToArray(),
                kinds = Defs.Select(KindJson).ToArray(),
            };
            return _catalog;
        }

        static object KindJson(KindDef d)
        {
            var size = d.CatalogSize();
            return new
            {
                id = (int)d.Kind, name = d.Name, label = d.Label, group = d.Group, legacy = d.Legacy,
                description = d.Description,
                styles = d.Styles.Select(s => s.ToString()).ToArray(),
                size = size.HasValue
                    ? new { min = new[] { size.Value.min.x, size.Value.min.y }, max = new[] { size.Value.max.x, size.Value.max.y } }
                    : null,
                fields = d.Fields.Select(f => FieldJson(f, d)).ToArray(),
                partStates = HasParts(d),
            };
        }

        static Dictionary<string, object> FieldJson(FieldDef f, KindDef d)
        {
            var j = new Dictionary<string, object> { ["key"] = f.Key, ["label"] = f.Label, ["type"] = f.Type, ["default"] = f.Default };
            switch (f.Type)
            {
                case "index":
                    j["min"] = f.Min;
                    j["max"] = f.Max;
                    if (f.MaxForStyle != null)
                        j["maxByStyle"] = d.Styles.ToDictionary(s => s.ToString(), s => f.MaxForStyle(s));
                    break;
                case "range":
                    j["min"] = f.Min;
                    j["max"] = f.Max;
                    break;
                case "choice":
                    j["options"] = f.Options.Select(o => new { value = o.value, label = o.label }).ToArray();
                    break;
                case "mask":
                    j["bits"] = f.Bits.Select(b => new { bit = b.bit, label = b.label }).ToArray();
                    break;
            }
            return j;
        }

        /// <summary>¿Tiene partes móviles con sus valores por defecto? (se construye una vez, con la huella mínima).</summary>
        static bool HasParts(KindDef d)
        {
            try
            {
                var r = Builder.Run(BuildRequest.Default(d), checks: false);
                return r.Build.Parts != null && r.Build.Parts.Count > 0;
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"catálogo: {d.Name} no se construye con sus valores por defecto: {e.Message}");
                return false;
            }
        }
    }
}
