import type {Loot} from './loot.js';

export interface GridPosition {
  x: number;
  y: number;
}

/** A loot item and its position within a cargo grid. */
export interface PlacedLoot {
  loot: Loot;
  position: GridPosition;
}

/** A rectangular cargo grid and the loot it contains. */
export interface CargoSpace {
  width: number;
  height: number;
  loots: PlacedLoot[];
}
