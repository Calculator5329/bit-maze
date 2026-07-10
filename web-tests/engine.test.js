import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { BitMazeGame, parseLevel, TRIAL_BYTES } from "../lib/bitmaze.js";

test("browser engine parses the committed trial payload", () => {
  assert.deepEqual([...TRIAL_BYTES], [...fs.readFileSync("levels/trial.bm")]);
  const level = parseLevel(TRIAL_BYTES);
  assert.equal(level.width, 8);
  assert.equal(level.height, 6);
  assert.equal(level.planes.length, 3);
  assert.deepEqual([...level.scripts.get(0x80)], [0x10, 5, 0x10, 3, 0x32, 1]);
});

test("trial can be won and the trigger opens its gate", () => {
  const game = new BitMazeGame();
  const moves = [
    [1,0],[1,0], [0,1],[0,1], [1,0],[1,0],[1,0],
    [0,-1],[0,-1], [0,1],[0,1],[0,1], [0,-1],
    [-1,0],[-1,0], [0,1], [-1,0],[-1,0],[-1,0],
  ];
  for (const move of moves) game.move(...move);
  assert.equal(game.getBit(0, 5, 3), false);
  assert.equal(game.state, "won");
  assert.equal(game.score, 3);
});

test("trial hazard ends the run", () => {
  const game = new BitMazeGame();
  game.move(1, 0);
  game.move(0, 1);
  game.move(0, 1);
  assert.equal(game.state, "lost");
});
