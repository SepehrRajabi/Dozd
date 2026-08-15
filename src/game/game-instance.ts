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
const GUARD_DETECTION_DISTANCE = 10;
const PLAYER_WEAPON_RANGE = 6;

export function createGameInstance(): GameInstance {
  const starterWeapon: Weapon = {name: 'Rustbite Pistol', type: 'plasma', damage: 12, weight: 2};
  const lootCaches: Array<{loot: Loot; position: GridPosition; guardPosition: GridPosition}> = [
    {loot: {name: 'Neural Core', reward: 1_200}, position: {x: 0, y: 0}, guardPosition: {x: 1, y: 0}},
    {loot: {name: 'Engine Cache', reward: 800}, position: {x: 12, y: 8}, guardPosition: {x: 12, y: 9}},
    {loot: {name: 'Quantum Coil', reward: 1_600}, position: {x: 24, y: 4}, guardPosition: {x: 23, y: 4}},
    {loot: {name: 'Star Chart', reward: 950}, position: {x: 38, y: 18}, guardPosition: {x: 38, y: 19}},
    {loot: {name: 'Cryo Pod', reward: 1_400}, position: {x: 50, y: 7}, guardPosition: {x: 49, y: 7}},
    {loot: {name: 'Void Relic', reward: 2_000}, position: {x: 59, y: 30}, guardPosition: {x: 58, y: 30}},
    {loot: {name: 'Medical Cache', reward: 650}, position: {x: 44, y: 45}, guardPosition: {x: 44, y: 46}},
    {loot: {name: 'Gold Chips', reward: 1_050}, position: {x: 20, y: 38}, guardPosition: {x: 19, y: 38}},
    {loot: {name: 'Navigation Seed', reward: 1_700}, position: {x: 8, y: 56}, guardPosition: {x: 8, y: 55}},
    {loot: {name: 'Royal Stasis Crown', reward: 2_500}, position: {x: 31, y: 58}, guardPosition: {x: 30, y: 58}},
  ];
  const obstacles: GridPosition[] = [
    ...line({x: 3, y: 2}, 6, 'horizontal'),
    ...line({x: 6, y: 14}, 8, 'vertical'),
    ...line({x: 17, y: 15}, 9, 'horizontal'),
    ...line({x: 32, y: 4}, 10, 'vertical'),
    ...line({x: 40, y: 27}, 12, 'horizontal'),
    ...line({x: 13, y: 31}, 8, 'vertical'),
    ...line({x: 27, y: 47}, 11, 'horizontal'),
    ...line({x: 51, y: 45}, 9, 'vertical'),
  ];

  return {
    cargoSpace: {
      width: CARGO_SIZE,
      height: CARGO_SIZE,
      obstacles,
      loots: lootCaches.map(({loot, position}) => ({loot, position})),
    },
    player: {
      health: 100,
      inventory: {width: 8, height: 8, obstacles: [], loots: []},
      agility: 5,
      stamina: 10,
      position: {x: 2, y: 2},
      weapon: starterWeapon,
    },
    npcs: lootCaches.map(({loot, guardPosition}) => new Npc({
      name: `${loot.name} Keeper`,
      health: 30,
      position: guardPosition,
      weapon: {name: 'Watchman Carbine', type: 'laser', damage: 8, weight: 4},
      guardedLootName: loot.name,
    })),
    lastEvent: `${lootCaches.length} guarded loot caches are scattered through the cargo hold.`,
  };
}

export function movePlayer(game: GameInstance, movement: GridPosition): GameInstance {
  if (game.player.health <= 0) return {...game, lastEvent: 'You are incapacitated and cannot move.'};

  const {cargoSpace, player} = game;
  const position = {
    x: clamp(player.position.x + movement.x, 0, cargoSpace.width - 1),
    y: clamp(player.position.y + movement.y, 0, cargoSpace.height - 1),
  };
  if (game.cargoSpace.obstacles.some((obstacle) => samePosition(obstacle, position))) {
    return {...game, lastEvent: 'A cargo bulkhead blocks your path.'};
  }
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
    const playerDistance = distance(npc.position, player.position);
    if (playerDistance > ATTACK_DISTANCE && playerDistance <= GUARD_DETECTION_DISTANCE) {
      const occupiedPositions = npcs
        .filter((otherNpc, otherIndex) => otherIndex !== index && otherNpc.isAlive)
        .map(({position}) => position);
      const path = findPath(npc.position, player.position, game.cargoSpace, [
        ...game.cargoSpace.obstacles,
        ...occupiedPositions,
      ]);
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

function line(start: GridPosition, length: number, direction: 'horizontal' | 'vertical'): GridPosition[] {
  return Array.from({length}, (_, index) => direction === 'horizontal'
    ? {x: start.x + index, y: start.y}
    : {x: start.x, y: start.y + index});
}
