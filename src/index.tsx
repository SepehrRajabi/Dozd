import React, {useEffect, useState} from 'react';
import {Box, render, Text, useApp, useInput, useWindowSize} from 'ink';
import {advanceNpcs, createGameInstance, getShotDistance, movePlayer, shootWeapon, type GameInstance, type ShotEvent} from './game/game-instance.js';
import type {GridPosition} from './models/index.js';

const NPC_REACTION_DELAY_MS = 220;
const STATUS_ROWS = 7;
const FOOTER_ROWS = 2;
const PROJECTILE_STEP_MS = 35;
const PROJECTILE_TAIL_LENGTH = 3;
const PLAYER_COLOR = 'cyan';
const TEAMMATE_COLOR = 'green';
const NPC_COLOR = 'red';

interface Projectile {
  path: GridPosition[];
  headDistance: number;
  color: string;
}

function App() {
  const [game, setGame] = useState<GameInstance>(createGameInstance);
  const [isFiring, setIsFiring] = useState(false);
  const [isWaitingForNpcs, setIsWaitingForNpcs] = useState(false);
  const [projectiles, setProjectiles] = useState<Projectile[]>([]);
  const {exit} = useApp();
  const {columns, rows} = useWindowSize();
  const viewportSize = getViewportSize(columns, rows);
  const transientStatus = isFiring
    ? 'Firing mode: choose a direction.'
    : isWaitingForNpcs
      ? 'Defenders are reacting…'
      : ' ';

  useInput((input, key) => {
    if (input === 'q') {
      exit();
      return;
    }

    if (isWaitingForNpcs) return;

    if (input === 'f') {
      setIsFiring(true);
      return;
    }

    const movement = key.upArrow || input === 'w'
      ? {x: 0, y: -1}
      : key.downArrow || input === 's'
        ? {x: 0, y: 1}
        : key.leftArrow || input === 'a'
          ? {x: -1, y: 0}
          : key.rightArrow || input === 'd'
            ? {x: 1, y: 0}
            : undefined;

    if (movement) {
      if (isFiring) {
        const origin = game.player.position;
        const shotDistance = Math.max(1, getShotDistance(game, movement));
        const target = {x: origin.x + movement.x * shotDistance, y: origin.y + movement.y * shotDistance};
        const nextGame = shootWeapon(game, movement);
        setGame(nextGame);
        addProjectile({path: tracePath(origin, target), headDistance: 1, color: PLAYER_COLOR});
        setIsFiring(false);
        scheduleNpcTurn(nextGame);
      } else {
        const nextGame = movePlayer(game, movement);
        setGame(nextGame);
        scheduleNpcTurn(nextGame);
      }
    }
  });

  useEffect(() => {
    if (projectiles.length === 0) return;

    const timer = setTimeout(() => {
      setProjectiles((current) => current
        .map((projectile) => ({...projectile, headDistance: projectile.headDistance + 1}))
        .filter((projectile) => projectile.headDistance - PROJECTILE_TAIL_LENGTH <= projectile.path.length));
    }, PROJECTILE_STEP_MS);

    return () => clearTimeout(timer);
  }, [projectiles]);

  function addProjectile(projectile: Projectile): void {
    setProjectiles((current) => [...current, projectile]);
  }

  function shotToProjectile(shot: ShotEvent): Projectile {
    return {path: tracePath(shot.origin, shot.target), headDistance: 1, color: shot.source === 'npc' ? NPC_COLOR : TEAMMATE_COLOR};
  }

  function scheduleNpcTurn(nextGame: GameInstance): void {
    if (!nextGame.npcs.some((npc) => npc.isAlive) || nextGame.player.health <= 0) return;

    setIsWaitingForNpcs(true);
    setTimeout(() => {
      setGame((currentGame) => {
        const {game: advancedGame, shots} = advanceNpcs(currentGame);
        if (shots.length > 0) {
          setProjectiles((current) => [...current, ...shots.map(shotToProjectile)]);
        }
        return advancedGame;
      });
      setIsWaitingForNpcs(false);
    }, NPC_REACTION_DELAY_MS);
  }

  return (
    <Box flexDirection="column">
      <Box flexDirection="column" height={STATUS_ROWS} width={columns}>
        <Box flexDirection="row" justifyContent="space-between">
          <Text bold color="cyan" wrap="truncate-end">DOZD // Cargo Hold</Text>
          <Text color={isWaitingForNpcs ? 'magenta' : 'green'} wrap="truncate-end">{isWaitingForNpcs ? 'DEFENDERS REACTING' : 'READY'}</Text>
        </Box>
        <Box flexDirection="row">
          <Text wrap="truncate-end">Pos: ({game.player.position.x}, {game.player.position.y})</Text>
          <Text wrap="truncate-end"> · HP: {game.player.health}</Text>
          <Text wrap="truncate-end"> · Weapon: {game.player.weapon.name}</Text>
        </Box>
        <Box flexDirection="row">
          <Text wrap="truncate-end">Squad: {game.teammates.filter((teammate) => teammate.isAlive).map((teammate) => teammate.name).join(', ') || 'No allies left'}</Text>
          <Text wrap="truncate-end"> · Loot: {game.player.inventory.loots.length}</Text>
        </Box>
        <Text color="yellow" wrap="truncate-end">{game.lastEvent}</Text>
        <Text color={isFiring ? 'red' : 'magenta'} wrap="truncate-end">{transientStatus}</Text>
        <Text> </Text>
      </Box>
      {makeViewport(game, viewportSize, projectiles).map((row, rowIndex) => (
        <Box key={rowIndex} flexDirection="row">
          {row.map((cell, columnIndex) => (
            <Text key={columnIndex} color={cell.color} bold={cell.bold} dimColor={cell.dimColor}>{cell.symbol} </Text>
          ))}
        </Box>
      ))}
      <Box flexDirection="column" height={FOOTER_ROWS} width={columns}>
        <Text dimColor wrap="truncate-end">Move: WASD / Arrows · Fire: F + direction · Quit: Q</Text>
        <Text dimColor wrap="truncate-end">Legend: P player · T teammate · N defender · L loot · # bulkhead</Text>
      </Box>
    </Box>
  );
}

interface GridCell {
  symbol: string;
  color?: string;
  bold?: boolean;
  dimColor?: boolean;
}

function makeViewport(game: GameInstance, viewportSize: number, projectiles: Projectile[]): GridCell[][] {
  const halfSize = Math.floor(viewportSize / 2);
  const originX = clamp(game.player.position.x - halfSize, 0, game.cargoSpace.width - viewportSize);
  const originY = clamp(game.player.position.y - halfSize, 0, game.cargoSpace.height - viewportSize);
  const projectileCells = buildProjectileCells(projectiles);

  return Array.from({length: viewportSize}, (_, row) =>
    Array.from({length: viewportSize}, (_, column): GridCell => {
      const x = originX + column;
      const y = originY + row;
      if (game.player.position.x === x && game.player.position.y === y) return {symbol: 'P', color: PLAYER_COLOR, bold: true};
      if (game.teammates.some((teammate) => teammate.isAlive && teammate.position.x === x && teammate.position.y === y)) return {symbol: 'T', color: TEAMMATE_COLOR, bold: true};
      if (game.npcs.some((npc) => npc.isAlive && npc.position.x === x && npc.position.y === y)) return {symbol: 'N', color: NPC_COLOR, bold: true};
      if (game.cargoSpace.loots.some(({position}) => position.x === x && position.y === y)) return {symbol: 'L', color: 'yellow', bold: true};
      if (game.cargoSpace.obstacles.some((obstacle) => obstacle.x === x && obstacle.y === y)) return {symbol: '#', color: 'gray'};
      const projectileCell = projectileCells.get(`${x},${y}`);
      if (projectileCell) return projectileCell;
      return {symbol: '·', dimColor: true};
    }),
  );
}

/** Straight grid-cell path from origin to target, excluding origin, capped at the Chebyshev distance between them. */
function tracePath(origin: GridPosition, target: GridPosition): GridPosition[] {
  const dx = target.x - origin.x;
  const dy = target.y - origin.y;
  const steps = Math.max(Math.abs(dx), Math.abs(dy), 1);
  return Array.from({length: steps}, (_, index) => {
    const t = (index + 1) / steps;
    return {x: Math.round(origin.x + dx * t), y: Math.round(origin.y + dy * t)};
  });
}

function buildProjectileCells(projectiles: Projectile[]): Map<string, GridCell> {
  const cells = new Map<string, GridCell>();

  for (const projectile of projectiles) {
    const tailStart = Math.max(1, projectile.headDistance - PROJECTILE_TAIL_LENGTH + 1);

    for (let step = tailStart; step <= projectile.headDistance; step++) {
      if (step < 1 || step > projectile.path.length) continue;
      const position = projectile.path[step - 1];
      const distanceFromHead = projectile.headDistance - step;
      cells.set(`${position.x},${position.y}`, projectileTailCell(distanceFromHead, projectile.color));
    }
  }

  return cells;
}

function projectileTailCell(distanceFromHead: number, color: string): GridCell {
  if (distanceFromHead <= 0) return {symbol: '•', color, bold: true};
  if (distanceFromHead === 1) return {symbol: '•', color};
  return {symbol: '·', color, dimColor: true};
}

function getViewportSize(columns: number, rows: number): number {
  const gridRows = Math.max(1, rows - STATUS_ROWS - FOOTER_ROWS);
  const gridColumns = Math.max(1, Math.floor((columns + 1) / 2));
  return Math.min(64, gridRows, gridColumns);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(value, maximum));
}

render(<App />);
