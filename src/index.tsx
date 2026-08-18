import React, {useState} from 'react';
import {Box, render, Text, useApp, useInput, useWindowSize} from 'ink';
import {advanceNpcs, createGameInstance, movePlayer, shootWeapon, type GameInstance} from './game/game-instance.js';

const NPC_REACTION_DELAY_MS = 220;
const STATUS_ROWS = 7;
const FOOTER_ROWS = 2;

function App() {
  const [game, setGame] = useState<GameInstance>(createGameInstance);
  const [isFiring, setIsFiring] = useState(false);
  const [isWaitingForNpcs, setIsWaitingForNpcs] = useState(false);
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
      const nextGame = isFiring ? shootWeapon(game, movement) : movePlayer(game, movement);
      setGame(nextGame);
      setIsFiring(false);
      scheduleNpcTurn(nextGame);
    }
  });

  function scheduleNpcTurn(nextGame: GameInstance): void {
    if (!nextGame.npcs.some((npc) => npc.isAlive) || nextGame.player.health <= 0) return;

    setIsWaitingForNpcs(true);
    setTimeout(() => {
      setGame((currentGame) => advanceNpcs(currentGame));
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
      {makeViewport(game, viewportSize).map((row, rowIndex) => (
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

function makeViewport(game: GameInstance, viewportSize: number): GridCell[][] {
  const halfSize = Math.floor(viewportSize / 2);
  const originX = clamp(game.player.position.x - halfSize, 0, game.cargoSpace.width - viewportSize);
  const originY = clamp(game.player.position.y - halfSize, 0, game.cargoSpace.height - viewportSize);

  return Array.from({length: viewportSize}, (_, row) =>
    Array.from({length: viewportSize}, (_, column): GridCell => {
      const x = originX + column;
      const y = originY + row;
      if (game.player.position.x === x && game.player.position.y === y) return {symbol: 'P', color: 'cyan', bold: true};
      if (game.teammates.some((teammate) => teammate.isAlive && teammate.position.x === x && teammate.position.y === y)) return {symbol: 'T', color: 'green', bold: true};
      if (game.npcs.some((npc) => npc.isAlive && npc.position.x === x && npc.position.y === y)) return {symbol: 'N', color: 'red', bold: true};
      if (game.cargoSpace.loots.some(({position}) => position.x === x && position.y === y)) return {symbol: 'L', color: 'yellow', bold: true};
      if (game.cargoSpace.obstacles.some((obstacle) => obstacle.x === x && obstacle.y === y)) return {symbol: '#', color: 'gray'};
      return {symbol: '·', dimColor: true};
    }),
  );
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
