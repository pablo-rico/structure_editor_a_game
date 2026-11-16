// Comprobaciones de un plano (docs/contrato.md, «checks»), en español llano para artistas. Son las reglas de
// CityBlueprintTests.BlueprintIsSound (y su Reachable), que viven en los tests del juego y no fuera: por eso el BFS
// está copiado aquí. Si cambian allí, cámbialas aquí.

using System;
using System.Collections.Generic;
using AGame.Core;
using AGame.Generation.WorldGen1.Structures;
using AGame.World.Structures;
using UnityEngine;

namespace AGame.StructureBridge
{
    static class Checks
    {
        /// <summary>Alto máximo de la rejilla que admiten los tests (60 niveles).</summary>
        const int MaxHeightSub = 60 * WorldConstants.SubPerLevel;

        const int S = WorldConstants.SubPerCell;

        public static void Run(BuildResult r)
        {
            var b = r.Build;
            var list = r.Checks;
            void Add(string severity, string code, string message, object at) =>
                list.Add(new { severity, code, message, at });
            object Cell(int x, int z, int level) => new { cell = new[] { x, z }, level };

            if (b.Grid.SizeY > MaxHeightSub)
                Add("error", "too-tall", $"Es demasiado alta: {b.Grid.SizeY / WorldConstants.SubPerLevel} niveles (el máximo son 60).", null);
            if (b.Parts != null && b.Parts.Count > StructureParts.MaxParts)
                Add("error", "too-many-parts", $"Tiene {b.Parts.Count} partes móviles (puertas, muros que se rompen…); caben {StructureParts.MaxParts}.", null);
            for (int i = 1; i < b.BandStarts.Length; i++)
                if (b.BandStarts[i] <= b.BandStarts[i - 1] || b.BandFloors[i] <= b.BandFloors[i - 1])
                    Add("error", "bands-order", $"Los pisos {i - 1} y {i} no van de abajo arriba (el corte por plantas saldría mal).", null);

            // Los barcos se entran por la pasarela de su costado: sus reglas son otras (BoatBlueprintContractTests).
            if (r.Def.Mode == BuildMode.Boat) return;
            var e = b.Entrance;
            if (b.Nav.InFootprint(e.x, e.y))
                Add("error", "entrance-inside", "La entrada cae dentro del edificio: tiene que ser la celda de fuera pegada a la puerta.",
                    Cell(e.x, e.y, b.GroundLevel));

            if (!IsClosed(r)) return;
            if (r.Damage != StructureDamage.Intact)
            {
                Add("info", "damaged-skipped", "Con daño no se comprueba la puerta ni las plantas (las ruinas no se recorren).", null);
                return;
            }

            // Las antiguas van con las medidas de VoxelBrush, no con StructureScale (StructureBlueprintTests no lo mira).
            if (r.Def.Mode == BuildMode.Legacy) { }
            else if (r.Turns == 0) DoorHeadroom(r, Add);
            else
                Add("info", "door-headroom-skipped", "El hueco de la puerta solo se comprueba con la puerta al sur (sin girar).", null);

            var reach = Reachable(b, Covered(r), out var door);
            if (door == null)
            {
                Add("error", "no-entry", "No se puede entrar por la puerta: falta suelo justo detrás o algo tapa el paso.",
                    Cell(e.x, e.y, b.GroundLevel));
                return;
            }
            for (int i = 0; i < CheckedBands(r); i++)
            {
                int lo = b.BandFloors[i], hi = i + 1 < b.BandFloors.Length ? b.BandFloors[i + 1] : int.MaxValue;
                bool any = false;
                foreach (var (_, _, lv) in reach) any |= lv >= lo && lv < hi;
                if (!any)
                    Add("error", "floor-unreachable", $"No se llega al piso {i} (nivel {lo}) desde la entrada: falta una escalera o un paso.",
                        FirstFloor(b.Nav, lo, hi) ?? Cell(door.Value.x, door.Value.y, lo));
            }
        }

        /// <summary>¿Cubre la estructura la celda (ya girada)? Casi todas llenan su caja (<see cref="StructureFootprints"/>).</summary>
        static Func<int, int, bool> Covered(BuildResult r)
        {
            if (r.Def.Mode != BuildMode.Instance) return (x, z) => true;
            var cells = r.Build.Cells;
            return (x, z) => StructureFootprints.Covers(r.Def.Kind, r.Variant, r.Request.Facing & 3, cells.x, cells.y, x, z);
        }

        /// <summary>Celda con suelo en la franja <c>[lo, hi)</c>, para señalar el piso al que no se llega.</summary>
        static object FirstFloor(StructureNav nav, int lo, int hi)
        {
            for (int lv = lo; lv < Math.Min(hi, nav.Levels); lv++)
            for (int z = 0; z < nav.CellsZ; z++)
            for (int x = 0; x < nav.CellsX; x++)
                if (nav.HasFloor(x, z, lv))
                    return new { cell = new[] { x, z }, level = lv };
            return null;
        }

        /// <summary>
        /// Edificios con puerta e interior, en los que se entra y se llega a cada piso (CityBlueprintTests.IsClosed y, para
        /// los demás grupos, sus tests: ZoneBlueprintTests, SchoolModuleTests, WildBlueprintTests, StructureBlueprintTests).
        /// </summary>
        static bool IsClosed(BuildResult r)
        {
            var kind = r.Def.Kind;
            switch (r.Def.Group)
            {
                case "zone": return !ZoneBlueprints.IsOpen(kind);
                case "school": return true;
                case "authored": return true; // AuthoredBlueprintTests: toda estructura a mano se entra y se recorre
                case "wild": return kind == StructureKind.OrcHall || kind == StructureKind.GoblinHut;
                case "legacy": return kind == StructureKind.House || kind == StructureKind.Tower;
                case "boat": return false;
            }
            // En obra, las piezas de muralla no tienen paso de ronda (FortKitTests.ScaffoldedPiecesBuildAndStayLow).
            if (FortKit.IsWallPiece(kind) && FortKit.IsScaffold(r.Variant)) return false;
            return !CityBlueprints.IsOpen(kind, r.Variant) && kind != StructureKind.WallSegment;
        }

        /// <summary>Franjas cuyo suelo se alcanza desde la puerta (la del tejado no cuenta), como CityBlueprintTests.CheckedBands.</summary>
        static int CheckedBands(BuildResult r)
        {
            var b = r.Build;
            if (r.Def.Kind == StructureKind.Gate) return 1; // el adarve se alcanza desde las torres
            if (r.Def.Kind == StructureKind.WallTower) return b.BandFloors.Length; // la terraza también
            if (r.Def.Group == "wild") return 1; // WildBlueprintTests solo pide que se entre
            return Math.Max(1, b.BandFloors.Length - 1);
        }

        /// <summary>
        /// El hueco de la puerta (sin girar) deja <see cref="StructureScale.HeadroomSub"/> sub-cubos libres en las dos
        /// columnas centrales de su celda, del borde de la huella a celda y media hacia dentro (CityBlueprintTests.DoorHasHeadroom).
        /// </summary>
        static void DoorHeadroom(BuildResult r, Action<string, string, string, object> add)
        {
            var b = r.Build;
            int dx = b.Entrance.x; // sin girar, la entrada es (DoorX, −1)
            int y0 = b.GroundLevel * WorldConstants.SubPerLevel;
            for (int z = 0; z < S + S / 2 && z < b.Grid.SizeZ; z++)
            for (int x = dx * S + 3; x < dx * S + 5; x++)
            for (int y = y0; y < y0 + StructureScale.HeadroomSub; y++)
                if (b.Grid.IsSolid(x, y, z))
                {
                    add("error", "door-headroom",
                        $"Hay {b.Grid.Get(x, y, z)} en el hueco de la puerta a {y - y0} sub-cubos del suelo: hacen falta " +
                        $"{StructureScale.HeadroomSub} libres para pasar.", new { voxel = new[] { x, y, z } });
                    return;
                }
        }

        /// <summary>
        /// Suelos (celda, nivel) alcanzables desde la entrada a pasos de un nivel sin cruzar aristas bloqueadas;
        /// <paramref name="doorCell"/> es la celda de dentro pegada a la entrada (null si no se entra). Copia de
        /// CityBlueprintTests.Reachable.
        /// </summary>
        public static HashSet<(int, int, int)> Reachable(StructureBuild b, Func<int, int, bool> covered, out Vector2Int? doorCell)
        {
            var nav = b.Nav;
            var seen = new HashSet<(int, int, int)>();
            var q = new Queue<(int, int, int)>();
            doorCell = null;
            var e = b.Entrance;
            for (int dir = 0; dir < 4; dir++)
            {
                var dv = StructureNav.Directions[dir];
                int cx = e.x + dv.x, cz = e.y + dv.y;
                // Huella con huecos (la torre de muralla): la puerta da a la celda de terreno del hueco; se entra por la
                // siguiente, como en FortKitTests (suelo en (DoorX, 1) sin girar).
                if (!covered(cx, cz)) (cx, cz) = (cx + dv.x, cz + dv.y);
                if (!nav.InFootprint(cx, cz)) continue;
                if (!nav.HasFloor(cx, cz, b.GroundLevel)) continue;
                if (nav.IsEdgeBlocked(cx, cz, dir ^ 1, b.GroundLevel, b.GroundLevel)) continue;
                doorCell = new Vector2Int(cx, cz);
                seen.Add((cx, cz, b.GroundLevel));
                q.Enqueue((cx, cz, b.GroundLevel));
            }
            while (q.Count > 0)
            {
                var (cx, cz, lv) = q.Dequeue();
                for (int dir = 0; dir < 4; dir++)
                {
                    var dv = StructureNav.Directions[dir];
                    int nx = cx + dv.x, nz = cz + dv.y;
                    if (!nav.InFootprint(nx, nz)) continue;
                    for (int to = lv - 1; to <= lv + 1; to++)
                    {
                        if (!nav.HasFloor(nx, nz, to)) continue;
                        if (nav.IsEdgeBlocked(cx, cz, dir, lv, to) || nav.IsEdgeBlocked(nx, nz, dir ^ 1, to, lv)) continue;
                        if (seen.Add((nx, nz, to))) q.Enqueue((nx, nz, to));
                    }
                }
            }
            return seen;
        }
    }
}
