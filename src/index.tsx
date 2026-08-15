import React, {useState} from 'react';
import {Box, render, Text, useApp, useInput, useWindowSize} from 'ink';
import {advanceNpcs, createGameInstance, movePlayer, shootWeapon, type GameInstance} from './game/game-instance.js';

const NPC_REACTION_DELAY_MS = 500;
const STATUS_ROWS = 6;
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
        <Text bold color="cyan" wrap="truncate-end">DOZD // Cargo Hold</Text>
        <Text wrap="truncate-end">Position: ({game.player.position.x}, {game.player.position.y}) / 63, 63 · Health: {game.player.health}</Text>
        <Text wrap="truncate-end">Weapon: {game.player.weapon.name} ({game.player.weapon.damage} damage) · Inventory: {game.player.inventory.loots.length} loot</Text>
        <Text color="yellow" wrap="truncate-end">{game.lastEvent}</Text>
        <Text color={isFiring ? 'red' : 'magenta'} wrap="truncate-end">{transientStatus}</Text>
        <Text> </Text>
      </Box>
      {makeViewport(game, viewportSize).map((row, index) => <Text key={index}>{row}</Text>)}
      <Box flexDirection="column" height={FOOTER_ROWS} width={columns}>
        <Text dimColor wrap="truncate-end">Move: arrow keys or WASD · Fire: F, then direction · Q = quit</Text>
        <Text dimColor wrap="truncate-end">P = player · N = armed defender · L = loot · # = bulkhead</Text>
      </Box>
    </Box>
  );
}

function makeViewport(game: GameInstance, viewportSize: number): string[] {
  const halfSize = Math.floor(viewportSize / 2);
  const originX = clamp(game.player.position.x - halfSize, 0, game.cargoSpace.width - viewportSize);
  const originY = clamp(game.player.position.y - halfSize, 0, game.cargoSpace.height - viewportSize);

  return Array.from({length: viewportSize}, (_, row) =>
    Array.from({length: viewportSize}, (_, column) => {
      const x = originX + column;
      const y = originY + row;
      if (game.player.position.x === x && game.player.position.y === y) return 'P';
      if (game.npcs.some((npc) => npc.isAlive && npc.position.x === x && npc.position.y === y)) return 'N';
      if (game.cargoSpace.loots.some(({position}) => position.x === x && position.y === y)) return 'L';
      if (game.cargoSpace.obstacles.some((obstacle) => obstacle.x === x && obstacle.y === y)) return '#';
      return '·';
    }).join(' '),
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
