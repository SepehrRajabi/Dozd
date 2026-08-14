import type {GridPosition} from '../models/index.js';

export interface GridBounds {
  width: number;
  height: number;
}

interface PathNode {
  position: GridPosition;
  cost: number;
  estimatedTotalCost: number;
}

/** Finds a shortest four-direction path using the A* algorithm. The start is excluded from the result. */
export function findPath(
  start: GridPosition,
  goal: GridPosition,
  bounds: GridBounds,
  blockedPositions: GridPosition[] = [],
): GridPosition[] {
  if (samePosition(start, goal)) return [];

  const blocked = new Set(blockedPositions.map(positionKey));
  const open = new Map<string, PathNode>();
  const cameFrom = new Map<string, GridPosition>();
  const costFromStart = new Map<string, number>([[positionKey(start), 0]]);
  const closed = new Set<string>();
  open.set(positionKey(start), {position: start, cost: 0, estimatedTotalCost: distance(start, goal)});

  while (open.size > 0) {
    const current = lowestCostNode(open);
    const currentKey = positionKey(current.position);
    open.delete(currentKey);

    if (samePosition(current.position, goal)) return rebuildPath(cameFrom, current.position);
    closed.add(currentKey);

    for (const neighbor of neighbors(current.position, bounds)) {
      const neighborKey = positionKey(neighbor);
      if (closed.has(neighborKey) || (blocked.has(neighborKey) && !samePosition(neighbor, goal))) continue;

      const tentativeCost = current.cost + 1;
      if (tentativeCost >= (costFromStart.get(neighborKey) ?? Infinity)) continue;

      cameFrom.set(neighborKey, current.position);
      costFromStart.set(neighborKey, tentativeCost);
      open.set(neighborKey, {
        position: neighbor,
        cost: tentativeCost,
        estimatedTotalCost: tentativeCost + distance(neighbor, goal),
      });
    }
  }

  return [];
}

function lowestCostNode(open: Map<string, PathNode>): PathNode {
  return [...open.values()].reduce((best, candidate) =>
    candidate.estimatedTotalCost < best.estimatedTotalCost ? candidate : best,
  );
}

function rebuildPath(cameFrom: Map<string, GridPosition>, goal: GridPosition): GridPosition[] {
  const path = [goal];
  let current = goal;

  while (cameFrom.has(positionKey(current))) {
    current = cameFrom.get(positionKey(current))!;
    path.unshift(current);
  }

  return path.slice(1);
}

function neighbors(position: GridPosition, bounds: GridBounds): GridPosition[] {
  return [
    {x: position.x + 1, y: position.y},
    {x: position.x - 1, y: position.y},
    {x: position.x, y: position.y + 1},
    {x: position.x, y: position.y - 1},
  ].filter(({x, y}) => x >= 0 && y >= 0 && x < bounds.width && y < bounds.height);
}

function distance(first: GridPosition, second: GridPosition): number {
  return Math.abs(first.x - second.x) + Math.abs(first.y - second.y);
}

function samePosition(first: GridPosition, second: GridPosition): boolean {
  return first.x === second.x && first.y === second.y;
}

function positionKey({x, y}: GridPosition): string {
  return `${x},${y}`;
}
