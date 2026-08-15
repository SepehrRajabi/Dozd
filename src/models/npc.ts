import type {GridPosition} from './cargo-space.js';
import type {Weapon} from './weapon.js';

export interface NpcOptions {
  name: string;
  health: number;
  position: GridPosition;
  weapon: Weapon;
  guardedLootName: string;
}

/** An armed cargo defender controlled by the game. */
export class Npc {
  public readonly name: string;
  public readonly health: number;
  public readonly position: GridPosition;
  public readonly weapon: Weapon;
  public readonly guardedLootName: string;

  public constructor({name, health, position, weapon, guardedLootName}: NpcOptions) {
    this.name = name;
    this.health = health;
    this.position = position;
    this.weapon = weapon;
    this.guardedLootName = guardedLootName;
  }

  public get isAlive(): boolean {
    return this.health > 0;
  }

  public takeDamage(damage: number): Npc {
    return new Npc({
      name: this.name,
      health: Math.max(0, this.health - damage),
      position: this.position,
      weapon: this.weapon,
      guardedLootName: this.guardedLootName,
    });
  }

  public moveTo(position: GridPosition): Npc {
    return new Npc({
      name: this.name,
      health: this.health,
      position,
      weapon: this.weapon,
      guardedLootName: this.guardedLootName,
    });
  }
}
