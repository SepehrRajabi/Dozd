import type {CargoSpace} from './cargo-space.js';
import type {GridPosition} from './cargo-space.js';
import type {Weapon} from './weapon.js';

export interface Player {
  health: number;
  inventory: CargoSpace;
  agility: number;
  stamina: number;
  position: GridPosition;
  weapon: Weapon;
}
