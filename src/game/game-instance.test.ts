import {describe, expect, it} from 'bun:test';
import {createGameInstance, movePlayer} from './game-instance.js';

describe('team mates', () => {
  it('spawns a small squad with the player', () => {
    const game = createGameInstance();

    expect(game.teammates).toHaveLength(3);
    expect(game.teammates.every((teammate) => teammate.isAlive)).toBe(true);
    expect(game.teammates.map((teammate) => teammate.name)).toEqual(['Nova', 'Kite', 'Rook']);
  });

  it('keeps teammates spaced out around the player', () => {
    const game = createGameInstance();

    const teammatePositions = game.teammates.map((teammate) => teammate.position);
    expect(teammatePositions.every((position) => Math.abs(position.x - game.player.position.x) >= 2 || Math.abs(position.y - game.player.position.y) >= 2)).toBe(true);
    expect(teammatePositions.every((position) => Math.abs(position.x - game.player.position.x) <= 5 && Math.abs(position.y - game.player.position.y) <= 5)).toBe(true);
  });

  it('blocks the player from stepping onto a teammate', () => {
    const game = createGameInstance();
    const nextGame = movePlayer(game, {x: -2, y: 0});

    expect(nextGame.player.position).toEqual({x: 2, y: 2});
    expect(nextGame.lastEvent).toContain('Nova is in the way');
  });

  it('allows the player to move away from teammates after collecting the top-left loot', () => {
    const game = createGameInstance();
    const afterLoot = movePlayer(game, {x: -2, y: -2});

    expect(afterLoot.player.position).toEqual({x: 0, y: 0});
    expect(afterLoot.lastEvent).toContain('Captured Neural Core');

    const nextMove = movePlayer(afterLoot, {x: 0, y: 1});

    expect(nextMove.player.position).toEqual({x: 0, y: 1});
    expect(nextMove.lastEvent).toContain('Moved to (0, 1)');
  });
});
