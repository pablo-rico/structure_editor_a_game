// AGame.StructureBridge (docs/contrato.md): construye los planos de a_game para el editor de estructuras.
//   serve            tramas por stdin/stdout (stdout solo lleva tramas; los registros van por stderr)
//   selftest [--hashes]  todas las combinaciones del catálogo; sale con 1 si alguna falla
//   dump <carpeta>   escribe las fixtures del contrato

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using AGame.Localization;
using AGame.Shared;
using AGame.World.Structures;

namespace AGame.StructureBridge
{
    static class Program
    {
        public const int Protocol = 1;

        static readonly JsonSerializerOptions Compact = new JsonSerializerOptions
        {
            Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        };

        static readonly JsonSerializerOptions Pretty = new JsonSerializerOptions
        {
            Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping, WriteIndented = true,
        };

        static int Main(string[] args)
        {
            // stdout es de las tramas: cualquier Console.Write o Log del juego va a stderr.
            var stdout = Console.OpenStandardOutput();
            Console.SetOut(Console.Error);
            Log.SetSink(new StderrLog());
            // Los nombres del juego pueden venir empaquetados para traducir (Lang.Pack): el editor es en español.
            Lang.Use(Lang.Fallback, Path.Combine(Meta("AGameRoot"), "Assets", "StreamingAssets", "lang"));
            var mode = args.Length > 0 ? args[0] : "";
            try
            {
                switch (mode)
                {
                    case "serve": return Serve(Console.OpenStandardInput(), stdout);
                    case "selftest": return SelfTest(new StreamWriter(stdout) { AutoFlush = true }, args.Contains("--hashes"));
                    case "dump" when args.Length > 1: return Dump(args[1]);
                }
            }
            catch (Exception e)
            {
                Console.Error.WriteLine(e);
                return 2;
            }
            Console.Error.WriteLine("Uso: AGame.StructureBridge serve | selftest [--hashes] | dump <carpeta>");
            return 2;
        }

        sealed class StderrLog : ILog
        {
            public void Info(string message) => Console.Error.WriteLine(message);
            public void Warn(string message) => Console.Error.WriteLine("AVISO: " + message);
            public void Error(string message) => Console.Error.WriteLine("ERROR: " + message);
        }

        static string Meta(string key) => typeof(Program).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(a => a.Key == key)?.Value ?? "";

        // ------------------------------------------------------------------ serve

        static int Serve(Stream input, Stream output)
        {
            var reader = new BufferedStream(input, 1 << 16);
            Console.Error.WriteLine($"puente listo (protocolo {Protocol}, a_game en {Meta("AGameRoot")})");
            while (true)
            {
                var line = ReadLine(reader);
                if (line == null) return 0; // el editor cerró stdin
                if (line.Length == 0) continue;
                long id = 0;
                try
                {
                    using var doc = JsonDocument.Parse(line);
                    var req = doc.RootElement;
                    if (req.TryGetProperty("id", out var idv)) id = idv.GetInt64();
                    byte[] payload = Array.Empty<byte>();
                    if (req.TryGetProperty("payloadLen", out var pl) && pl.GetInt64() > 0)
                        payload = ReadExactly(reader, checked((int)pl.GetInt64()));
                    string op = req.TryGetProperty("op", out var opv) ? opv.GetString() : "";
                    switch (op)
                    {
                        case "hello":
                            Write(output, id, new { protocol = Protocol, agameRoot = Meta("AGameRoot"), unityVersion = Meta("UnityVersion") });
                            break;
                        case "catalog":
                            Write(output, id, Catalog.Header());
                            break;
                        case "build":
                        {
                            var r = Builder.Run(BuildRequest.Parse(req));
                            var (header, bytes) = Builder.Encode(r);
                            Write(output, id, header, bytes);
                            break;
                        }
                        case "validate":
                        {
                            var (header, bytes) = Builder.Encode(Authored.Validate(req, payload), withGrid: false);
                            Write(output, id, header, bytes);
                            break;
                        }
                        case "export":
                            Write(output, id, Authored.Export(req.TryGetProperty("sourceDir", out var sd) ? sd.GetString() : null));
                            break;
                        case "shutdown":
                            Write(output, id, new { });
                            return 0;
                        default:
                            Fail(output, id, $"El puente no sabe hacer «{op}».", line.Length > 200 ? line.Substring(0, 200) : line);
                            break;
                    }
                }
                catch (BridgeException e)
                {
                    Fail(output, id, e.Message, e.ToString());
                }
                catch (Exception e)
                {
                    Console.Error.WriteLine(e);
                    Fail(output, id, "Esta combinación ha fallado al generarse.", e.ToString());
                }
            }
        }

        /// <summary>Una línea (sin el \n) en UTF-8; null al final del flujo.</summary>
        static string ReadLine(Stream s)
        {
            var buf = new MemoryStream();
            while (true)
            {
                int b = s.ReadByte();
                if (b < 0) return buf.Length == 0 ? null : Encoding.UTF8.GetString(buf.ToArray());
                if (b == '\n') return Encoding.UTF8.GetString(buf.ToArray()).TrimEnd('\r');
                buf.WriteByte((byte)b);
            }
        }

        static byte[] ReadExactly(Stream s, int n)
        {
            var data = new byte[n];
            s.ReadExactly(data, 0, n);
            return data;
        }

        /// <summary>Respuesta: u32 LE largo de la cabecera · cabecera · u32 LE largo de la carga · carga.</summary>
        static void Write(Stream output, long id, object body, byte[] payload = null, bool ok = true)
        {
            var o = new JsonObject { ["id"] = id, ["ok"] = ok };
            var node = JsonSerializer.SerializeToNode(body, Compact).AsObject();
            foreach (var key in node.Select(kv => kv.Key).ToList())
            {
                var value = node[key];
                node.Remove(key);
                o[key] = value;
            }
            var header = Encoding.UTF8.GetBytes(o.ToJsonString(Compact));
            payload ??= Array.Empty<byte>();
            output.Write(BitConverter.GetBytes((uint)header.Length));
            output.Write(header);
            output.Write(BitConverter.GetBytes((uint)payload.Length));
            output.Write(payload);
            output.Flush();
        }

        static void Fail(Stream output, long id, string message, string detail) =>
            Write(output, id, new { error = new { message, detail } }, ok: false);

        // ------------------------------------------------------------------ muestras (fixtures y paridad)

        static BuildRequest Sample(string kind, string style, int seed, int w, int d, params (string, long)[] fields)
        {
            var r = new BuildRequest { Kind = kind, Style = style, Seed = seed, Width = w, Depth = d, Foundation = 1 };
            foreach (var (k, v) in fields) r.Fields[k] = v;
            return r;
        }

        /// <summary>Las cinco muestras fijas (docs/contrato.md, «Fixtures»): también las de paridad con Unity.</summary>
        static readonly (string name, BuildRequest req)[] Samples =
        {
            ("home_northern", Sample("Home", "Northern", 1234, 5, 5, ("design", 2), ("rank", 1),
                ("district", (long)CityDistrict.Residential), ("tier", (long)DistrictTier.Modest))),
            ("shop_arabic", Sample("Shop", "Arabic", 4321, 6, 7, ("shop", (long)ShopStyle.Store),
                ("district", (long)CityDistrict.Market), ("tier", (long)DistrictTier.WellOff))),
            // Salidas +X, −X y +Z (bit i = StructureNav.Directions[i]).
            ("walltower_ports", Sample("WallTower", "Northern", 77, 6, 6, ("rise", 1), ("scaffold", 0), ("ports", 0b0111),
                ("doorRight", 0))),
            ("schooltower", Sample("SchoolTower", "Northern", 99, 16, 16, ("element", (long)AGame.Core.Magic.MagicElement.Ignis))),
            ("harbour", Sample("Harbour", "Northern", 5, 5, 5, ("piece", (long)HarbourPiece.HarbourHut))),
        };

        static int Dump(string dir)
        {
            Directory.CreateDirectory(dir);
            File.WriteAllText(Path.Combine(dir, "catalog.json"), JsonSerializer.Serialize(Catalog.Header(), Pretty) + "\n");
            foreach (var (name, req) in Samples)
            {
                var r = Builder.Run(req);
                var (header, payload) = Builder.Encode(r);
                header["ms"] = 0.0; // las fixtures no cambian de un volcado a otro
                header["request"] = req.Json();
                File.WriteAllText(Path.Combine(dir, name + ".json"), JsonSerializer.Serialize(header, Pretty) + "\n");
                File.WriteAllBytes(Path.Combine(dir, name + ".bin"), payload);
                Console.Error.WriteLine($"{name}: {r.Build.Grid.SizeX}×{r.Build.Grid.SizeY}×{r.Build.Grid.SizeZ}, " +
                                        $"{r.Build.Parts?.Count ?? 0} partes, {r.Build.Interactables.Count} interactuables, " +
                                        $"{r.Checks.Count} avisos, {r.Ms:0.0} ms, FNV-1a {Builder.Fnv1a(r.Build.Grid.Raw):x8}");
            }
            return 0;
        }

        // ------------------------------------------------------------------ selftest

        static int SelfTest(TextWriter o, bool hashes)
        {
            if (hashes)
            {
                foreach (var (name, req) in Samples)
                {
                    var r = Builder.Run(req, checks: false);
                    o.WriteLine($"{name} {Builder.Fnv1a(r.Build.Grid.Raw):x8}");
                }
                o.WriteLine();
                o.WriteLine("// Lo mismo dentro de Unity (unity command eval):");
                o.WriteLine(UnitySnippet());
                return 0;
            }
            var sw = Stopwatch.StartNew();
            int total = 0, failed = 0;
            var slow = new List<(double ms, string what)>();
            foreach (var def in Catalog.Kinds)
            {
                var kindWatch = Stopwatch.StartNew();
                int kindCount = 0;
                foreach (var (req, facings) in Cases(def))
                    foreach (int facing in facings)
                    {
                        req.Facing = facing;
                        total++;
                        kindCount++;
                        string what = $"{def.Name} {req.Style} {string.Join(",", req.Fields.Select(kv => kv.Key + "=" + kv.Value))} " +
                                      $"{req.Width}×{req.Depth} semilla {req.Seed} hacia {facing}";
                        try
                        {
                            var r = Builder.Run(req);
                            slow.Add((r.Ms, what));
                            var errors = r.Checks.Select(c => JsonSerializer.SerializeToElement(c))
                                .Where(c => c.GetProperty("severity").GetString() == "error").ToList();
                            if (errors.Count == 0) continue;
                            failed++;
                            foreach (var c in errors)
                                o.WriteLine($"FALLO {what}: {c.GetProperty("code").GetString()}: {c.GetProperty("message").GetString()}");
                        }
                        catch (Exception e)
                        {
                            failed++;
                            o.WriteLine($"FALLO {what}: excepción {e.GetType().Name}: {e.Message}");
                        }
                    }
                Console.Error.WriteLine($"{def.Name}: {kindCount} combinaciones, {kindWatch.Elapsed.TotalSeconds:0.0} s");
            }
            foreach (var (ms, what) in slow.OrderByDescending(s => s.ms).Take(5))
                Console.Error.WriteLine($"lenta: {ms:0} ms {what}");
            o.WriteLine($"selftest: {total} construcciones, {failed} con fallos, {sw.Elapsed.TotalSeconds:0.0} s");
            return failed == 0 ? 0 : 1;
        }

        /// <summary>
        /// Lo que recorre el selftest, como CityBlueprintTests.BlueprintIsSound: cada estilo × combinación de campos ×
        /// huella mínima (las cuatro orientaciones) y máxima (sin girar). Sin huella (antiguas y barcos), tres semillas.
        /// </summary>
        static IEnumerable<(BuildRequest req, int[] facings)> Cases(KindDef def)
        {
            var styles = def.Styles.Length > 0 ? def.Styles : new[] { ArchStyle.Northern };
            foreach (var style in styles)
            foreach (var f in def.Combos(style))
            {
                BuildRequest Req(int w, int d, int seed = 1000)
                {
                    var r = new BuildRequest { Kind = def.Name, Style = style.ToString(), Width = w, Depth = d, Foundation = 2 };
                    r.Seed = seed + (int)def.Kind * 97 + w * 7 + d;
                    foreach (var kv in f.All) r.Fields[kv.Key] = kv.Value;
                    return r;
                }
                if (def.SizeRange == null)
                {
                    for (int seed = 1; seed <= 3; seed++) yield return (Req(0, 0, seed), new[] { 3 });
                    continue;
                }
                // La bocamina saca el Variant del fondo: el mínimo y el máximo de la galería.
                foreach (int depthHint in def.Kind == StructureKind.MineEntrance ? new[] { 0, 1000 } : new[] { 0 })
                {
                    var (min, max) = def.SizeRange(def.Pack(f, new UnityEngine.Vector2Int(0, depthHint)));
                    yield return (Req(min.x, min.y), new[] { 3, 0, 2, 1 });
                    if (max != min) yield return (Req(max.x, max.y), new[] { 3 });
                }
            }
        }

        /// <summary>El fragmento C# para <c>unity command eval</c> que calcula los mismos FNV-1a dentro de Unity.</summary>
        static string UnitySnippet()
        {
            var sb = new StringBuilder();
            sb.Append("uint F(System.ReadOnlySpan<byte> d){uint h=2166136261;foreach(var x in d)h=(h^x)*16777619;return h;} ");
            sb.Append("string H(AGame.World.Structures.StructureInstance s)=>F(AGame.Generation.WorldGen1.Structures.StructureBlueprints.Build(s).Grid.Raw).ToString(\"x8\"); ");
            sb.Append("return string.Join(\" \", new[]{");
            sb.Append(string.Join(",", Samples.Select(s =>
            {
                var r = Builder.Run(s.req, checks: false);
                var rs = AGame.World.Structures.StructureRotation.RotateSize(r.Size, r.Turns);
                byte party = AGame.World.Structures.StructureRotation.RotateMask((byte)(s.req.Party & 15), r.Turns);
                return $"\"{s.name}=\"+H(new AGame.World.Structures.StructureInstance((AGame.World.Structures.StructureKind){(int)r.Def.Kind}," +
                       $"{s.req.Seed},0,0,{rs.x},{rs.y},{r.Foundation},{r.Foundation},{r.Variant},{s.req.Facing & 3}," +
                       $"(AGame.World.Structures.ArchStyle){(int)r.Style},{party},(AGame.World.Structures.StructureDamage)0,{s.req.Dynamic}u))";
            })));
            sb.Append("});");
            return sb.ToString();
        }
    }
}
