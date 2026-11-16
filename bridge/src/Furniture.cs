// Muebles del modo Editar (docs/contrato.md, «furniture» y «stamp»): los del juego (VoxelFurniture, SacredFurniture,
// ZoneFurniture, SchoolBlueprints), pintados en una rejilla aparte con Spot.At y recortados por lo que no es aire. Los
// parámetros de cada uno (colores, alturas) son los que usan sus planos; los que tiran de azar, con la semilla pedida.

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using AGame.Core;
using AGame.Generation.WorldGen1.Structures;
using AGame.World.Structures;
using F = AGame.Generation.WorldGen1.Structures.VoxelFurniture;
using M = AGame.World.Structures.StructureMaterial;
using SB = AGame.Generation.WorldGen1.Structures.SchoolBlueprints;
using SF = AGame.Generation.WorldGen1.Structures.SacredFurniture;
using ZF = AGame.Generation.WorldGen1.Structures.ZoneFurniture;

namespace AGame.StructureBridge
{
    static class Furniture
    {
        public static readonly (string key, string label)[] Categories =
        {
            ("bedroom", "Dormitorio"), ("kitchen", "Cocina y taller"), ("shop", "Tienda y banco"), ("church", "Iglesia"),
            ("jail", "Cárcel"), ("fields", "Campo y oficios"), ("school", "Escuela de magia"), ("decor", "Decoración"),
        };

        /// <summary>Dónde se pinta: la rejilla aparte, el <see cref="Spot"/> (u a lo largo de la pared, v hacia la sala) y su origen.</summary>
        sealed class Ctx
        {
            public VoxelGrid G;
            public Spot S;
            public int W, X, Y, Z, Facing;
            public Random Rng;

            /// <summary>El mueble va a lo largo de X (mira hacia ±Z).</summary>
            public bool AlongX => Facing >= 2;
            public M Cloth => F.RandomCloth(Rng);
        }

        sealed class Piece
        {
            public string Id, Label, Category;
            public (int min, int max, int def)? Width;
            public Action<Ctx> Paint;

            /// <summary>Se incrusta en la capa del suelo (solo donde ya es sólida): se pinta sobre un suelo de prueba que luego se quita.</summary>
            public bool InFloor;
        }

        static Piece P(string id, string label, string cat, Action<Ctx> paint, (int, int, int)? width = null) =>
            new Piece { Id = id, Label = label, Category = cat, Paint = paint, Width = width };

        static Piece InFloor(Piece p)
        {
            p.InFloor = true;
            return p;
        }

        /// <summary>Suelo de prueba de las piezas <see cref="Piece.InFloor"/>: un material que ningún mueble pinta.</summary>
        const M FloorMark = M.Hidden;

        static readonly M[] Goods = { M.ClothRed, M.Straw, M.Terracotta, M.Leaves, M.Wool, M.Gold };

        static readonly (string id, string label, ZoneTool tool)[] ZoneTools =
        {
            ("Pick", "Pico", ZoneTool.Pick), ("Shovel", "Pala", ZoneTool.Shovel), ("Rake", "Rastrillo", ZoneTool.Rake),
            ("Hoe", "Azada", ZoneTool.Hoe), ("Fork", "Horca", ZoneTool.Fork), ("Scythe", "Guadaña", ZoneTool.Scythe),
            ("Axe", "Hacha", ZoneTool.Axe), ("Sledge", "Mazo", ZoneTool.Sledge),
        };

        static readonly Piece[] All = BuildAll();

        static Piece[] BuildAll()
        {
            var l = new List<Piece>
            {
                // Dormitorio
                P("Bed", "Cama", "bedroom", c => F.Bed(c.G, c.S, 0, c.Cloth)),
                P("BedWide", "Cama de matrimonio", "bedroom", c => F.Bed(c.G, c.S, 0, c.Cloth, true)),
                P("Chest", "Cofre", "bedroom", c => F.Chest(c.G, c.S, 0, 0)),
                P("Wardrobe", "Armario", "bedroom", c => SB.Wardrobe(c.G, c.S, 0, M.Gold)),
                P("Rug", "Alfombra", "bedroom", c => F.Rug(c.G, c.S, 0, c.W, 0, 6, c.Cloth, M.Gold), (4, 16, 8)),
                P("ConsoleMirror", "Consola con espejo", "bedroom", c => F.ConsoleMirror(c.G, c.S, 0, c.W), (4, 8, 5)),
                // Cocina y taller
                P("Table", "Mesa", "kitchen", c => F.Table(c.G, c.S, 0, 0, c.W, 4), (3, 12, 6)),
                P("Stool", "Taburete", "kitchen", c => F.Chair(c.G, c.S, 0, 0, false)),
                P("Chair", "Silla", "kitchen", c => F.Chair(c.G, c.S, 0, 0, true)),
                P("Bench", "Banco corrido", "kitchen", c => F.Bench(c.G, c.S, 0, 0, c.W), (3, 12, 6)),
                P("Oven", "Horno de pan", "kitchen", c => F.Oven(c.G, c.S, 0, 0)),
                P("Anvil", "Yunque", "kitchen", c => F.Anvil(c.G, c.S, 0, 0)),
                P("Forge", "Fragua", "kitchen", c => F.Forge(c.G, c.S, 0, c.W), (6, 10, 7)),
                P("Workbench", "Banco de trabajo", "kitchen", c => F.Workbench(c.G, c.S, 0, c.W, c.Rng), (4, 10, 6)),
                P("Barrel", "Barril", "kitchen", c => F.Barrel(c.G, c.S, 0, 0)),
                P("BarrelLying", "Barril tumbado", "kitchen", c => F.Barrel(c.G, c.S, 0, 0, true)),
                P("Crate", "Caja", "kitchen", c => F.Crate(c.G, c.S, 0, 0)),
                P("Sack", "Saco de grano", "kitchen", c => F.Sack(c.G, c.S, 0, 0)),
                P("Bucket", "Cubo", "kitchen", c => F.Bucket(c.G, c.S, 0, 0)),
                // Tienda y banco
                P("Counter", "Mostrador", "shop", c => F.Counter(c.G, c.S, 0, 0, c.W), (4, 16, 8)),
                P("Shelf", "Estantería", "shop", c => F.Shelf(c.G, c.S, 0, c.W, c.Rng), (3, 12, 6)),
                P("Bookshelf", "Librería", "shop", c => F.Bookshelf(c.G, c.S, 0, c.W, c.Rng), (3, 12, 6)),
                P("Produce", "Cesto de género", "shop", c => F.Produce(c.G, c.S, 0, 0, Goods[c.Rng.Next(Goods.Length)])),
                P("Bar", "Barra de posada", "shop", c => F.Bar(c.G, c.S, 0, c.W, c.Rng), (6, 16, 10)),
                P("Balance", "Balanza", "shop", c => F.Balance(c.G, c.S, 0, 0, 0)),
                P("CoinStacks", "Monedas", "shop", c => F.CoinStacks(c.G, c.S, 0, c.W, 0, 4, 0, c.Rng), (2, 8, 4)),
                P("GoldBars", "Lingotes", "shop", c => F.GoldBars(c.G, c.S, 0, 0)),
                P("CoinSack", "Talega de monedas", "shop", c => F.CoinSack(c.G, c.S, 0, 0)),
                P("IronboundChest", "Arca de hierro", "shop", c => F.IronboundChest(c.G, c.S, 0, 0)),
                P("CounterGrille", "Reja de ventanilla", "shop",
                    c => F.CounterGrille(c.G, c.S, 0, c.W, 0, 0, 12, c.W / 2 - 2, c.W / 2 + 2), (8, 16, 10)),
                P("BankersArms", "Escudo de los cambistas", "shop", c => F.BankersArms(c.G, c.S, 0, 4, M.ClothBlue, M.Gold, M.ClothRed)),
                // Iglesia
                P("Altar", "Altar mayor", "church", c => F.Altar(c.G, c.S, 0, M.Fresco)),
                P("Pew", "Banco de iglesia", "church", c => F.Pew(c.G, c.S, 0, 0, c.W), (4, 16, 8)),
                P("Pulpit", "Púlpito", "church", c => SF.Pulpit(c.G, c.S, 0, 0)),
                P("SmallAltar", "Altar de ermita", "church", c => SF.SmallAltar(c.G, c.S, 0, M.Whitewash, M.Wool)),
                P("Font", "Pila bautismal", "church", c => SF.Font(c.G, c.S, 0, 0)),
                P("ChoirStalls", "Sillería de coro", "church", c => SF.ChoirStalls(c.G, c.S, 0, c.W), (6, 18, 9)),
                P("Candelabrum", "Candelabro", "church", c => F.Candelabrum(c.G, c.S, 0, 0)),
                P("StoneCross", "Crucero de piedra", "church", c => SF.StoneCross(c.G, c.X, c.Y, c.Z)),
                P("Headstone", "Lápida", "church", c => SF.Headstone(c.G, c.X, c.Y, c.Z, c.AlongX, c.Rng.Next(4), M.Stone)),
                P("GraveCross", "Cruz de tumba", "church", c => SF.GraveCross(c.G, c.X, c.Y, c.Z, c.AlongX, M.DarkTimber)),
                P("Grave", "Túmulo", "church", c => SF.Mound(c.G, c.X, c.Y, c.Z, c.X + (c.AlongX ? 3 : 6), c.Z + (c.AlongX ? 6 : 3), M.Gravel)),
                P("Cypress", "Ciprés", "church", c => SF.Cypress(c.G, c.X, c.Y, c.Z)),
                P("Yew", "Tejo", "church", c => SF.Yew(c.G, c.X, c.Y, c.Z)),
                // Cárcel
                P("Bars", "Reja", "jail", c => F.Bars(c.G, c.S, 0, c.W, 0, 0, 12), (4, 16, 8)),
                P("Cot", "Catre", "jail", c => F.Cot(c.G, c.S, 0, M.ClothRed)),
                P("Stocks", "Cepo", "jail", c => F.Stocks(c.G, c.S, 0, 0)),
                P("Shackles", "Grilletes", "jail", c => F.Shackles(c.G, c.S, 0, 8)),
                P("WallTorch", "Antorcha de pared", "jail", c => F.WallTorch(c.G, c.S, 0, 8)),
                P("WantedBoard", "Tablón de bandos", "jail", c => F.WantedBoard(c.G, c.S, 0, c.W, 4, c.Rng), (4, 10, 6)),
                // Campo y oficios
                P("HayBale", "Paca de paja", "fields", c => F.HayBale(c.G, c.S, 0, 0)),
                P("ToolRack", "Tablero de herramientas", "fields",
                    c => ZF.ToolRack(c.G, c.S, 0, c.W, new[] { ZoneTool.Pick, ZoneTool.Shovel, ZoneTool.Rake, ZoneTool.Axe }), (4, 12, 6)),
                P("LogPile", "Pila de troncos", "fields", c => ZF.LogPile(c.G, c.S, 0, c.W), (4, 16, 8)),
                P("PlankStack", "Montón de tablas", "fields", c => ZF.PlankStack(c.G, c.S, 0, c.W, 4, 3), (4, 12, 8)),
                P("ChoppingBlock", "Tajo", "fields", c => ZF.ChoppingBlock(c.G, c.S, 0, 0)),
                P("TiedSack", "Saco atado", "fields", c => ZF.Sack(c.G, c.S, 0, 0, M.Canvas, M.Rope)),
                P("Pelt", "Piel colgada", "fields", c => ZF.Pelt(c.G, c.S, 0, 6, M.Straw, M.Wool)),
                P("Antlers", "Cornamenta", "fields", c => ZF.Antlers(c.G, c.S, 0, 9)),
                P("Net", "Red de pesca", "fields", c => ZF.Net(c.G, c.S, 0, c.W, 2, 12, M.ClothRed), (4, 12, 8)),
                P("Minecart", "Vagoneta", "fields", c => ZF.Minecart(c.G, c.S, 0, 0, M.Iron)),
                P("OreHeap", "Montón de mineral", "fields", c => ZF.Mound(c.G, c.Rng, c.X, c.Y, c.Z, 4, 4, M.Gravel, M.Iron)),
                P("CartWheel", "Rueda de carro", "fields", c => ZF.CartWheel(c.G, c.S, 0, 1)),
                P("Plough", "Arado", "fields", c => ZF.Plough(c.G, c.S, 0, 0)),
                P("Oar", "Remo", "fields", c => ZF.Oar(c.G, c.S, 0)),
                P("FishLine", "Cordel de pescado", "fields", c => ZF.FishLine(c.G, c.S, 0, c.W, 10), (4, 12, 6)),
                P("Crook", "Cayado", "fields", c => ZF.Crook(c.G, c.S, 0)),
                P("Churn", "Mantequera", "fields", c => ZF.Churn(c.G, c.S, 0, 0)),
                P("Blocks", "Sillares", "fields", c => ZF.Blocks(c.G, c.S, 0, 0, false)),
                P("BlocksStacked", "Sillares apilados", "fields", c => ZF.Blocks(c.G, c.S, 0, 0, true)),
                P("Scale", "Báscula", "fields", c => ZF.Scale(c.G, c.S, 0, 0)),
                P("Wheelbarrow", "Carretilla", "fields", c => ZF.Wheelbarrow(c.G, c.S)),
                P("HideFrame", "Bastidor de pieles", "fields", c => ZF.HideFrame(c.G, c.S, 0, M.Canvas)),
                // Escuela de magia
                P("Desk", "Pupitre", "school", c => SB.Desk(c.G, c.S, 0, 0)),
                P("Lectern", "Atril", "school", c => SB.Lectern(c.G, c.S, 0, 0)),
                P("RuneBoard", "Pizarra de runas", "school", c => SB.RuneBoard(c.G, c.S, 0, c.W, c.Rng), (6, 16, 10)),
                P("Bookcase", "Librería alta", "school", c => SB.Bookcase(c.G, c.S, 0, c.W, 12, c.Rng), (4, 12, 6)),
                P("CrystalCluster", "Cristales", "school", c => SB.CrystalCluster(c.G, c.X, c.Y, c.Z, 8)),
                P("FloatingCrystal", "Cristal flotante", "school", c => SB.FloatingCrystal(c.G, c.X, c.Y + 6, c.Z, 2)),
                P("CrystalPedestal", "Pedestal de cristal", "school", c => SB.CrystalPedestal(c.G, c.X, c.Y, c.Z)),
                P("Athanor", "Atanor", "school", c => SB.Athanor(c.G, c.S, 0, 15)),
                P("Cauldron", "Caldero", "school", c => SB.Cauldron(c.G, c.X, c.Y, c.Z)),
                P("Flasks", "Frascos", "school", c => SB.Flasks(c.G, c.S, 0, c.W, 0, 0, c.Rng), (3, 8, 4)),
                P("LabTable", "Mesa de laboratorio", "school", c => SB.LabTable(c.G, c.S, 0, 0, c.W, 4, c.Rng), (4, 10, 6)),
                P("Telescope", "Telescopio", "school", c => SB.Telescope(c.G, c.S, 6)),
                P("Orrery", "Planetario", "school", c => SB.Orrery(c.G, c.X, c.Y, c.Z, c.Rng)),
                P("StarTable", "Mesa de cartas celestes", "school", c => SB.StarTable(c.G, c.S, 0, 0, c.W, 4, c.Rng), (4, 10, 6)),
                P("Candle", "Vela", "school", c => SB.Candle(c.G, c.X, c.Y, c.Z)),
                InFloor(P("RuneRing", "Círculo de runas", "school", c => SB.RuneRing(c.G, c.X, c.Z, c.Y, 3, 5, M.ThemeGlow, 6))),
                InFloor(P("RuneStar", "Estrella de runas", "school", c => SB.RuneStar(c.G, c.X, c.Z, c.Y, 5, 5, 2, M.ThemeGlow))),
                P("SchoolBanner", "Estandarte de la escuela", "school", c => SB.SchoolBanner(c.G, c.S, 0, 14)),
                // Decoración
                P("Banner", "Estandarte", "decor", c => F.Banner(c.G, c.S, 0, 14, c.Cloth)),
                P("WallLantern", "Farol de pared", "decor", c => F.WallLantern(c.G, c.S, 0, 9)),
                P("HangingLantern", "Farol colgado", "decor", c => F.HangingLantern(c.G, c.X, c.Z, c.Y + CityBrush.Storey - 1)),
                P("Chandelier", "Araña", "decor", c => F.Chandelier(c.G, c.X, c.Z, c.Y + CityBrush.Storey - 1)),
                P("Throne", "Trono", "decor", c => F.Throne(c.G, c.S, 0, 0)),
                P("PottedPlant", "Maceta", "decor", c => F.PottedPlant(c.G, c.S, 0, 0, c.Rng)),
                P("Tree", "Árbol de patio", "decor", c => F.Tree(c.G, c.X, c.Y, c.Z, 14)),
                P("Lion", "León dorado", "decor", c => F.Lion(c.G, c.S, 0, 0)),
                P("Statue", "Estatua", "decor", c => F.Statue(c.G, c.S, 0, 0, M.Marble)),
                P("Urn", "Jarrón", "decor", c => F.Urn(c.G, c.S, 0, 0, M.Ashlar)),
                P("Balustrade", "Balaustrada", "decor", c => F.Balustrade(c.G, c.S, 0, c.W, 0, M.Marble), (4, 16, 8)),
                P("Canopy", "Dosel", "decor", c => F.Canopy(c.G, c.S, 0, 8, 18, c.Cloth)),
                P("CoatOfArms", "Escudo", "decor", c => F.CoatOfArms(c.G, c.S, 0, 6, c.Cloth, M.Gold)),
                P("Tapestry", "Tapiz", "decor", c => F.Tapestry(c.G, c.S, 0, c.W, 4, 14, c.Cloth, M.Gold), (4, 14, 6)),
                P("ArmorStand", "Armadura", "decor", c => F.ArmorStand(c.G, c.S, 0, 0, c.Cloth)),
                P("WeaponRack", "Panoplia", "decor", c => F.WeaponRack(c.G, c.S, 0, c.W), (4, 10, 6)),
                P("Harpsichord", "Clavecín", "decor", c => F.Harpsichord(c.G, c.S, 0, 0)),
                P("LampPost", "Farola", "decor", c => F.LampPost(c.G, c.X, c.Y, c.Z)),
                P("Globe", "Globo terráqueo", "decor", c => F.Globe(c.G, c.S)),
            };
            foreach (var (id, label, tool) in ZoneTools)
                l.Add(P("Tool" + id, label, "fields", c => ZF.Tool(c.G, c.S, 0, tool)));
            return l.ToArray();
        }

        // ------------------------------------------------------------------ pintar y recortar

        /// <summary>Lado de la rejilla aparte, en sub-cubos (el mueble se pinta en su centro) y su alto.</summary>
        const int Side = 96, Height = 64, C = Side / 2, Floor = 1;

        sealed class Stamp
        {
            public VoxelGrid Grid;
            public List<VoxelInteractable> Items = new List<VoxelInteractable>();
            public int[] Anchor;
        }

        static Stamp Paint(Piece p, int facing, int width, int seed)
        {
            var g = new VoxelGrid(Side, Height, Side);
            var c = new Ctx
            {
                G = g, S = Spot.At(C, Floor, C, facing & 3), X = C, Y = Floor, Z = C, Facing = facing & 3,
                W = p.Width.HasValue ? Math.Clamp(width <= 0 ? p.Width.Value.def : width, p.Width.Value.min, p.Width.Value.max) : 0,
                Rng = new Random(seed),
            };
            if (p.InFloor) g.Fill(0, Floor - 1, 0, Side, Floor, Side, FloorMark);
            // Los muebles son detalle: el cutaway no los corta en la planta del jugador (docs/ciudades.md).
            using (g.DetailScope()) p.Paint(c);
            if (p.InFloor)
                for (int z = 0; z < Side; z++)
                for (int x = 0; x < Side; x++)
                    if (g.Get(x, Floor - 1, z) == FloorMark) g.Set(x, Floor - 1, z, M.Air);
            int x0 = Side, y0 = Height, z0 = Side, x1 = -1, y1 = -1, z1 = -1;
            var raw = g.Raw;
            for (int i = 0; i < raw.Length; i++)
            {
                if (raw[i] == 0) continue;
                int x = i % Side, row = i / Side, z = row % Side, y = row / Side;
                x0 = Math.Min(x0, x); x1 = Math.Max(x1, x);
                y0 = Math.Min(y0, y); y1 = Math.Max(y1, y);
                z0 = Math.Min(z0, z); z1 = Math.Max(z1, z);
            }
            if (x1 < 0) throw new BridgeException($"«{p.Label}» no ha pintado nada.");
            var out_ = new VoxelGrid(x1 - x0 + 1, y1 - y0 + 1, z1 - z0 + 1);
            for (int y = y0; y <= y1; y++)
            for (int z = z0; z <= z1; z++)
            for (int x = x0; x <= x1; x++)
            {
                var m = g.Get(x, y, z);
                if (m == M.Air) continue;
                out_.MarkDetail = g.IsDetail(x, y, z);
                out_.Set(x - x0, y - y0, z - z0, m);
            }
            out_.MarkDetail = false;
            var s = new Stamp { Grid = out_, Anchor = new[] { C - x0, Floor - y0, C - z0 } };
            if (g.Interactables != null)
                foreach (var it in g.Interactables)
                    s.Items.Add(it.WithBox(it.X0 - x0, it.Y0 - y0, it.Z0 - z0, it.X1 - x0, it.Y1 - y0, it.Z1 - z0, it.Facing));
            return s;
        }

        static object _list;

        /// <summary><c>furniture</c>: categorías y piezas, con su tamaño mirando al norte (u = x, v = z) y su ancho si lo admite.</summary>
        public static object List()
        {
            if (_list != null) return _list;
            _list = new
            {
                categories = Categories.Select(c => new { key = c.key, label = c.label }).ToArray(),
                pieces = All.Select(p =>
                {
                    var s = Paint(p, 2, 0, 0);
                    return new
                    {
                        id = p.Id, label = p.Label, category = p.Category,
                        size = new[] { s.Grid.SizeX, s.Grid.SizeZ, s.Grid.SizeY },
                        width = p.Width.HasValue ? new { min = p.Width.Value.min, max = p.Width.Value.max, @default = p.Width.Value.def } : null,
                        interacts = s.Items.Count > 0,
                    };
                }).ToArray(),
            };
            return _list;
        }

        /// <summary><c>stamp</c>: la pieza recortada (cabecera y carga con rejilla y detalle, sin navegación).</summary>
        public static (Dictionary<string, object> header, byte[] payload) StampJson(JsonElement req)
        {
            // «id» es el de la trama (docs/contrato.md, «Tramas»): el del mueble va en «piece».
            string id = req.TryGetProperty("piece", out var pj) && pj.ValueKind == JsonValueKind.String ? pj.GetString() : null;
            var p = All.FirstOrDefault(x => x.Id == id) ?? throw new BridgeException($"No hay ningún mueble «{id}».");
            int facing = req.TryGetProperty("facing", out var fj) ? fj.GetInt32() : 3;
            int width = req.TryGetProperty("width", out var wj) && wj.ValueKind == JsonValueKind.Number ? wj.GetInt32() : 0;
            int seed = req.TryGetProperty("seed", out var sj) && sj.ValueKind == JsonValueKind.Number ? sj.GetInt32() : 0;
            var s = Paint(p, facing, width, seed);
            var grid = s.Grid.Raw.ToArray();
            var detail = s.Grid.RawDetail.ToArray();
            var header = new Dictionary<string, object>
            {
                ["size"] = new[] { s.Grid.SizeX, s.Grid.SizeY, s.Grid.SizeZ },
                ["anchor"] = s.Anchor,
                ["parts"] = Array.Empty<object>(),
                ["interactables"] = s.Items.Select(it => Json.ItemJson(it)).ToArray(),
                ["payload"] = new { grid = grid.Length, detail = detail.Length, navFloors = 0, navSlabs = 0, navEdges = 0 },
            };
            return (header, grid.Concat(detail).ToArray());
        }

        /// <summary>Cada mueble en las cuatro orientaciones: los que fallan.</summary>
        public static List<string> SelfTest()
        {
            var failures = new List<string>();
            foreach (var p in All)
                for (int facing = 0; facing < 4; facing++)
                    try
                    {
                        var s = Paint(p, facing, 0, 7);
                        if (s.Grid.CountSolid() == 0) failures.Add($"{p.Id} hacia {facing}: vacío");
                    }
                    catch (Exception e)
                    {
                        failures.Add($"{p.Id} hacia {facing}: {e.GetType().Name}: {e.Message}");
                    }
            return failures;
        }
    }
}
