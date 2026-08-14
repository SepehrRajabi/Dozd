import {Npc, type CargoSpace, type GridPosition, type Loot, type Player, type Weapon} from '../models/index.js';
import {findPath} from './pathfinding.js';

export interface GameInstance {
  cargoSpace: CargoSpace;
  player: Player;
  npcs: Npc[];
  lastEvent: string;
}

const CARGO_SIZE = 64;
const ATTACK_DISTANCE = 1;
const PLAYER_WEAPON_RANGE = 6;

export function createGameInstance(): GameInstance {
  const starterWeapon: Weapon = {name: 'Rustbite Pistol', type: 'plasma', damage: 12, weight: 2};
  const defenderWeapon: Weapon = {name: 'Watchman Carbine', type: 'laser', damage: 8, weight: 4};
  const recoveredCore: Loot = {name: 'Neural Core', reward: 1_200};

  return {
    cargoSpace: {
      width: CARGO_SIZE,
      height: CARGO_SIZE,
      loots: [{loot: recoveredCore, position: {x: 0, y: 0}}],
    },
    player: {
      health: 100,
      inventory: {width: 8, height: 8, loots: []},
      agility: 5,
      stamina: 10,
      position: {x: 2, y: 2},
      weapon: starterWeapon,
    },
    npcs: [new Npc({name: 'Cargo Sentry', health: 30, position: {x: 1, y: 0}, weapon: defenderWeapon})],
    lastEvent: 'A Cargo Sentry is defending the Neural Core at the top-left.',
  };
}

export function movePlayer(game: GameInstance, movement: GridPosition): GameInstance {
  if (game.player.health <= 0) return {...game, lastEvent: 'You are incapacitated and cannot move.'};

  const {cargoSpace, player} = game;
  const position = {
    x: clamp(player.position.x + movement.x, 0, cargoSpace.width - 1),
    y: clamp(player.position.y + movement.y, 0, cargoSpace.height - 1),
  };
  const defender = game.npcs.find((npc) => npc.isAlive && samePosition(npc.position, position));
  if (defender) return {...game, lastEvent: `${defender.name} blocks your path.`};

  const capturedLoot = cargoSpace.loots.find(({position: lootPosition}) => samePosition(lootPosition, position));
  if (!capturedLoot) {
    return {...game, player: {...player, position}, lastEvent: `Moved to (${position.x}, ${position.y}).`};
  }

  return {
    ...game,
    cargoSpace: {...cargoSpace, loots: cargoSpace.loots.filter(({loot}) => loot !== capturedLoot.loot)},
    player: {...player, position, inventory: {...player.inventory, loots: [...player.inventory.loots, capturedLoot]}},
    lastEvent: `Captured ${capturedLoot.loot.name} (+${capturedLoot.loot.reward} credits).`,
  };
}

/** Fires at the nearest living defender in the supplied direction. */
export function shootWeapon(game: GameInstance, direction: GridPosition): GameInstance {
  if (game.player.health <= 0) return {...game, lastEvent: 'You are incapacitated and cannot fire.'};

  const target = nearestTarget(game.player.position, direction, game.npcs);
  if (!target) return {...game, lastEvent: `${game.player.weapon.name} fires into empty cargo.`};

  const damagedTarget = target.takeDamage(game.player.weapon.damage);
  const npcs = game.npcs.map((npc) => (npc === target ? damagedTarget : npc));
  const shotEvent = damagedTarget.isAlive
    ? `Hit ${target.name} for ${game.player.weapon.damage} damage (${damagedTarget.health} health left).`
    : `Destroyed ${target.name}.`;

  return {...game, npcs, lastEvent: shotEvent};
}

/** Resolves one movement-and-attack turn for every living defender. */
export function advanceNpcs(game: GameInstance): GameInstance {
  if (game.player.health <= 0) return game;
  let npcs = game.npcs;
  let player = game.player;
  const events: string[] = [];

  for (const [index, npc] of npcs.entries()) {
    if (!npc.isAlive || player.health <= 0) continue;

    let updatedNpc = npc;
    if (distance(npc.position, player.position) > ATTACK_DISTANCE) {
      const occupiedPositions = npcs
        .filter((otherNpc, otherIndex) => otherIndex !== index && otherNpc.isAlive)
        .map(({position}) => position);
      const path = findPath(npc.position, player.position, game.cargoSpace, occupiedPositions);
      const nextPosition = path[0];
      if (nextPosition) {
        updatedNpc = npc.moveTo(nextPosition);
        npcs = npcs.map((currentNpc, currentIndex) => currentIndex === index ? updatedNpc : currentNpc);
        events.push(`${npc.name} moves toward you.`);
      }
    }

    if (distance(updatedNpc.position, player.position) <= ATTACK_DISTANCE) {
      const health = Math.max(0, player.health - updatedNpc.weapon.damage);
      player = {...player, health};
      const outcome = health === 0 ? ' You are incapacitated.' : ` (${health} health left).`;
      events.push(`${updatedNpc.name} hits you for ${updatedNpc.weapon.damage} damage${outcome}`);
    }
  }

  return events.length === 0 ? game : {...game, npcs, player, lastEvent: `${game.lastEvent} ${events.join(' ')}`};
}

function nearestTarget(origin: GridPosition, direction: GridPosition, npcs: Npc[]): Npc | undefined {
  return npcs
    .filter((npc) => npc.isAlive && isInDirection(origin, npc.position, direction))
    .sort((first, second) => distance(origin, first.position) - distance(origin, second.position))[0];
}

function isInDirection(origin: GridPosition, target: GridPosition, direction: GridPosition): boolean {
  if (direction.x !== 0) {
    return target.y === origin.y && Math.sign(target.x - origin.x) === direction.x && distance(origin, target) <= PLAYER_WEAPON_RANGE;
  }
  return target.x === origin.x && Math.sign(target.y - origin.y) === direction.y && distance(origin, target) <= PLAYER_WEAPON_RANGE;
}

function distance(first: GridPosition, second: GridPosition): number {
  return Math.abs(first.x - second.x) + Math.abs(first.y - second.y);
}

function samePosition(first: GridPosition, second: GridPosition): boolean {
  return first.x === second.x && first.y === second.y;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(value, maximum));
}
