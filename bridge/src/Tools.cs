// Herramientas de construcción del modo Editar (docs/contrato.md, «look», «tools», «tool»): adaptadores finos sobre los
// pinceles del juego (CityBrush, CultureBrush, SacredBrush, FortArt), con sus convenciones: suelo de la planta a F
// (sub-cubos), forjado en F − 1, planta siguiente en F + 16, muros de T por dentro de la huella con la capa exterior a un
// sub-cubo del borde (como CityHouse). Los materiales salen de los papeles (look) que manda el editor.

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using AGame.Core;
using AGame.Generation.WorldGen1.Structures;
using AGame.World.Structures;
using UnityEngine;
using M = AGame.World.Structures.StructureMaterial;

namespace AGame.StructureBridge
{
    static class Tools
    {
        const int S = CityBrush.S, T = CityBrush.T, Storey = CityBrush.Storey;

        // ------------------------------------------------------------------ look

        public static readonly string[] Roles =
            { "wall", "timber", "plinth", "quoin", "roof", "ridge", "verge", "frame", "shutter", "floor", "leaf", "trim", "chimney", "dome" };

        /// <summary>
        /// Papeles de material de una cultura, barrio y clase: los campos del estilo de las casas del juego
        /// (<c>HouseStyle</c>, <c>MedStyle</c>, <c>PatioStyle</c>) tras <c>DistrictLook.Apply(st, barrio, clase, 0)</c>.
        /// Donde el estilo no tiene el papel, el material con el que lo pinta su plano (<c>CityHouse</c>, <c>MedHouse</c>,
        /// <c>PatioHouse</c>): tablas de los forjados, marcos de las ventanas…
        /// </summary>
        public static Dictionary<string, M> Look(ArchStyle style, CityDistrict d, DistrictTier t)
        {
            switch (style)
            {
                case ArchStyle.Mediterranean:
                {
                    var st = new MedStyle();
                    DistrictLook.Apply(st, d, t, 0);
                    var trim = st.Trim == M.Air ? st.Wall : st.Trim;
                    return new Dictionary<string, M>
                    {
                        ["wall"] = st.Wall, ["timber"] = st.Leaf, ["plinth"] = st.Plinth, ["quoin"] = trim, ["roof"] = st.RoofMat,
                        ["ridge"] = st.RoofMat, ["verge"] = M.Air, ["frame"] = trim, ["shutter"] = st.Shutter, ["floor"] = M.Plank,
                        ["leaf"] = st.Leaf, ["trim"] = st.DoorTrim, ["chimney"] = st.Wall, ["dome"] = M.Terracotta,
                    };
                }
                case ArchStyle.Arabic:
                {
                    var st = new PatioStyle();
                    DistrictLook.Apply(st, d, t, 0);
                    return new Dictionary<string, M>
                    {
                        ["wall"] = st.Wall, ["timber"] = st.Leaf, ["plinth"] = st.Plinth, ["quoin"] = st.Trim, ["roof"] = st.Trim,
                        ["ridge"] = st.Trim, ["verge"] = M.Air, ["frame"] = st.Trim, ["shutter"] = st.Leaf, ["floor"] = M.Plank,
                        ["leaf"] = st.Leaf, ["trim"] = st.DoorTrim, ["chimney"] = st.Wall, ["dome"] = st.DomeMat,
                    };
                }
                default:
                {
                    var st = new HouseStyle();
                    DistrictLook.Apply(st, d, t, 0);
                    return new Dictionary<string, M>
                    {
                        ["wall"] = st.Infill, ["timber"] = st.Timber, ["plinth"] = st.Ground, ["quoin"] = st.Quoin, ["roof"] = st.RoofMat,
                        ["ridge"] = st.Ridge, ["verge"] = st.Verge, ["frame"] = st.Timber, ["shutter"] = st.Shutter, ["floor"] = M.Plank,
                        ["leaf"] = M.Plank, ["trim"] = st.Quoin, ["chimney"] = st.ChimneyMat, ["dome"] = M.Lead,
                    };
                }
            }
        }

        public static object LookJson(JsonElement req)
        {
            var errors = new List<string>();
            var style = Json.Named<ArchStyle>(Str(req, "style", "Northern"), "el estilo", errors);
            var d = Json.Named<CityDistrict>(Str(req, "district", "Residential"), "el barrio", errors);
            var t = Json.Named<DistrictTier>(Str(req, "tier", "Modest"), "la clase", errors);
            if (errors.Count > 0) throw new BridgeException(string.Join(" ", errors));
            return new { roles = Look(style, d, t).ToDictionary(kv => kv.Key, kv => kv.Value.ToString()) };
        }

        static string Str(JsonElement j, string key, string def) =>
            j.TryGetProperty(key, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : def;

        // ------------------------------------------------------------------ catálogo de herramientas

        sealed class Tool
        {
            public string Id, Label, Icon, Target;
            public FieldDef[] Params = Array.Empty<FieldDef>();
            public Action<Ctx> Run;
        }

        static FieldDef Storeys() => FieldDef.Range("storeys", "Nº de plantas", 1, 4, 1);

        static FieldDef Walls() => FieldDef.Choice("walls", "Muro", 0, (0, "Entramado"), (1, "Piedra"), (2, "Liso (cal, adobe)"));

        static FieldDef Side() => FieldDef.Choice("side", "Lado de la celda", 0, (0, "Sur u oeste"), (1, "Norte o este"));

        static FieldDef Bool(string key, string label, bool on)
        {
            var f = FieldDef.Bool(key, label);
            f.Default = on ? 1 : 0;
            return f;
        }

        static FieldDef Arch(long def) => FieldDef.Choice("arch", "Arco", def, (0, "Adintelado"), (1, "De medio punto"),
            (2, "Rebajado"), (3, "De herradura"), (4, "Apuntado"));

        static readonly Tool[] All =
        {
            new Tool
            {
                Id = "room", Label = "Habitación", Icon = "HOUSE_LINE", Target = "rect", Run = Room,
                Params = new[] { Storeys(), Walls(), Bool("floor", "Con suelo", true), Bool("ceiling", "Con techo", true) },
            },
            new Tool { Id = "wall", Label = "Muro", Icon = "WALL", Target = "line", Run = Wall, Params = new[] { Storeys(), Walls(), Side() } },
            new Tool
            {
                Id = "floor", Label = "Suelo", Icon = "CHECKERBOARD", Target = "rect", Run = Floor,
                Params = new[] { Bool("stone", "De piedra", false), Bool("edge", "Hasta el borde", false) },
            },
            new Tool
            {
                Id = "roof", Label = "Tejado", Icon = "TRIANGLE", Target = "rect", Run = Roof,
                Params = new[]
                {
                    FieldDef.Choice("shape", "Forma", (long)RoofShape.GableZ,
                        ((long)RoofShape.GableZ, "Dos aguas, hastial a la calle"), ((long)RoofShape.GableX, "Dos aguas, alero a la calle"),
                        ((long)RoofShape.Hip, "Cuatro aguas"), ((long)RoofShape.HalfHipZ, "Copete, hastial a la calle"),
                        ((long)RoofShape.HalfHipX, "Copete, alero a la calle"), ((long)RoofShape.Mansard, "Mansarda"),
                        ((long)RoofShape.Lean, "Un agua, sube al norte"), ((long)RoofShape.LeanNegZ, "Un agua, sube al sur"),
                        ((long)RoofShape.LeanPosX, "Un agua, sube al este"), ((long)RoofShape.LeanNegX, "Un agua, sube al oeste"),
                        ((long)RoofShape.Spire, "Chapitel")),
                    FieldDef.Range("pitch", "Pendiente", 1, 4, 3),
                    Bool("gables", "Cerrar los hastiales", true),
                    Bool("framed", "Hastial de entramado", true),
                },
            },
            new Tool
            {
                Id = "window", Label = "Ventana", Icon = "SQUARES_FOUR", Target = "face", Run = Window,
                Params = new[]
                {
                    FieldDef.Choice("kind", "Tipo", 0, (0, "Ventana"), (1, "Con arco"), (2, "Saetera"), (3, "Rosetón")),
                    FieldDef.Range("width", "Ancho (sub-cubos)", 2, 6, 4),
                    Bool("shutters", "Contraventanas", true),
                    Bool("flowers", "Jardinera", false),
                    Arch(1),
                    Bool("stained", "Vidriera", false),
                },
            },
            new Tool
            {
                Id = "door", Label = "Puerta", Icon = "DOOR_OPEN", Target = "face", Run = Door,
                Params = new[] { Arch(2), Bool("leaf", "Con hoja (se abre y se cierra)", true) },
            },
            new Tool
            {
                Id = "stairs", Label = "Escalera", Icon = "STAIRS", Target = "cell", Run = Stairs,
                Params = new[]
                {
                    FieldDef.Range("floors", "Plantas que sube", 1, 4, 1),
                    FieldDef.Choice("turn", "Al subir otra, vuelve hacia", 0, (0, "La izquierda"), (1, "La derecha")),
                    Bool("rail", "Barandilla", true),
                },
            },
            new Tool
            {
                Id = "chimney", Label = "Chimenea", Icon = "FLAME", Target = "cell", Run = Chimney,
                Params = new[] { FieldDef.Range("above", "Sobresale del tejado (sub-cubos)", 2, 12, 5) },
            },
            new Tool
            {
                Id = "crenels", Label = "Almenas", Icon = "CASTLE_TURRET", Target = "rect", Run = Crenels,
                Params = new[] { Bool("ring", "En los cuatro lados", true), Side() },
            },
            new Tool
            {
                Id = "dome", Label = "Cúpula", Icon = "MOSQUE", Target = "rect", Run = Dome,
                Params = new[]
                {
                    FieldDef.Choice("rise", "Forma", 100, (100, "Media naranja"), (130, "Peraltada"), (70, "Rebajada")),
                    Bool("ribs", "Nervios", false), Bool("finial", "Remate dorado", true),
                },
            },
            new Tool
            {
                Id = "spire", Label = "Aguja", Icon = "CHURCH", Target = "rect", Run = Spire,
                Params = new[]
                {
                    FieldDef.Range("height", "Alto (sub-cubos)", 8, 96, 32), Bool("octagonal", "Ochavada", true),
                    Bool("edges", "Aristas de otro material", false), Bool("cross", "Cruz en la punta", true),
                },
            },
        };

        public static object Catalog() => new
        {
            tools = All.Select(t => new
            {
                id = t.Id, label = t.Label, icon = t.Icon, target = t.Target,
                @params = t.Params.Select(f => StructureBridge.Catalog.FieldJson(f, null)).ToArray(),
            }).ToArray(),
        };

        // ------------------------------------------------------------------ aplicar

        /// <summary>Lo que ve una herramienta: el documento, sus parámetros, los papeles y el objetivo.</summary>
        sealed class Ctx
        {
            public Doc Doc;
            public FieldValues P;
            public Dictionary<string, M> Roles;
            public ArchStyle Style;
            public int F;
            public int Cx0, Cz0, Cx1, Cz1; // rect (celdas, incluidas)
            public Vector2Int Cell;
            public int Dir;
            public readonly System.Random Rng = new System.Random(0);
            CityBrush _b;
            int _registered;

            public M R(string role) => Roles[role];
            public VoxelGrid Grid => Doc.Grid;
            public int X0 => Cx0 * S;
            public int X1 => (Cx1 + 1) * S;
            public int Z0 => Cz0 * S;
            public int Z1 => (Cz1 + 1) * S;

            /// <summary>Crece la rejilla hacia arriba hasta <paramref name="y"/> sub-cubos (en niveles enteros). Antes de pintar.</summary>
            public void Grow(int y)
            {
                if (_b != null) throw new InvalidOperationException("Grow después de pintar");
                const int l = WorldConstants.SubPerLevel;
                if (y <= Grid.SizeY) return;
                int sy = (y + l - 1) / l * l;
                if (sy > 63 * l) throw new BridgeException("No cabe: la estructura pasaría de 63 niveles de alto.");
                var g = new VoxelGrid(Grid.SizeX, sy, Grid.SizeZ);
                var raw = Grid.Raw;
                var det = Grid.RawDetail;
                for (int i = 0; i < raw.Length; i++)
                {
                    bool d = det.Length > 0 && det[i] != 0;
                    if (raw[i] == 0 && !d) continue;
                    int x = i % Grid.SizeX, row = i / Grid.SizeX, z = row % Grid.SizeZ, yy = row / Grid.SizeZ;
                    g.MarkDetail = d;
                    g.Set(x, yy, z, (M)raw[i]);
                }
                g.MarkDetail = false;
                Doc.Grid = g;
            }

            /// <summary>El pincel sobre la rejilla, con las partes del documento ya registradas (cuentan para MaxParts).</summary>
            public CityBrush B
            {
                get
                {
                    if (_b != null) return _b;
                    foreach (var p in Doc.Parts) Grid.AddPart(p);
                    _registered = Grid.Interactables?.Count ?? 0;
                    _b = new CityBrush(Grid, Rng);
                    return _b;
                }
            }

            /// <summary>Lleva al documento las partes y las piezas que han puesto los pinceles.</summary>
            public void Finish()
            {
                if (_b == null) return;
                Doc.Parts = Grid.Parts?.ToList() ?? new List<VoxelPart>();
                if (Grid.Interactables != null) Doc.Items.AddRange(Grid.Interactables.Skip(_registered));
            }

            /// <summary>Con el rectángulo de una sola fila o columna: ¿va a lo largo de X?</summary>
            public bool LineAlongX()
            {
                if (Cx0 != Cx1 && Cz0 != Cz1) throw new BridgeException("Arrastra en línea recta: una celda de ancho.");
                return Cz0 == Cz1;
            }

            /// <summary>Añade las franjas de las plantas nuevas (suelo en <paramref name="y"/>) para el corte por plantas.</summary>
            public void AddBand(int y)
            {
                int level = y / WorldConstants.SubPerLevel;
                if (y - 1 > Doc.BandStarts[^1] && level > Doc.BandFloors[^1])
                {
                    Doc.BandStarts.Add(y - 1);
                    Doc.BandFloors.Add(level);
                }
            }
        }

        /// <summary>
        /// <c>tool</c>: aplica la herramienta <c>name</c> al documento y devuelve el resultado como un <c>build</c> (con la
        /// rejilla nueva). Lo que no se puede hacer es una <see cref="BridgeException"/> con el porqué en español.
        /// </summary>
        public static BuildResult Apply(JsonElement h, byte[] payload)
        {
            var watch = System.Diagnostics.Stopwatch.StartNew();
            var doc = Doc.Parse(h, payload);
            string name = Str(h, "name", "");
            var tool = All.FirstOrDefault(t => t.Id == name) ?? throw new BridgeException($"No hay ninguna herramienta «{name}».");
            if (doc.Parts.Count > StructureParts.MaxParts)
                throw new BridgeException($"El documento ya tiene {doc.Parts.Count} partes móviles y caben {StructureParts.MaxParts}.");
            var raw = new Dictionary<string, long>();
            if (h.TryGetProperty("params", out var pj) && pj.ValueKind == JsonValueKind.Object)
                foreach (var p in pj.EnumerateObject())
                    raw[p.Name] = p.Value.ValueKind == JsonValueKind.True ? 1 : p.Value.ValueKind == JsonValueKind.False ? 0 : p.Value.GetInt64();
            var errors = new List<string>();
            var style = Json.Named<ArchStyle>(Str(h, "style", doc.Replaces?.Style.ToString() ?? "Northern"), "el estilo", errors);
            var roles = Look(style, CityDistrict.Residential, DistrictTier.Modest);
            if (h.TryGetProperty("roles", out var rj) && rj.ValueKind == JsonValueKind.Object)
                foreach (var r in rj.EnumerateObject())
                    roles[r.Name] = Json.Named<M>(r.Value.GetString(), $"el material del papel «{r.Name}»", errors);
            if (errors.Count > 0) throw new BridgeException(string.Join(" ", errors));
            var c = new Ctx
            {
                Doc = doc, P = new KindDef { Fields = tool.Params }.Values(raw, style), Roles = roles, Style = style,
                F = h.TryGetProperty("floorY", out var fy) ? fy.GetInt32() : doc.GroundLevel * WorldConstants.SubPerLevel,
            };
            if (c.F < 1 || c.F >= 63 * WorldConstants.SubPerLevel) throw new BridgeException("La planta de trabajo está fuera de la estructura.");
            ReadTarget(c, tool.Target, h.TryGetProperty("target", out var tj) ? tj : default);
            tool.Run(c);
            c.Finish();
            var result = doc.Result();
            result.Ms = watch.Elapsed.TotalMilliseconds;
            return result;
        }

        static void ReadTarget(Ctx c, string kind, JsonElement t)
        {
            var cells = c.Doc.Cells;
            if (t.ValueKind != JsonValueKind.Object) throw new BridgeException("Falta dónde aplicar la herramienta.");
            if (kind == "rect" || kind == "line")
            {
                var r = t.TryGetProperty("rect", out var rr) ? Json.Ints(rr) : Array.Empty<int>();
                if (r.Length != 4) throw new BridgeException("Arrastra un rectángulo de celdas.");
                (c.Cx0, c.Cx1) = (Math.Min(r[0], r[2]), Math.Max(r[0], r[2]));
                (c.Cz0, c.Cz1) = (Math.Min(r[1], r[3]), Math.Max(r[1], r[3]));
                if (c.Cx0 < 0 || c.Cz0 < 0 || c.Cx1 >= cells.x || c.Cz1 >= cells.y)
                    throw new BridgeException("El rectángulo se sale de la estructura.");
                return;
            }
            var cell = t.TryGetProperty("cell", out var cj) ? Json.Ints(cj) : Array.Empty<int>();
            if (cell.Length != 2) throw new BridgeException("Haz clic en una celda.");
            c.Cell = new Vector2Int(cell[0], cell[1]);
            c.Dir = t.TryGetProperty("dir", out var dj) ? dj.GetInt32() & 3 : 3;
            if (c.Cell.x < 0 || c.Cell.y < 0 || c.Cell.x >= cells.x || c.Cell.y >= cells.y)
                throw new BridgeException("Esa celda está fuera de la estructura.");
        }

        // ------------------------------------------------------------------ las herramientas

        /// <summary>Un piso de muro en la cara según el tipo elegido (entramado, piedra o liso).</summary>
        static void WallStorey(Ctx c, Face face, int y, int top, int kind, bool ground)
        {
            var b = c.B;
            switch (kind)
            {
                case 0:
                    b.FramedStorey(face, y, top, c.R("wall"), c.R("timber"), null);
                    if (ground) face.Box(b.Grid, face.A0, face.A1, y, y + 1, 0, T, c.R("plinth"));
                    break;
                case 1: b.StoneStorey(face, y, top, c.R("plinth"), c.R("quoin"), M.Air); break;
                default:
                    face.Box(b.Grid, face.A0, face.A1, y, top, 0, T, c.R("wall"));
                    if (ground) face.Box(b.Grid, face.A0, face.A1, y, y + 3, -1, T, c.R("plinth"));
                    break;
            }
        }

        static void Room(Ctx c)
        {
            int n = c.P["storeys"], kind = c.P["walls"];
            c.Grow(c.F + n * Storey);
            var b = c.B;
            int x0 = c.X0 + 1, x1 = c.X1 - 1, z0 = c.Z0 + 1, z1 = c.Z1 - 1;
            if (x1 - x0 < 2 * T + 1 || z1 - z0 < 2 * T + 1) throw new BridgeException("La habitación necesita al menos una celda.");
            for (int k = 0; k < n; k++)
            {
                int y = c.F + k * Storey, top = y + Storey - 1;
                b.Box(x0 + T, y, z0 + T, x1 - T, top, z1 - T, M.Air);
                if (k > 0 || c.P.On("floor")) b.Box(x0, y - 1, z0, x1, y, z1, c.R("floor"));
                if (k > 0 && kind == 0) b.Ring(x0, z0, x1, z1, y - 1, y, c.R("timber"));
                for (int dir = 0; dir < 4; dir++) WallStorey(c, Face.Of(dir, x0, z0, x1, z1), y, top, kind, k == 0);
                if (k > 0) c.AddBand(y);
            }
            if (c.P.On("ceiling"))
            {
                int y = c.F + n * Storey - 1;
                b.Box(x0, y, z0, x1, y + 1, z1, c.R("floor"));
                if (kind == 0) b.Ring(x0, z0, x1, z1, y, y + 1, c.R("timber"));
            }
        }

        static void Wall(Ctx c)
        {
            bool alongX = c.LineAlongX();
            int n = c.P["storeys"], kind = c.P["walls"];
            bool far = c.P["side"] == 1;
            c.Grow(c.F + n * Storey);
            var face = alongX
                ? new Face(far ? 2 : 3, far ? c.Z1 - 2 : c.Z0 + 1, c.X0, c.X1)
                : new Face(far ? 0 : 1, far ? c.X1 - 2 : c.X0 + 1, c.Z0, c.Z1);
            for (int k = 0; k < n; k++)
            {
                int y = c.F + k * Storey;
                WallStorey(c, face, y, y + Storey - 1, kind, k == 0);
                if (k > 0) c.AddBand(y);
            }
        }

        static void Floor(Ctx c)
        {
            int inset = c.P.On("edge") ? 0 : 1;
            c.B.Box(c.X0 + inset, c.F - 1, c.Z0 + inset, c.X1 - inset, c.F, c.Z1 - inset, c.R(c.P.On("stone") ? "plinth" : "floor"));
        }

        static void Roof(Ctx c)
        {
            var shape = (RoofShape)c.P["shape"];
            int pitch = c.P["pitch"];
            int wallTop = c.F + Storey, baseY = wallTop - (pitch + 1) / 2;
            int x0 = c.X0, x1 = c.X1, z0 = c.Z0, z1 = c.Z1;
            var probe = new RoofField(x0, z0, x1, z1, baseY, shape, pitch, (pitch + 1) / 2 + 1);
            c.Grow(probe.Peak + 1);
            var b = c.B;
            var roof = b.Roof(x0, z0, x1, z1, baseY, shape, pitch, c.R("roof"), c.R("ridge"), c.R("verge"));
            if (!c.P.On("gables")) return;
            var front = new Face(3, z0 + 1, x0 + 1, x1 - 1);
            var back = new Face(2, z1 - 2, x0 + 1, x1 - 1);
            var left = new Face(1, x0 + 1, z0 + 1, z1 - 1);
            var right = new Face(0, x1 - 2, z0 + 1, z1 - 1);
            bool framed = c.P.On("framed");
            if (shape == RoofShape.GableZ || shape == RoofShape.HalfHipZ)
                foreach (var f in new[] { front, back }) b.Gable(f, roof, wallTop, c.R("wall"), c.R("timber"), framed);
            else if (shape == RoofShape.GableX || shape == RoofShape.HalfHipX)
                foreach (var f in new[] { left, right }) b.Gable(f, roof, wallTop, c.R("wall"), c.R("timber"), framed);
            // Como CityHouse.BuildRoof: cierra lo que queda entre lo alto de los muros y el faldón, donde hay muro.
            for (int z = z0 + 1; z < z1 - 1; z++)
            for (int x = x0 + 1; x < x1 - 1; x++)
            {
                bool rim = x < x0 + 3 || x >= x1 - 3 || z < z0 + 3 || z >= z1 - 3;
                if (!rim || !b.Grid.IsSolid(x, wallTop - 1, z)) continue;
                int under = roof.Under(x, z);
                for (int y = wallTop; y < under; y++)
                    if (b.Get(x, y, z) == M.Air) b.Set(x, y, z, c.R("wall"));
            }
        }

        /// <summary>
        /// La cara del muro que se ha pinchado: la primera capa sólida desde el borde <c>dir</c> de la celda hacia dentro, a
        /// la altura <paramref name="y"/> en el centro de la celda. Con una sustitución, una medianera no admite huecos.
        /// </summary>
        static Face WallFace(Ctx c, int y, string what)
        {
            var cell = c.Cell;
            int dir = c.Dir;
            var rp = c.Doc.Replaces;
            var cells = c.Doc.Cells;
            bool edge = dir == 0 ? cell.x == cells.x - 1 : dir == 1 ? cell.x == 0 : dir == 2 ? cell.y == cells.y - 1 : cell.y == 0;
            if (rp != null && edge && CultureBrush.IsParty(rp.Party, dir))
                throw new BridgeException("Medianera: aquí no puede haber huecos (este lado da a la casa de al lado).");
            int a = (dir < 2 ? cell.y : cell.x) * S + S / 2;
            for (int k = 0; k < S; k++)
            {
                int n = dir == 0 ? cell.x * S + S - 1 - k : dir == 1 ? cell.x * S + k : dir == 2 ? cell.y * S + S - 1 - k : cell.y * S + k;
                bool solid = dir < 2 ? c.Grid.IsSolid(n, y, a) : c.Grid.IsSolid(a, y, n);
                if (!solid) continue;
                int bay = dir < 2 ? cell.y : cell.x;
                return new Face(dir, n, bay * S, bay * S + S);
            }
            throw new BridgeException($"Pon {what} sobre un muro: en esa cara de la celda no hay ninguno a esa altura.");
        }

        static ArchKind ArchOf(int v) => (ArchKind)Math.Clamp(v, 0, 4);

        static void Window(Ctx c)
        {
            int kind = c.P["kind"], width = c.P["width"];
            var face = WallFace(c, c.F + CityBrush.Sill + 2, "la ventana");
            var b = c.B;
            int bay = (c.Dir < 2 ? c.Cell.y : c.Cell.x), mid = bay * S + S / 2;
            switch (kind)
            {
                case 0:
                    b.Window(face, bay, c.F, c.R("frame"), c.P.On("shutters") ? c.R("shutter") : M.Air, c.P.On("flowers"), width);
                    break;
                case 1:
                    b.ArchWindow(face, mid - width / 2, width, c.F + CityBrush.Sill, CityBrush.WinH + 3, ArchOf(c.P["arch"]),
                        c.R("frame"), c.P.On("stained"));
                    break;
                case 2:
                    b.Slit(face, mid - Math.Min(2, width) / 2, c.F + 4, Math.Min(2, width), 8, c.R("frame"));
                    break;
                default:
                {
                    int r = Math.Max(3, width + 1);
                    b.RoseWindow(face, mid, c.F + r + 3, r, c.R("frame"));
                    break;
                }
            }
        }

        static void Door(Ctx c)
        {
            bool leaf = c.P.On("leaf");
            if (leaf && c.Doc.Parts.Count >= StructureParts.MaxParts)
                throw new BridgeException($"Ya hay {StructureParts.MaxParts} partes móviles: quita alguna puerta o pon esta sin hoja.");
            var face = WallFace(c, c.F + 4, "la puerta");
            var b = c.B;
            int bay = c.Dir < 2 ? c.Cell.y : c.Cell.x;
            var arch = ArchOf(c.P["arch"]);
            int depth = Math.Min(T, Depth(c, face, c.F + 4));
            if (arch == ArchKind.Segmental) b.Door(face, bay, c.F, c.R("frame"), leaf ? c.R("leaf") : M.Air, depth);
            else b.ArchedDoor(face, bay, c.F, arch, c.R("trim"), leaf ? c.R("leaf") : M.Air, depth);
            // Puerta a pie de calle en la fachada sur: es la entrada.
            if (c.Dir == 3 && c.Cell.y == 0 && c.F == c.Doc.GroundLevel * WorldConstants.SubPerLevel)
                c.Doc.Entrance = new[] { c.Cell.x, -1 };
        }

        /// <summary>Grosor del muro desde la capa exterior de <paramref name="face"/> (al menos 1).</summary>
        static int Depth(Ctx c, Face face, int y)
        {
            int a = (face.A0 + face.A1) / 2, d = 0;
            while (d < S && face.Get(c.Grid, a, y, d) != M.Air) d++;
            return Math.Max(1, d);
        }

        static readonly int[] LeftOf = { 2, 3, 1, 0 }; // +X → +Z, −X → −Z, +Z → −X, −Z → +X

        static void Stairs(Ctx c)
        {
            int floors = c.P["floors"];
            var cells = c.Doc.Cells;
            var starts = new List<(Vector2Int cell, int dir)>();
            var cur = c.Cell;
            int dir = c.Dir;
            for (int k = 0; k < floors; k++)
            {
                var d = StructureNav.Directions[dir];
                var landing = cur + 3 * d;
                foreach (var cell in new[] { cur, cur + d, cur + 2 * d, landing })
                    if (cell.x < 0 || cell.y < 0 || cell.x >= cells.x || cell.y >= cells.y)
                        throw new BridgeException("La escalera no cabe: cada tramo necesita tres celdas libres hacia ese lado y el rellano.");
                starts.Add((cur, dir));
                int side = c.P["turn"] == 0 ? LeftOf[dir] : LeftOf[dir] ^ 1;
                cur = landing + StructureNav.Directions[side];
                dir ^= 1;
            }
            c.Grow(c.F + floors * Storey + 1);
            var b = c.B;
            for (int k = 0; k < starts.Count; k++)
            {
                var (cell, sd) = starts[k];
                int open = c.P["turn"] == 0 ? LeftOf[sd] ^ 1 : LeftOf[sd];
                b.Stairs(cell.x, cell.y, sd, c.F + k * Storey, c.R("floor"), c.R("timber"), c.P.On("rail") ? open : -1);
            }
        }

        static void Chimney(Ctx c)
        {
            int x0 = c.Cell.x * S + 2, z0 = c.Cell.y * S + 2;
            int top = c.F;
            for (int y = c.Grid.SizeY - 1; y >= 0 && top == c.F; y--)
            for (int z = z0 - 1; z < z0 + 4 && top == c.F; z++)
            for (int x = x0 - 1; x < x0 + 5; x++)
                if (c.Grid.IsSolid(x, y, z)) { top = y + 1; break; }
            int topY = Math.Max(c.F + Storey, top + c.P["above"]);
            c.Grow(topY + 1);
            c.B.Chimney(x0, z0, 4, 3, c.F, topY, c.R("chimney"));
        }

        static void Crenels(Ctx c)
        {
            c.Grow(c.F + 12);
            var st = new FortStyle(c.Style, c.Rng);
            if (c.P.On("ring"))
            {
                FortArt.CrenelRing(c.B, st, c.X0, c.Z0, c.X1, c.Z1, c.F);
                return;
            }
            bool alongX = c.LineAlongX(), far = c.P["side"] == 1;
            if (alongX) FortArt.Crenels(c.B, st, true, c.X0, c.X1, far ? c.Z1 - T : c.Z0, far ? c.Z1 : c.Z0 + T, c.F);
            else FortArt.Crenels(c.B, st, false, c.Z0, c.Z1, far ? c.X1 - T : c.X0, far ? c.X1 : c.X0 + T, c.F);
        }

        static void Dome(Ctx c)
        {
            double stretch = c.P["rise"] / 100.0;
            double cx = (c.X0 + c.X1) / 2.0, cz = (c.Z0 + c.Z1) / 2.0, r = Math.Min(c.X1 - c.X0, c.Z1 - c.Z0) / 2.0 - 0.5;
            c.Grow(c.F + (int)Math.Ceiling(r * stretch) + 9);
            var b = c.B;
            int apex = b.Dome(cx, cz, c.F, r, c.R("dome"), stretch, 2, c.P.On("ribs") ? c.R("trim") : M.Air);
            if (c.P.On("finial")) b.Finial((int)Math.Floor(cx), apex, (int)Math.Floor(cz));
        }

        static void Spire(Ctx c)
        {
            int h = c.P["height"];
            c.Grow(c.F + h + 8);
            var b = c.B;
            int top = b.Spire(c.X0, c.Z0, c.X1, c.Z1, c.F, h, c.P.On("octagonal"), c.R("roof"), c.P.On("edges") ? c.R("ridge") : M.Air);
            if (c.P.On("cross")) b.Cross((c.X0 + c.X1) / 2, top, (c.Z0 + c.Z1) / 2, true);
        }

        // ------------------------------------------------------------------ selftest

        /// <summary>Cada herramienta, con sus valores por defecto, sobre una casa del juego: cuántas y qué fallos.</summary>
        public static List<string> SelfTest(Func<string, (JsonElement header, byte[] payload)> sample)
        {
            var failures = new List<string>();
            foreach (var t in All)
            {
                var (h, payload) = sample(t.Id);
                try
                {
                    var r = Apply(h, payload);
                    Builder.Encode(r);
                }
                catch (Exception e)
                {
                    failures.Add($"{t.Id}: {e.GetType().Name}: {e.Message}");
                }
            }
            return failures;
        }
    }
}
