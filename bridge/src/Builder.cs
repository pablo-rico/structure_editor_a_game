// build (docs/contrato.md): de la petición a la StructureBuild del juego, y de ella a la cabecera y la carga binaria.

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using AGame.Core;
using AGame.Generation.WorldGen1.Structures;
using AGame.World.Boats;
using AGame.World.Structures;
using UnityEngine;

namespace AGame.StructureBridge
{
    /// <summary>Error de la petición con el mensaje en español llano para el artista.</summary>
    sealed class BridgeException : Exception
    {
        public BridgeException(string message) : base(message) { }
    }

    /// <summary>Petición <c>build</c> (los mismos campos que <c>BuildRequest</c> en proto.rs).</summary>
    sealed class BuildRequest
    {
        public string Kind = "Home", Style = "Northern", Damage = "Intact";
        public Dictionary<string, long> Fields = new Dictionary<string, long>();
        public int Seed = 1, Width, Depth, Facing = StructureRotation.DefaultFacing, Foundation = 1;
        public uint Dynamic, Party;

        /// <summary>El plano del juego aunque haya una sustitución hecha a mano (docs/contrato.md, fase 3).</summary>
        public bool Procedural;

        /// <summary>Valores por defecto del tipo, con la huella mínima.</summary>
        public static BuildRequest Default(KindDef d, int seed = 1)
        {
            var r = new BuildRequest { Kind = d.Name, Seed = seed };
            if (d.Styles.Length > 0) r.Style = d.Styles[0].ToString();
            foreach (var kv in d.Defaults(ArchStyle.Northern).All) r.Fields[kv.Key] = kv.Value;
            var size = d.CatalogSize();
            if (size.HasValue) (r.Width, r.Depth) = (size.Value.min.x, size.Value.min.y);
            return r;
        }

        public static BuildRequest Parse(JsonElement j)
        {
            var r = new BuildRequest();
            if (j.TryGetProperty("kind", out var v)) r.Kind = v.GetString();
            if (j.TryGetProperty("style", out v) && v.ValueKind == JsonValueKind.String) r.Style = v.GetString();
            if (j.TryGetProperty("damage", out v) && v.ValueKind == JsonValueKind.String) r.Damage = v.GetString();
            if (j.TryGetProperty("fields", out v) && v.ValueKind == JsonValueKind.Object)
                foreach (var p in v.EnumerateObject())
                    r.Fields[p.Name] = p.Value.ValueKind == JsonValueKind.True ? 1 : p.Value.ValueKind == JsonValueKind.False ? 0 : p.Value.GetInt64();
            int Int(string key, int def) => j.TryGetProperty(key, out var x) && x.ValueKind == JsonValueKind.Number ? x.GetInt32() : def;
            r.Seed = Int("seed", r.Seed);
            r.Width = Int("width", 0);
            r.Depth = Int("depth", 0);
            r.Facing = Int("facing", r.Facing);
            r.Foundation = Int("foundation", r.Foundation);
            r.Dynamic = j.TryGetProperty("dynamic", out v) && v.ValueKind == JsonValueKind.Number ? v.GetUInt32() : 0;
            r.Party = j.TryGetProperty("party", out v) && v.ValueKind == JsonValueKind.Number ? v.GetUInt32() : 0;
            r.Procedural = j.TryGetProperty("procedural", out v) && v.ValueKind == JsonValueKind.True;
            return r;
        }

        /// <summary>La petición tal y como la escribe el contrato (para <c>"request"</c> de las fixtures).</summary>
        public object Json() => new
        {
            kind = Kind, style = Style, fields = Fields, seed = Seed, width = Width, depth = Depth, facing = Facing,
            foundation = Foundation, damage = Damage, dynamic = Dynamic, party = Party,
        };
    }

    /// <summary>Una estructura construida, con lo que hace falta para la respuesta.</summary>
    sealed class BuildResult
    {
        public KindDef Def;
        public BuildRequest Request;
        public StructureBuild Build;
        public ArchStyle Style;
        public int Variant, Turns, Foundation;
        public int Theme = -1;
        public StructureDamage Damage;

        /// <summary>Huella sin girar con la que se ha construido (cero si la decide la semilla).</summary>
        public Vector2Int Size;

        public double Ms;
        public List<object> Checks = new List<object>();

        /// <summary><c>{file, name}</c> de la sustitución hecha a mano que se ha construido; null si es el plano del juego.</summary>
        public object Override;
    }

    static class Builder
    {
        public static BuildResult Run(BuildRequest req, bool checks = true)
        {
            var def = Catalog.Find(req.Kind) ??
                      throw new BridgeException($"No conozco el tipo de estructura «{req.Kind}».");
            var r = new BuildResult { Def = def, Request = req };
            var sw = Stopwatch.StartNew();

            r.Style = ArchStyle.Northern;
            if (def.Styles.Length > 0)
            {
                if (!Enum.TryParse(req.Style, out ArchStyle style) || !def.Styles.Contains(style))
                {
                    style = def.Styles[0];
                    Info(r, "style-adjusted", $"Este tipo no existe en el estilo «{req.Style}»: se ha hecho en {Catalog.StyleLabel(style)}.");
                }
                r.Style = style;
            }
            if (!Enum.TryParse(req.Damage, out StructureDamage damage)) damage = StructureDamage.Intact;
            r.Damage = def.Mode == BuildMode.Instance ? damage : StructureDamage.Intact;
            var f = def.Values(req.Fields, r.Style);
            r.Foundation = Math.Max(1, req.Foundation);
            if (def.Group == "wild" && r.Foundation < WildBlueprints.FoundationOf(def.Kind))
            {
                r.Foundation = WildBlueprints.FoundationOf(def.Kind);
                Info(r, "foundation-adjusted", $"Esta pieza lleva {r.Foundation} niveles de cimientos (sus sótanos van dentro).");
            }

            switch (def.Mode)
            {
                case BuildMode.Legacy:
                    r.Build = StructureBlueprints.Build(def.Kind, req.Seed, r.Foundation);
                    break;
                case BuildMode.Boat:
                    r.Build = BoatBlueprints.Build((BoatKind)f["boat"], req.Seed);
                    break;
                default:
                {
                    var asked = new Vector2Int(req.Width, req.Depth);
                    r.Variant = def.Pack(f, asked);
                    var (min, max) = def.SizeRange(r.Variant);
                    r.Size = Vector2Int.Max(min, Vector2Int.Min(max, asked));
                    if (r.Size != asked && min != max) // con una sola huella no hay nada que elegir
                        Info(r, "size-adjusted", $"El tamaño se ha ajustado a {r.Size.x}×{r.Size.y}: este diseño admite de " +
                                                 $"{min.x}×{min.y} a {max.x}×{max.y}.");
                    r.Variant = def.Pack(f, r.Size);
                    r.Turns = StructureRotation.QuarterTurns(req.Facing);
                    var rs = StructureRotation.RotateSize(r.Size, r.Turns);
                    // party llega sin girar (bit i = StructureNav.Directions[i] de la plantilla); la instancia lo quiere girado.
                    byte party = StructureRotation.RotateMask((byte)(req.Party & 15), r.Turns);
                    var inst = new StructureInstance(def.Kind, req.Seed, 0, 0, rs.x, rs.y, r.Foundation, r.Foundation,
                        r.Variant, req.Facing & 3, r.Style, party, damage, req.Dynamic);
#if AGAME_OVERRIDES
                    r.Build = StructureBlueprints.Build(inst, req.Procedural);
                    // Las mismas condiciones que el enganche de CityBlueprints.Construct (ni kit de murallas, ni parque, ni
                    // puerto; el palacio imperial siempre norteño).
                    if (!req.Procedural && CityBlueprints.IsCity(def.Kind) && !FortKit.IsFort(def.Kind) &&
                        def.Kind != StructureKind.Park && def.Kind != StructureKind.Harbour)
                    {
                        var style = def.Kind == StructureKind.ImperialPalace ? ArchStyle.Northern : r.Style;
                        var o = AuthoredBlueprints.OverrideFor(def.Kind, style, r.Variant, r.Size, (byte)(req.Party & 15));
                        if (o != null) r.Override = new { file = o.Source ?? "", name = o.Name };
                    }
#else
                    r.Build = StructureBlueprints.Build(inst);
#endif
                    if (SchoolModules.IsSchool(def.Kind)) r.Theme = (int)SchoolModules.ElementOf(r.Variant);
                    break;
                }
            }
            r.Ms = sw.Elapsed.TotalMilliseconds;
            if (checks) AGame.StructureBridge.Checks.Run(r);
            return r;
        }

        static void Info(BuildResult r, string code, string message) =>
            r.Checks.Add(new { severity = "info", code, message, at = (object)null });

        // ------------------------------------------------------------------ respuesta

        /// <summary>Cabecera de la respuesta de <c>build</c> (sin id ni ok) y la carga en el orden de <c>payload</c>.</summary>
        public static (Dictionary<string, object> header, byte[] payload) Encode(BuildResult r, bool withGrid = true)
        {
            var b = r.Build;
            var g = b.Grid;
            var nav = b.Nav;
            int cells = nav.CellsX * nav.CellsZ;

            // validate no devuelve la rejilla: ya la tiene el editor.
            var grid = withGrid ? g.Raw.ToArray() : Array.Empty<byte>();
            var detail = withGrid ? g.RawDetail.ToArray() : Array.Empty<byte>(); // vacío si no hay detalle
            var floors = new byte[cells * 8];
            var slabs = new byte[cells * 8];
            var edges = new byte[nav.Levels * cells];
            for (int cz = 0; cz < nav.CellsZ; cz++)
            for (int cx = 0; cx < nav.CellsX; cx++)
            {
                ulong fl = 0, sl = 0;
                for (int lv = 0; lv < nav.Levels; lv++)
                {
                    if (nav.HasFloor(cx, cz, lv)) fl |= 1UL << lv;
                    if (nav.HasSlab(cx, cz, lv)) sl |= 1UL << lv;
                    byte mask = 0;
                    for (int dir = 0; dir < 4; dir++)
                    {
                        if (nav.IsEdgeBlocked(cx, cz, dir, lv, lv)) mask |= (byte)(1 << dir);
                        if (nav.IsEdgeBlocked(cx, cz, dir, lv, lv + 1)) mask |= (byte)(1 << (dir + 4));
                    }
                    edges[(lv * nav.CellsZ + cz) * nav.CellsX + cx] = mask;
                }
                int i = cz * nav.CellsX + cx;
                BitConverter.TryWriteBytes(floors.AsSpan(i * 8), fl);
                BitConverter.TryWriteBytes(slabs.AsSpan(i * 8), sl);
            }
            if (!BitConverter.IsLittleEndian) throw new PlatformNotSupportedException("El puente escribe u64 LE");

            var header = new Dictionary<string, object>
            {
                ["ms"] = Math.Round(r.Ms, 1),
                ["size"] = new[] { g.SizeX, g.SizeY, g.SizeZ },
                ["groundLevel"] = b.GroundLevel,
                ["bandStarts"] = b.BandStarts,
                ["bandFloors"] = b.BandFloors,
                ["entrance"] = new[] { b.Entrance.x, b.Entrance.y },
                ["courtyard"] = b.Courtyard,
                ["theme"] = r.Theme >= 0 ? r.Theme : (int?)null,
                ["parts"] = Parts(r),
                ["interactables"] = (b.Interactables ?? Array.Empty<VoxelInteractable>()).Select(it => new
                {
                    kind = it.Kind.ToString(), box = new[] { it.X0, it.Y0, it.Z0, it.X1, it.Y1, it.Z1 }, arg = it.Arg,
                    facing = (int)it.Facing,
                }).ToArray(),
                ["footprint"] = Footprint(r),
                ["override"] = r.Override,
                ["nav"] = new { levels = nav.Levels, cellsX = nav.CellsX, cellsZ = nav.CellsZ, dirs = Catalog.DirNames },
                ["checks"] = r.Checks,
                ["payload"] = new
                {
                    grid = grid.Length, detail = detail.Length, navFloors = floors.Length, navSlabs = slabs.Length,
                    navEdges = edges.Length,
                },
            };
            var ms = new MemoryStream(grid.Length + detail.Length + floors.Length * 2 + edges.Length);
            ms.Write(grid);
            ms.Write(detail);
            ms.Write(floors);
            ms.Write(slabs);
            ms.Write(edges);
            return (header, ms.ToArray());
        }

        /// <summary>
        /// Partes con la caja girada al marco de la rejilla (las de <see cref="StructureBuild.Parts"/> están en el de la
        /// plantilla): mismas esquinas que <see cref="VoxelInteractables.Rotate(in VoxelInteractable, int, int, int)"/>.
        /// </summary>
        static object[] Parts(BuildResult r)
        {
            var b = r.Build;
            if (b.Parts == null) return Array.Empty<object>();
            int turns = r.Turns & 3;
            // Tamaño de la rejilla sin girar.
            int sx = (turns & 1) == 0 ? b.Grid.SizeX : b.Grid.SizeZ, sz = (turns & 1) == 0 ? b.Grid.SizeZ : b.Grid.SizeX;
            var list = new object[b.Parts.Count];
            for (int i = 0; i < b.Parts.Count; i++)
            {
                var p = b.Parts[i];
                var a = VoxelInteractables.RotateSub(p.X0, p.Z0, sx, sz, turns);
                var c = VoxelInteractables.RotateSub(p.X1 - 1, p.Z1 - 1, sx, sz, turns);
                var cell = b.PartCells != null && i < b.PartCells.Length ? b.PartCells[i] : StructureParts.CellOf(p);
                list[i] = new
                {
                    kind = p.Kind.ToString(), pattern = p.Pattern.ToString(), material = p.Material.ToString(),
                    box = new[] { Math.Min(a.x, c.x), p.Y0, Math.Min(a.y, c.y), Math.Max(a.x, c.x) + 1, p.Y1, Math.Max(a.y, c.y) + 1 },
                    swing = p.Swing, detail = p.Detail, startsOpen = p.StartsOpen, locked = p.Locked,
                    cell = new[] { cell.x, cell.y },
                };
            }
            return list;
        }

        /// <summary>Huella con huecos (<see cref="StructureFootprints"/>) en base64, índice <c>z·cellsX + x</c>; null si la llena.</summary>
        static string Footprint(BuildResult r)
        {
            if (r.Def.Mode != BuildMode.Instance || !StructureFootprints.IsMasked(r.Def.Kind, r.Variant)) return null;
            var cells = r.Build.Cells;
            var bytes = new byte[cells.x * cells.y];
            for (int z = 0; z < cells.y; z++)
            for (int x = 0; x < cells.x; x++)
                if (StructureFootprints.Covers(r.Def.Kind, r.Variant, r.Request.Facing & 3, cells.x, cells.y, x, z))
                    bytes[z * cells.x + x] = 1;
            return Convert.ToBase64String(bytes);
        }

        /// <summary>FNV-1a de 32 bits (paridad con Unity).</summary>
        public static uint Fnv1a(ReadOnlySpan<byte> data)
        {
            uint h = 2166136261;
            foreach (byte x in data) h = (h ^ x) * 16777619;
            return h;
        }
    }
}
