// El documento del editor (la cabecera de validate y de tool, docs/contrato.md): rejilla sin girar, franjas, entrada,
// partes, interactuables y, si sustituye a un diseño del juego, su «replaces». De aquí sale la respuesta de validate y
// de tool, con las comprobaciones de siempre y las de las sustituciones.

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using AGame.Core;
using AGame.Generation.WorldGen1.Structures;
using AGame.World.Structures;
using UnityEngine;

namespace AGame.StructureBridge
{
    /// <summary>Lectura de los trozos del contrato que comparten validate, tool y export.</summary>
    static class Json
    {
        public static T Named<T>(string name, string what, List<string> errors) where T : struct, Enum
        {
            // Solo nombres del enum (Enum.TryParse también aceptaría «5»).
            if (name != null && Enum.GetNames(typeof(T)).Contains(name)) return Enum.Parse<T>(name);
            errors.Add($"El juego no tiene {what} «{name}».");
            return default;
        }

        public static int[] Ints(JsonElement j) => j.EnumerateArray().Select(e => e.GetInt32()).ToArray();

        public static object Voxel(int[] box) => new { voxel = new[] { box[0], box[1], box[2] } };

        public static void Error(List<object> checks, string code, string message, object at) =>
            checks.Add(new { severity = "error", code, message, at });

        public static bool Bool(JsonElement j, string key) => j.TryGetProperty(key, out var x) && x.ValueKind == JsonValueKind.True;

        /// <summary>Partes del contrato (<c>Part</c> de proto.rs) a <see cref="VoxelPart"/>; las que no se entienden, a checks.</summary>
        public static List<VoxelPart> Parts(JsonElement j, List<object> checks)
        {
            var list = new List<VoxelPart>();
            if (j.ValueKind != JsonValueKind.Array) return list;
            foreach (var p in j.EnumerateArray())
            {
                var errors = new List<string>();
                var kind = Named<PartKind>(p.GetProperty("kind").GetString(), "la parte", errors);
                var pattern = Named<PartPattern>(p.TryGetProperty("pattern", out var v) ? v.GetString() : "Solid", "el dibujo de parte", errors);
                var material = Named<StructureMaterial>(p.GetProperty("material").GetString(), "el material", errors);
                var b = Ints(p.GetProperty("box"));
                if (b.Length != 6 || b[0] >= b[3] || b[1] >= b[4] || b[2] >= b[5]) errors.Add("La caja de la parte está vacía o mal formada.");
                if (errors.Count > 0)
                {
                    Error(checks, "bad-part", $"Parte {list.Count + 1}: {string.Join(" ", errors)}", b.Length >= 3 ? Voxel(b) : null);
                    continue;
                }
                int swing = p.TryGetProperty("swing", out v) ? v.GetInt32() : 1;
                list.Add(new VoxelPart(kind, pattern, material, b[0], b[1], b[2], b[3], b[4], b[5], swing, Bool(p, "detail"),
                    Bool(p, "startsOpen"), Bool(p, "locked")));
            }
            return list;
        }

        public static List<VoxelInteractable> Interactables(JsonElement j, List<object> checks)
        {
            var list = new List<VoxelInteractable>();
            if (j.ValueKind != JsonValueKind.Array) return list;
            foreach (var it in j.EnumerateArray())
            {
                var errors = new List<string>();
                var kind = Named<InteractKind>(it.GetProperty("kind").GetString(), "la pieza", errors);
                var b = Ints(it.GetProperty("box"));
                if (b.Length != 6) errors.Add("La caja de la pieza está mal formada.");
                if (errors.Count > 0)
                {
                    Error(checks, "bad-interactable", string.Join(" ", errors), b.Length >= 3 ? Voxel(b) : null);
                    continue;
                }
                int arg = it.TryGetProperty("arg", out var v) ? v.GetInt32() : 0;
                int facing = it.TryGetProperty("facing", out v) ? v.GetInt32() : 3;
                list.Add(new VoxelInteractable(kind, b[0], b[1], b[2], b[3], b[4], b[5], arg, (byte)(facing & 3)));
            }
            return list;
        }

        public static object PartJson(in VoxelPart p, Vector2Int cell) => new
        {
            kind = p.Kind.ToString(), pattern = p.Pattern.ToString(), material = p.Material.ToString(),
            box = new[] { p.X0, p.Y0, p.Z0, p.X1, p.Y1, p.Z1 }, swing = p.Swing, detail = p.Detail,
            startsOpen = p.StartsOpen, locked = p.Locked, cell = new[] { cell.x, cell.y },
        };

        public static object ItemJson(in VoxelInteractable it) => new
        {
            kind = it.Kind.ToString(), box = new[] { it.X0, it.Y0, it.Z0, it.X1, it.Y1, it.Z1 }, arg = it.Arg,
            facing = (int)it.Facing,
        };
    }

    /// <summary>
    /// Lo que sustituye una estructura a mano (<c>replaces</c> del <c>.agvox</c>, docs/contrato.md): tipo, estilo, diseño,
    /// huella y medianeras sin girar, y rango, barrio y clase (null = cualquiera).
    /// </summary>
    sealed class Replaces
    {
        public StructureKind Kind;
        public ArchStyle Style;
        public int Design;
        public Vector2Int Size;
        public byte Party;
        public int? Rank;
        public CityDistrict? District;
        public DistrictTier? Tier;

        /// <summary>El <c>Variant</c> del diseño, con los valores de siempre donde no fija nada (el juego la usa para todos).</summary>
        public int Variant => Kind == StructureKind.Home
            ? CityHouseVariant.Pack(Design, Rank ?? 1, District ?? CityDistrict.Residential, Tier ?? DistrictTier.Modest)
            : Kind == StructureKind.Shop
                ? CityHouseVariant.ForShop(Design, District ?? CityDistrict.Residential, Tier ?? DistrictTier.Modest)
                : Design;

        /// <summary>Clave de <c>duplicate-override</c>: dos fuentes con la misma sustituyen exactamente lo mismo.</summary>
        public string Key => $"{Kind}/{Style}/{Design}/{Size.x}x{Size.y}/{Party}/{Rank}/{District}/{Tier}";

        public string Label => $"{Catalog.Find(Kind.ToString())?.Label ?? Kind.ToString()} {Catalog.StyleLabel(Style)}";

        /// <summary>Null si no hay o es null; los nombres que no conoce el juego van a <paramref name="errors"/>.</summary>
        public static Replaces Parse(JsonElement j, List<string> errors)
        {
            if (j.ValueKind != JsonValueKind.Object) return null;
            var r = new Replaces
            {
                Kind = Json.Named<StructureKind>(j.GetProperty("kind").GetString(), "el tipo", errors),
                Style = Json.Named<ArchStyle>(j.TryGetProperty("style", out var s) ? s.GetString() : "Northern", "el estilo", errors),
                Design = j.TryGetProperty("design", out var d) && d.ValueKind == JsonValueKind.Number ? d.GetInt32() : 0,
                Party = (byte)(j.TryGetProperty("party", out var p) && p.ValueKind == JsonValueKind.Number ? p.GetInt32() & 15 : 0),
            };
            var size = Json.Ints(j.GetProperty("size"));
            if (size.Length == 2) r.Size = new Vector2Int(size[0], size[1]);
            else errors.Add("El tamaño de lo que sustituye está mal formado.");
            if (j.TryGetProperty("rank", out var rk) && rk.ValueKind == JsonValueKind.Number) r.Rank = rk.GetInt32();
            if (j.TryGetProperty("district", out var di) && di.ValueKind == JsonValueKind.String)
                r.District = Json.Named<CityDistrict>(di.GetString(), "el barrio", errors);
            if (j.TryGetProperty("tier", out var ti) && ti.ValueKind == JsonValueKind.String)
                r.Tier = Json.Named<DistrictTier>(ti.GetString(), "la clase", errors);
            if (errors.Count == 0 && !CityBlueprints.IsCity(r.Kind))
                errors.Add($"Solo se pueden sustituir edificios de ciudad, y {r.Kind} no lo es.");
            return r;
        }

        /// <summary>
        /// Las comprobaciones de una sustitución sobre su build sin girar (docs/contrato.md, «validate con replaces»):
        /// huella, puerta, medianeras ciegas, mostrador y patio.
        /// </summary>
        public void Check(StructureBuild b, int entranceX, bool courtyard, List<object> checks)
        {
            var cells = b.Cells;
            var (min, max) = CityBlueprints.SizeRange(Kind, Variant);
            if (cells != Size)
                Json.Error(checks, "override-size", $"Mide {cells.x}×{cells.y} celdas y lo que sustituye mide {Size.x}×{Size.y}.", null);
            else if (Size.x < min.x || Size.y < min.y || Size.x > max.x || Size.y > max.y)
                Json.Error(checks, "override-size", $"{Label} no se hace de {Size.x}×{Size.y}: va de {min.x}×{min.y} a {max.x}×{max.y}.", null);
            int door = CityBlueprints.DoorX(Kind, Style, Variant, Size);
            if (entranceX != door)
                Json.Error(checks, "override-door", $"La puerta tiene que ir en la celda {door} de la fachada, donde la pone el juego.",
                    new { cell = new[] { door, -1 }, level = b.GroundLevel });
            var hole = PartyHole(b.Grid, b.GroundLevel, Party);
            if (hole != null)
                Json.Error(checks, "override-party", "Medianera: este lado da a la casa de al lado y no puede tener huecos ni vidrio.",
                    new { voxel = hole });
            bool counter = Kind == StructureKind.Bank || Kind == StructureKind.Shop &&
                (CityHouseVariant.ShopStyleOf(Variant) == ShopStyle.Store || CityHouseVariant.ShopStyleOf(Variant) == ShopStyle.Inn);
            if (counter && !VendorSpots.TryFind(b, out _, out _))
                Json.Error(checks, "override-counter", "Falta el mostrador con sitio para el vendedor detrás: el juego lo necesita.", null);
            if (courtyard && !OriginalCourtyard())
                Json.Error(checks, "override-courtyard", $"{Label} no tiene patio en el juego: quita la marca de patio.", null);
        }

        /// <summary>¿Tiene patio el plano del juego que se sustituye? (solo lo admiten los que ya lo traen).</summary>
        bool OriginalCourtyard()
        {
            var s = new StructureInstance(Kind, 1, 0, 0, Size.x, Size.y, 1, 1, Variant, StructureRotation.DefaultFacing, Style, Party);
#if AGAME_OVERRIDES
            return StructureBlueprints.Build(s, procedural: true).Courtyard;
#else
            return StructureBlueprints.Build(s).Courtyard;
#endif
        }

        /// <summary>
        /// Primer sub-cubo que rompe una medianera de <paramref name="party"/> (bit i = dirs[i], sin girar; la fachada −Z
        /// nunca lo es): vidrio en sus dos capas de fuera, o hueco en la de fuera entre el suelo y lo más alto de su
        /// columna, fuera de la primera y la última celda del lado (como <c>CityBlueprintTests.PartyFacesHaveNoGlassNorOpenings</c>).
        /// </summary>
        public static int[] PartyHole(VoxelGrid g, int groundLevel, byte party)
        {
            const int S = WorldConstants.SubPerCell;
            for (int dir = 0; dir < 3; dir++)
            {
                if ((party >> dir & 1) == 0) continue;
                int len = dir < 2 ? g.SizeZ : g.SizeX;
                for (int a = 0; a < len; a++)
                for (int layer = 0; layer < 2; layer++)
                {
                    int x = dir == 0 ? g.SizeX - 1 - layer : dir == 1 ? layer : a;
                    int z = dir == 2 ? g.SizeZ - 1 - layer : a;
                    for (int y = 0; y < g.SizeY; y++)
                        if (g.Get(x, y, z) == StructureMaterial.Glass) return new[] { x, y, z };
                    if (layer > 0 || a < S || a >= len - S) continue;
                    int top = -1;
                    for (int y = g.SizeY - 1; y >= 0 && top < 0; y--)
                        if (g.IsSolid(x, y, z)) top = y;
                    for (int y = groundLevel * WorldConstants.SubPerLevel; y < top; y++)
                        if (!g.IsSolid(x, y, z)) return new[] { x, y, z };
                }
            }
            return null;
        }
    }

    /// <summary>El documento de una cabecera de validate o tool, con su carga (rejilla y detalle).</summary>
    sealed class Doc
    {
        public VoxelGrid Grid;
        public List<VoxelPart> Parts;
        public List<VoxelInteractable> Items;
        public int GroundLevel;
        public List<int> BandStarts, BandFloors;
        public int[] Entrance;
        public bool Courtyard;
        public uint Dynamic;
        public Replaces Replaces;

        /// <summary>Errores de la lectura (materiales, partes, entrada): salen con los checks de la respuesta.</summary>
        public readonly List<object> Checks = new List<object>();

        public static Doc Parse(JsonElement h, byte[] payload)
        {
            const int s = WorldConstants.SubPerCell;
            var doc = new Doc();
            var size = Json.Ints(h.GetProperty("size"));
            if (size.Length != 3 || size.Any(n => n <= 0) || size[0] % s != 0 || size[2] % s != 0)
                throw new BridgeException($"La rejilla tiene que medir celdas enteras ({s} sub-cubos) de ancho y de fondo.");
            int count = size[0] * size[1] * size[2];
            bool hasDetail = Json.Bool(h, "hasDetail");
            if (payload.Length != count * (hasDetail ? 2 : 1))
                throw new BridgeException($"La carga trae {payload.Length} bytes y la rejilla pide {count * (hasDetail ? 2 : 1)}.");

            var grid = new VoxelGrid(size[0], size[1], size[2]);
            int maxMaterial = Enum.GetValues(typeof(StructureMaterial)).Cast<byte>().Max(), unknown = 0;
            int[] firstUnknown = null;
            for (int i = 0; i < count; i++)
            {
                byte m = payload[i];
                bool detail = hasDetail && payload[count + i] != 0;
                if (m == 0 && !detail) continue;
                int x = i % size[0], row = i / size[0], z = row % size[2], y = row / size[2];
                if (m > maxMaterial && unknown++ == 0) firstUnknown = new[] { x, y, z };
                grid.MarkDetail = detail;
                grid.Set(x, y, z, (StructureMaterial)m);
            }
            grid.MarkDetail = false;
            doc.Grid = grid;
            if (unknown > 0)
                Json.Error(doc.Checks, "unknown-material",
                    $"Hay {(unknown == 1 ? "un sub-cubo" : unknown + " sub-cubos")} con un material que el juego no tiene.", Json.Voxel(firstUnknown));

            doc.Parts = Json.Parts(h.TryGetProperty("parts", out var pj) ? pj : default, doc.Checks);
            doc.Items = Json.Interactables(h.TryGetProperty("interactables", out var ij) ? ij : default, doc.Checks);
            doc.Dynamic = h.TryGetProperty("dynamic", out var dj) && dj.ValueKind == JsonValueKind.Number ? dj.GetUInt32() : 0;
            doc.GroundLevel = h.GetProperty("groundLevel").GetInt32();
            doc.BandStarts = Json.Ints(h.GetProperty("bandStarts")).ToList();
            doc.BandFloors = Json.Ints(h.GetProperty("bandFloors")).ToList();
            doc.Entrance = Json.Ints(h.GetProperty("entrance"));
            doc.Courtyard = Json.Bool(h, "courtyard");
            var errors = new List<string>();
            doc.Replaces = Replaces.Parse(h.TryGetProperty("replaces", out var rj) ? rj : default, errors);
            foreach (var e in errors) Json.Error(doc.Checks, "bad-replaces", e, null);
            if (errors.Count > 0) doc.Replaces = null;
            return doc;
        }

        public Vector2Int Cells => new Vector2Int(Grid.SizeX / WorldConstants.SubPerCell, Grid.SizeZ / WorldConstants.SubPerCell);

        /// <summary>
        /// La StructureBuild del juego con este documento y sus checks: las partes se pintan en su estado (<see cref="Dynamic"/>)
        /// como en el juego, y la navegación sale de esa rejilla.
        /// </summary>
        public BuildResult Result()
        {
            const int s = WorldConstants.SubPerCell;
            var r = new BuildResult { Def = Authored.Def, Request = new BuildRequest { Kind = Authored.Def.Name } };
            r.Checks.AddRange(Checks);
            StructureParts.Apply(Grid, Parts, Dynamic);
            var e = Entrance;
            if (e.Length != 2 || e[1] != -1 || e[0] < 0 || e[0] >= Grid.SizeX / s)
                Json.Error(r.Checks, "bad-entrance", "La entrada tiene que ser una celda de la fachada sur, justo fuera (z = −1).",
                    e.Length == 2 ? new { cell = e, level = GroundLevel } : null);
            try
            {
                r.Build = new StructureBuild(Authored.Kind, Grid, GroundLevel, BandStarts.ToArray(), BandFloors.ToArray(),
                    new Vector2Int(e.Length > 0 ? e[0] : 0, e.Length > 1 ? e[1] : -1), Courtyard);
            }
            catch (ArgumentException ex)
            {
                throw new BridgeException("Las plantas no casan: " + ex.Message);
            }
            if (Parts.Count > 0)
            {
                r.Build.Parts = Parts;
                r.Build.PartCells = Parts.Select(p => StructureParts.CellOf(p)).ToArray();
            }
            r.Build.Interactables = Items;
            Checks.Clear();
            AGame.StructureBridge.Checks.Run(r);
            Replaces?.Check(r.Build, e.Length > 0 ? e[0] : -1, Courtyard, r.Checks);
            return r;
        }
    }
}
