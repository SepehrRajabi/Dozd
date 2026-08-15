import React, {useState} from 'react';
import {Box, render, Text, useApp, useInput} from 'ink';
import {advanceNpcs, createGameInstance, movePlayer, shootWeapon, type GameInstance} from './game/game-instance.js';

const VIEWPORT_SIZE = 17;
const NPC_REACTION_DELAY_MS = 500;

function App() {
  const [game, setGame] = useState<GameInstance>(createGameInstance);
  const [isFiring, setIsFiring] = useState(false);
  const [isWaitingForNpcs, setIsWaitingForNpcs] = useState(false);
  const {exit} = useApp();

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
      <Text bold color="cyan">DOZD // Cargo Hold</Text>
      <Text>Position: ({game.player.position.x}, {game.player.position.y}) / 63, 63</Text>
      <Text>Health: {game.player.health}</Text>
      <Text>Weapon: {game.player.weapon.name} ({game.player.weapon.damage} damage)</Text>
      <Text>Inventory: {game.player.inventory.loots.length} loot</Text>
      <Text color="yellow">{game.lastEvent}</Text>
      {isFiring && <Text color="red">Firing mode: choose a direction.</Text>}
      {isWaitingForNpcs && <Text color="magenta">Defenders are reacting…</Text>}
      <Text> </Text>
      {makeViewport(game).map((row, index) => <Text key={index}>{row}</Text>)}
      <Text> </Text>
      <Text dimColor>Move: arrow keys or WASD · Fire: F, then direction · Q = quit</Text>
      <Text dimColor>P = player · N = armed defender · L = loot · # = bulkhead</Text>
    </Box>
  );
}

function makeViewport(game: GameInstance): string[] {
  const halfSize = Math.floor(VIEWPORT_SIZE / 2);
  const originX = clamp(game.player.position.x - halfSize, 0, game.cargoSpace.width - VIEWPORT_SIZE);
  const originY = clamp(game.player.position.y - halfSize, 0, game.cargoSpace.height - VIEWPORT_SIZE);

  return Array.from({length: VIEWPORT_SIZE}, (_, row) =>
    Array.from({length: VIEWPORT_SIZE}, (_, column) => {
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

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(value, maximum));
}

render(<App />);
