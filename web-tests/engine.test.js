import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { BitMazeGame, decodeGateScript, inspectTile, parseLevel } from "../lib/bitmaze.js";

const trialBytes = fs.readFileSync("levels/trial.bm");
const circuitBytes = fs.readFileSync("levels/circuit.bm");

test("browser engine parses the committed trial payload", () => {
  const level = parseLevel(trialBytes);
  assert.equal(level.width, 8);
  assert.equal(level.height, 6);
  assert.equal(level.planes.length, 3);
  assert.deepEqual([...level.scripts.get(0x80)], [0x10, 5, 0x10, 3, 0x32, 1]);
});

test("trial can be won and the trigger opens its gate", () => {
  const game = new BitMazeGame(trialBytes);
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
  const game = new BitMazeGame(trialBytes);
  game.move(1, 0);
  game.move(0, 1);
  game.move(0, 1);
  assert.equal(game.state, "lost");
});

test("browser engine parses the 24x16 circuit and both gate scripts", () => {
  const level = parseLevel(circuitBytes);
  assert.equal(circuitBytes.length, 555);
  assert.equal(level.width, 24);
  assert.equal(level.height, 16);
  assert.equal(level.planes.length, 3);
  assert.deepEqual([...level.scripts.get(0x80)], [0x10, 8, 0x10, 4, 0x32, 1]);
  assert.deepEqual([...level.scripts.get(0x81)], [0x10, 16, 0x10, 11, 0x32, 1]);

  const game = new BitMazeGame(circuitBytes);
  assert.equal(game.totalItems, 12);
  game.x = 5; game.y = 2;
  assert.equal(game.fireTrigger(), true);
  assert.equal(game.getBit(0, 8, 4), false);
  game.x = 12; game.y = 13;
  assert.equal(game.fireTrigger(), true);
  assert.equal(game.getBit(0, 16, 11), false);
});

test("bit inspector exposes tile addressing and decodes gate bytecode", () => {
  const game = new BitMazeGame(circuitBytes);
  game.x = 5; game.y = 2;
  const tile = inspectTile(game);
  assert.deepEqual(
    { index: tile.index, byteIndex: tile.byteIndex, bitIndex: tile.bitIndex, mask: tile.mask, trigger: tile.trigger },
    { index: 53, byteIndex: 6, bitIndex: 2, mask: 0x04, trigger: 0x80 },
  );
  assert.equal(tile.planes[0].binary.length, 8);
  assert.deepEqual(decodeGateScript(tile.script), {
    x: 8,
    y: 4,
    text: "PUSH8 8  →  PUSH8 4  →  CLR_WALL  →  HALT",
  });
});
