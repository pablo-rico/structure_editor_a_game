// Estructuras hechas a mano (docs/contrato.md, «validate», «export», «.agvox»): comprobar la rejilla que pinta el
// editor y exportar los .agvox al juego (a_game: Generation/WorldGen1/Structures/Authored).

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using AGame.Core;
using AGame.Generation.WorldGen1.Structures;
using AGame.World.Structures;
using UnityEngine;

namespace AGame.StructureBridge
{
    static class Authored
    {
#if AGAME_AUTHORED
        public const StructureKind Kind = StructureKind.Authored;
#else
        // a_game aún sin estructuras a mano: validate sigue funcionando con un tipo que no construye nada.
        public const StructureKind Kind = (StructureKind)byte.MaxValue;
#endif

        /// <summary>Lo que ven los checks de una estructura a mano: un edificio cerrado, sin estilo ni variantes.</summary>
        public static readonly KindDef Def = new KindDef
        {
            Kind = Kind, Group = "authored", Label = "Hecha a mano",
            Description = "Estructura dibujada a mano en el editor y exportada al juego.",
        };

        static T Named<T>(string name, string what, List<string> errors) where T : struct, Enum
        {
            // Solo nombres del enum (Enum.TryParse también aceptaría «5»).
            if (name != null && Enum.GetNames(typeof(T)).Contains(name)) return Enum.Parse<T>(name);
            errors.Add($"El juego no tiene {what} «{name}».");
            return default;
        }

        static int[] Ints(JsonElement j) => j.EnumerateArray().Select(e => e.GetInt32()).ToArray();

        static object Voxel(int[] box) => new { voxel = new[] { box[0], box[1], box[2] } };

        static void Error(List<object> checks, string code, string message, object at) =>
            checks.Add(new { severity = "error", code, message, at });

        /// <summary>Partes del contrato (<c>Part</c> de proto.rs) a <see cref="VoxelPart"/>; las que no se entienden, a checks.</summary>
        static List<VoxelPart> Parts(JsonElement j, List<object> checks)
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
                bool Bool(string key) => p.TryGetProperty(key, out var x) && x.ValueKind == JsonValueKind.True;
                if (errors.Count > 0)
                {
                    Error(checks, "bad-part", $"Parte {list.Count + 1}: {string.Join(" ", errors)}", b.Length >= 3 ? Voxel(b) : null);
                    continue;
                }
                int swing = p.TryGetProperty("swing", out v) ? v.GetInt32() : 1;
                list.Add(new VoxelPart(kind, pattern, material, b[0], b[1], b[2], b[3], b[4], b[5], swing, Bool("detail"),
                    Bool("startsOpen"), Bool("locked")));
            }
            return list;
        }

        static List<VoxelInteractable> Interactables(JsonElement j, List<object> checks)
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

        // ------------------------------------------------------------------ validate

        /// <summary>
        /// <c>validate</c>: la rejilla del editor (sin girar) con su cabecera → la misma respuesta que <c>build</c> sin la
        /// rejilla. Las partes se pintan en su estado (<c>dynamic</c>, 0 por defecto), como en el juego.
        /// </summary>
        public static BuildResult Validate(JsonElement h, byte[] payload)
        {
            var r = new BuildResult { Def = Def, Request = new BuildRequest { Kind = Def.Name } };
            var size = Ints(h.GetProperty("size"));
            const int s = WorldConstants.SubPerCell;
            if (size.Length != 3 || size.Any(n => n <= 0) || size[0] % s != 0 || size[2] % s != 0)
                throw new BridgeException($"La rejilla tiene que medir celdas enteras ({s} sub-cubos) de ancho y de fondo.");
            int n = size[0] * size[1] * size[2];
            bool hasDetail = h.TryGetProperty("hasDetail", out var hd) && hd.ValueKind == JsonValueKind.True;
            if (payload.Length != n * (hasDetail ? 2 : 1))
                throw new BridgeException($"La carga trae {payload.Length} bytes y la rejilla pide {n * (hasDetail ? 2 : 1)}.");

            var grid = new VoxelGrid(size[0], size[1], size[2]);
            int maxMaterial = Enum.GetValues(typeof(StructureMaterial)).Cast<byte>().Max(), unknown = 0;
            int[] firstUnknown = null;
            for (int i = 0; i < n; i++)
            {
                byte m = payload[i];
                bool detail = hasDetail && payload[n + i] != 0;
                if (m == 0 && !detail) continue;
                int x = i % size[0], row = i / size[0], z = row % size[2], y = row / size[2];
                if (m > maxMaterial && unknown++ == 0) firstUnknown = new[] { x, y, z };
                grid.MarkDetail = detail;
                grid.Set(x, y, z, (StructureMaterial)m);
            }
            grid.MarkDetail = false;
            if (unknown > 0)
                Error(r.Checks, "unknown-material", $"Hay {(unknown == 1 ? "un sub-cubo" : unknown + " sub-cubos")} con un material que el juego no tiene.", Voxel(firstUnknown));

            var parts = Parts(h.TryGetProperty("parts", out var pj) ? pj : default, r.Checks);
            var items = Interactables(h.TryGetProperty("interactables", out var ij) ? ij : default, r.Checks);
            uint dynamic = h.TryGetProperty("dynamic", out var dj) && dj.ValueKind == JsonValueKind.Number ? dj.GetUInt32() : 0;
            StructureParts.Apply(grid, parts, dynamic);

            var entrance = Ints(h.GetProperty("entrance"));
            int ground = h.GetProperty("groundLevel").GetInt32();
            if (entrance.Length != 2 || entrance[1] != -1 || entrance[0] < 0 || entrance[0] >= size[0] / s)
                Error(r.Checks, "bad-entrance", "La entrada tiene que ser una celda de la fachada sur, justo fuera (z = −1).",
                    entrance.Length == 2 ? new { cell = entrance, level = ground } : null);
            try
            {
                r.Build = new StructureBuild(Kind, grid, ground, Ints(h.GetProperty("bandStarts")), Ints(h.GetProperty("bandFloors")),
                    new Vector2Int(entrance[0], entrance.Length > 1 ? entrance[1] : -1),
                    h.TryGetProperty("courtyard", out var cy) && cy.ValueKind == JsonValueKind.True);
            }
            catch (ArgumentException e)
            {
                throw new BridgeException("Las plantas no casan: " + e.Message);
            }
            if (parts.Count > 0)
            {
                r.Build.Parts = parts;
                r.Build.PartCells = parts.Select(p => StructureParts.CellOf(p)).ToArray();
            }
            r.Build.Interactables = items;
            Checks.Run(r);
            return r;
        }

#if AGAME_AUTHORED
        // ------------------------------------------------------------------ export

        sealed class Source
        {
            public string File, Rel, Pascal, Name;
            public int? Id;
            public AuthoredDef Def;
            public readonly List<object> Checks = new List<object>();
            public bool Failed => Checks.Any(c => JsonSerializer.SerializeToElement(c).GetProperty("severity").GetString() == "error");
        }

        /// <summary>Carpeta del juego con los <c>.g.cs</c>, a partir de <c>&lt;a_game&gt;/ArtSource/Structures</c>.</summary>
        static string OutDir(string root) =>
            Path.Combine(root, "Assets", "_Game", "Scripts", "Generation", "WorldGen1", "Structures", "Authored", "Generated");

        /// <summary>
        /// <c>export</c>: lee todos los <c>.agvox</c> de <paramref name="sourceDir"/>, los decodifica con
        /// <see cref="AuthoredGrid"/> y los construye con <see cref="AuthoredBlueprints"/> en las cuatro orientaciones con
        /// los checks de siempre. Si ninguno tiene errores, escribe un <c>Authored&lt;Nombre&gt;.g.cs</c> por fuente y el
        /// <c>AuthoredCatalog.g.cs</c>, y borra los <c>.g.cs</c> huérfanos; si alguno falla, no escribe nada.
        /// </summary>
        public static object Export(string sourceDir)
        {
            if (string.IsNullOrEmpty(sourceDir) || !Directory.Exists(sourceDir))
                throw new BridgeException($"No encuentro la carpeta de estructuras «{sourceDir}».");
            string root = Path.GetFullPath(Path.Combine(sourceDir, "..", ".."));
            string outDir = OutDir(root);
            if (!Directory.Exists(Path.GetDirectoryName(outDir)))
                throw new BridgeException("Este a_game no tiene estructuras a mano (falta Generation/WorldGen1/Structures/Authored).");

            var sources = Directory.GetFiles(sourceDir, "*.agvox").OrderBy(f => f, StringComparer.Ordinal)
                .Select(f => Read(f, root)).ToList();
            foreach (var g in sources.Where(x => x.Id.HasValue).GroupBy(x => x.Id.Value).Where(g => g.Count() > 1))
                foreach (var x in g)
                    Error(x.Checks, "duplicate-id", $"El id {g.Key} lo tienen también: {string.Join(", ", g.Where(o => o != x).Select(o => o.Rel))}.", null);
            foreach (var g in sources.Where(x => x.Pascal != null).GroupBy(x => x.Pascal).Where(g => g.Count() > 1))
                foreach (var x in g)
                    Error(x.Checks, "duplicate-name", $"El nombre de fichero da la misma clase (Authored{g.Key}) que otra fuente: cámbiale el nombre.", null);
            foreach (var x in sources.Where(x => x.Def != null && !x.Failed)) Check(x);

            bool ok = sources.All(x => !x.Failed);
            var written = new List<string>();
            var removed = new List<string>();
            int unchanged = 0;
            if (ok)
            {
                var files = sources.ToDictionary(x => $"Authored{x.Pascal}.g.cs", Code);
                files["AuthoredCatalog.g.cs"] = CatalogCode(sources);
                Directory.CreateDirectory(outDir);
                foreach (var (name, code) in files)
                {
                    string path = Path.Combine(outDir, name);
                    if (File.Exists(path) && File.ReadAllText(path) == code) unchanged++;
                    else
                    {
                        File.WriteAllText(path, code);
                        written.Add(name);
                    }
                    // Unity las crearía al importar, pero puede no estar abierto: se commitean con su .cs.
                    if (!File.Exists(path + ".meta")) File.WriteAllText(path + ".meta", Meta());
                }
                foreach (var path in Directory.GetFiles(outDir, "Authored*.g.cs"))
                {
                    string name = Path.GetFileName(path);
                    if (files.ContainsKey(name)) continue;
                    File.Delete(path);
                    if (File.Exists(path + ".meta")) File.Delete(path + ".meta");
                    removed.Add(name);
                }
            }
            return new
            {
                exported = ok, outDir, written, unchanged, removed,
                sources = sources.Select(x => new { file = x.Rel, id = x.Id, name = x.Name, checks = x.Checks }).ToArray(),
            };
        }

        static Source Read(string file, string root)
        {
            var x = new Source { File = file, Rel = Path.GetRelativePath(root, file).Replace('\\', '/') };
            x.Pascal = Pascal(Path.GetFileNameWithoutExtension(file));
            try
            {
                using var doc = JsonDocument.Parse(File.ReadAllBytes(file));
                var j = doc.RootElement;
                if (!j.TryGetProperty("format", out var f) || f.GetString() != "agvox") throw new FormatException("no es un .agvox (format)");
                if (j.GetProperty("version").GetInt32() > 1) throw new FormatException("es de una versión más nueva del editor");
                x.Name = j.GetProperty("name").GetString();
                if (j.TryGetProperty("id", out var idj) && idj.ValueKind == JsonValueKind.Number) x.Id = idj.GetInt32();
                if (x.Id == null || x.Id <= 0)
                    Error(x.Checks, "missing-id", "No tiene id: el editor se lo pone (el máximo + 1) antes de exportar.", null);

                var errors = new List<string>();
                var style = Named<ArchStyle>(j.TryGetProperty("style", out var sj) ? sj.GetString() : "Northern", "el estilo", errors);
                var materials = j.GetProperty("materials").EnumerateArray()
                    .Select(m => Named<StructureMaterial>(m.GetString(), "el material", errors)).ToArray();
                foreach (var e in errors) Error(x.Checks, "unknown-name", e, null);
                var parts = Parts(j.GetProperty("parts"), x.Checks);
                var items = Interactables(j.GetProperty("interactables"), x.Checks);
                var bands = j.GetProperty("bands").EnumerateArray().Select(Ints).ToArray();
                var size = Ints(j.GetProperty("size"));
                if (x.Failed) return x;
                x.Def = new AuthoredDef(x.Id.Value, x.Name, style, size[0], size[1], size[2], j.GetProperty("groundLevel").GetInt32(),
                    bands.Select(b => b[0]).ToArray(), bands.Select(b => b[1]).ToArray(), j.GetProperty("entranceX").GetInt32(),
                    j.TryGetProperty("courtyard", out var cy) && cy.ValueKind == JsonValueKind.True, materials,
                    j.GetProperty("voxels").GetString(),
                    j.TryGetProperty("detail", out var dj) && dj.ValueKind == JsonValueKind.String ? dj.GetString() : null,
                    parts.ToArray(), items.ToArray());
                AuthoredGrid.Decode(x.Def); // el RLE, con el decodificador del juego
            }
            catch (Exception e) when (e is FormatException || e is ArgumentException || e is JsonException ||
                                      e is KeyNotFoundException || e is InvalidOperationException || e is IndexOutOfRangeException)
            {
                x.Def = null;
                Error(x.Checks, "bad-file", $"No se puede leer: {e.Message}", null);
            }
            return x;
        }

        /// <summary>Las cuatro orientaciones con los checks de siempre; las de las giradas, solo si dicen algo nuevo.</summary>
        static void Check(Source x)
        {
            var seen = new HashSet<string>();
            foreach (int facing in new[] { StructureRotation.DefaultFacing, 0, 2, 1 })
            {
                int turns = StructureRotation.QuarterTurns(facing);
                var rs = StructureRotation.RotateSize(x.Def.Cells, turns);
                int g = Math.Max(1, x.Def.GroundLevel);
                var r = new BuildResult
                {
                    Def = Def, Variant = x.Def.Id, Turns = turns, Foundation = g, Size = x.Def.Cells,
                    Request = new BuildRequest { Kind = Def.Name, Facing = facing, Width = x.Def.Cells.x, Depth = x.Def.Cells.y },
                };
                try
                {
                    r.Build = StructureBlueprints.Build(
                        new StructureInstance(Kind, 1, 0, 0, rs.x, rs.y, g, g, x.Def.Id, facing), x.Def);
                }
                catch (Exception e)
                {
                    Error(x.Checks, "build-failed", $"Ha fallado al construirse hacia {Catalog.DirNames[facing]}: {e.Message}", null);
                    return;
                }
                Checks.Run(r);
                foreach (var c in r.Checks)
                {
                    var e = JsonSerializer.SerializeToElement(c);
                    string sev = e.GetProperty("severity").GetString(), code = e.GetProperty("code").GetString();
                    if (sev == "info" || !seen.Add(code)) continue;
                    if (turns == 0) x.Checks.Add(c);
                    else
                        x.Checks.Add(new
                        {
                            severity = sev, code, at = (object)null,
                            message = $"Con la puerta hacia {Catalog.DirNames[facing]}: {e.GetProperty("message").GetString()}",
                        });
                }
            }
        }

        /// <summary>De «torre-del-puerto» a «TorreDelPuerto» (letras y cifras ASCII; las tildes y la ñ, sin ellas).</summary>
        static string Pascal(string slug)
        {
            var sb = new StringBuilder();
            bool up = true;
            const string accented = "áéíóúüñÁÉÍÓÚÜÑ", plain = "aeiouunAEIOUUN";
            foreach (char ch in slug)
            {
                int k = accented.IndexOf(ch);
                char c = k >= 0 ? plain[k] : ch;
                if (c < 128 && char.IsLetterOrDigit(c))
                {
                    sb.Append(up ? char.ToUpperInvariant(c) : c);
                    up = false;
                }
                else up = true;
            }
            if (sb.Length == 0) return null;
            if (char.IsDigit(sb[0])) sb.Insert(0, 'S');
            return sb.ToString();
        }

        static string Literal(string s)
        {
            var sb = new StringBuilder("\"");
            foreach (char c in s ?? "")
                if (c == '"' || c == '\\') sb.Append('\\').Append(c);
                else if (c < ' ') sb.Append($"\\u{(int)c:x4}");
                else sb.Append(c);
            return sb.Append('"').ToString();
        }

        static string Xml(string s) => (s ?? "").Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;");

        /// <summary>Un base64 largo en trozos de una línea (sin cadenas de concatenación hondas para el compilador).</summary>
        static string Chunks(string b64, string indent)
        {
            const int n = 100;
            var sb = new StringBuilder("string.Concat(new[]\n" + indent + "{\n");
            for (int i = 0; i < b64.Length; i += n)
                sb.Append(indent).Append("    \"").Append(b64, i, Math.Min(n, b64.Length - i)).Append("\",\n");
            return sb.Append(indent).Append("})").ToString();
        }

        static string Header(string what) =>
            "// <auto-generated>\n" +
            $"// {what}\n" +
            "// Lo escribe el editor de estructuras al exportar (structure_editor_a_game, docs/contrato.md): no se edita a mano.\n" +
            "// </auto-generated>\n\n";

        static string Code(Source x)
        {
            var d = x.Def;
            const string I = "            ";
            var sb = new StringBuilder(Header($"Fuente: {x.Rel}"));
            sb.Append("using AGame.World.Structures;\nusing M = AGame.World.Structures.StructureMaterial;\n\n");
            sb.Append("namespace AGame.Generation.WorldGen1.Structures\n{\n");
            sb.Append($"    /// <summary>«{Xml(d.Name)}» (id {d.Id}), hecha a mano.</summary>\n");
            sb.Append($"    public static class Authored{x.Pascal}\n    {{\n");
            sb.Append("        public static readonly AuthoredDef Def = new AuthoredDef(\n");
            sb.Append($"{I}id: {d.Id},\n{I}name: {Literal(d.Name)},\n{I}style: ArchStyle.{d.Style},\n");
            sb.Append($"{I}sizeX: {d.SizeX}, sizeY: {d.SizeY}, sizeZ: {d.SizeZ},\n{I}groundLevel: {d.GroundLevel},\n");
            sb.Append($"{I}bandStarts: new int[] {{ {string.Join(", ", d.BandStarts)} }},\n");
            sb.Append($"{I}bandFloors: new int[] {{ {string.Join(", ", d.BandFloors)} }},\n");
            sb.Append($"{I}entranceX: {d.EntranceX},\n{I}courtyard: {(d.Courtyard ? "true" : "false")},\n");
            sb.Append($"{I}materials: new StructureMaterial[] {{ {string.Join(", ", d.Materials.Select(m => "M." + m))} }},\n");
            sb.Append($"{I}voxels: {Chunks(d.Voxels, I)},\n");
            sb.Append($"{I}detail: {(d.Detail == null ? "null" : Chunks(d.Detail, I))},\n");
            sb.Append($"{I}parts: new VoxelPart[]\n{I}{{\n");
            foreach (var p in d.Parts)
                sb.Append($"{I}    new VoxelPart(PartKind.{p.Kind}, PartPattern.{p.Pattern}, M.{p.Material}, {p.X0}, {p.Y0}, {p.Z0}, " +
                          $"{p.X1}, {p.Y1}, {p.Z1}, swing: {p.Swing}, detail: {B(p.Detail)}, startsOpen: {B(p.StartsOpen)}, " +
                          $"locked: {B(p.Locked)}),\n");
            sb.Append($"{I}}},\n{I}interactables: new VoxelInteractable[]\n{I}{{\n");
            foreach (var it in d.Interactables)
                sb.Append($"{I}    new VoxelInteractable(InteractKind.{it.Kind}, {it.X0}, {it.Y0}, {it.Z0}, {it.X1}, {it.Y1}, {it.Z1}, " +
                          $"arg: {it.Arg}, facing: {it.Facing}),\n");
            sb.Append($"{I}}});\n    }}\n}}\n");
            return sb.ToString();
        }

        static string B(bool b) => b ? "true" : "false";

        static string CatalogCode(List<Source> sources)
        {
            var sb = new StringBuilder(Header("Catálogo de las estructuras hechas a mano (docs/ciudades.md, «Estructuras a mano»): todos los .agvox de ArtSource/Structures."));
            sb.Append("using System.Collections.Generic;\n\nnamespace AGame.Generation.WorldGen1.Structures\n{\n");
            sb.Append("    public static class AuthoredCatalog\n    {\n");
            sb.Append("        /// <summary>Todas las estructuras hechas a mano, por id.</summary>\n");
            sb.Append("        public static readonly IReadOnlyList<AuthoredDef> All = new AuthoredDef[]\n        {\n");
            foreach (var x in sources.OrderBy(x => x.Id)) sb.Append($"            Authored{x.Pascal}.Def,\n");
            sb.Append("        };\n    }\n}\n");
            return sb.ToString();
        }

        /// <summary>.meta de un script nuevo (formato MonoImporter), con un GUID nuevo.</summary>
        static string Meta() =>
            $"fileFormatVersion: 2\nguid: {Guid.NewGuid():N}\nMonoImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n" +
            "  defaultReferences: []\n  executionOrder: 0\n  icon: {instanceID: 0}\n  userData: \n  assetBundleName: \n" +
            "  assetBundleVariant: \n";
#else
        public static object Export(string sourceDir) =>
            throw new BridgeException("Este a_game aún no tiene estructuras a mano: compila el puente con la rama que las trae.");
#endif
    }
}
