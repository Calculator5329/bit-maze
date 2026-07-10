export const TRIAL_BYTES = new Uint8Array([
  0x42, 0x4d, 0x01, 0x03, 0x08, 0x06, 0x03, 0x00,
  0xff, 0x85, 0x85, 0x85, 0x85, 0xff,
  0x00, 0x02, 0x00, 0x00, 0x42, 0x00,
  0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x01, 0x80,
  0x06, 0x00, 0x10, 0x05, 0x10, 0x03, 0x32, 0x01,
]);

export const SPRITES = {
  wall: [0xff, 0xee, 0xee, 0x00, 0xbb, 0xbb, 0x00, 0xff],
  floor: [0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00],
  player: [0x3c, 0x42, 0x9d, 0xa5, 0xa5, 0x9e, 0x40, 0x3c],
  item: [0x18, 0x3c, 0x7e, 0xff, 0x7e, 0x3c, 0x18, 0x00],
  hazard: [0x44, 0x44, 0x44, 0xee, 0xee, 0xee, 0xff, 0xff],
  plate: [0x00, 0x00, 0x3c, 0x42, 0x42, 0x3c, 0x00, 0x00],
};

export const PALETTE = {
  wallInk: "#455074",
  wallPaper: "#151a2c",
  floorInk: "#202338",
  floorPaper: "#090b12",
  player: "#ffb300",
  item: "#00e5ff",
  hazard: "#ff453a",
  plate: "#a855f7",
};

function readBit(plane, width, x, y) {
  const index = y * width + x;
  return ((plane[index >> 3] >> (7 - (index & 7))) & 1) === 1;
}

function writeBit(plane, width, x, y, value) {
  const index = y * width + x;
  const mask = 1 << (7 - (index & 7));
  if (value) plane[index >> 3] |= mask;
  else plane[index >> 3] &= ~mask;
}

export function parseLevel(bytes) {
  if (bytes.length < 8 || bytes[0] !== 0x42 || bytes[1] !== 0x4d) {
    throw new Error("Not a bit-maze level");
  }
  if (bytes[2] !== 1) throw new Error(`Unsupported .bm version ${bytes[2]}`);
  const flags = bytes[3];
  const width = bytes[4];
  const height = bytes[5];
  const planeCount = bytes[6];
  const planeLength = Math.ceil((width * height) / 8);
  let cursor = 8;
  const planes = [];
  for (let i = 0; i < planeCount; i += 1) {
    planes.push(bytes.slice(cursor, cursor + planeLength));
    cursor += planeLength;
  }
  const triggers = flags & 1 ? bytes.slice(cursor, cursor += width * height) : null;
  const scripts = new Map();
  if (flags & 2) {
    const count = bytes[cursor++];
    for (let i = 0; i < count; i += 1) {
      const id = bytes[cursor++];
      const length = bytes[cursor] | (bytes[cursor + 1] << 8);
      cursor += 2;
      scripts.set(id, bytes.slice(cursor, cursor + length));
      cursor += length;
    }
  }
  if (cursor !== bytes.length) throw new Error("Malformed .bm payload");
  return { width, height, planes, triggers, scripts };
}

export class BitMazeGame {
  constructor(bytes = TRIAL_BYTES) {
    this.level = parseLevel(bytes);
    this.score = 0;
    this.state = "playing";
    this.moves = 0;
    this.fired = new Set();
    this.totalItems = this.countPlane(1);
    this.spawn();
    this.collectItem();
  }

  get width() { return this.level.width; }
  get height() { return this.level.height; }
  getBit(plane, x, y) {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height || !this.level.planes[plane]) return false;
    return readBit(this.level.planes[plane], this.width, x, y);
  }
  setBit(plane, x, y, value) {
    if (x >= 0 && y >= 0 && x < this.width && y < this.height && this.level.planes[plane]) {
      writeBit(this.level.planes[plane], this.width, x, y, value);
    }
  }
  countPlane(plane) {
    let count = 0;
    for (let y = 0; y < this.height; y += 1) for (let x = 0; x < this.width; x += 1) count += this.getBit(plane, x, y) ? 1 : 0;
    return count;
  }
  spawn() {
    for (let y = 0; y < this.height; y += 1) for (let x = 0; x < this.width; x += 1) {
      if (!this.getBit(0, x, y)) { this.x = x; this.y = y; return; }
    }
    throw new Error("Level has no floor tile");
  }
  collectItem() {
    if (this.getBit(1, this.x, this.y)) {
      this.setBit(1, this.x, this.y, false);
      this.score += 1;
      if (this.totalItems > 0 && this.score === this.totalItems) this.state = "won";
    }
  }
  triggerAt(x, y) {
    return this.level.triggers?.[y * this.width + x] ?? 0;
  }
  move(dx, dy) {
    if (this.state !== "playing") return "idle";
    const nx = this.x + dx;
    const ny = this.y + dy;
    if (nx < 0 || ny < 0 || nx >= this.width || ny >= this.height || this.getBit(0, nx, ny)) return "blocked";
    this.x = nx;
    this.y = ny;
    this.moves += 1;
    if (this.getBit(2, nx, ny)) { this.state = "lost"; return "lost"; }
    this.collectItem();
    if (this.state === "playing") this.fireTrigger();
    return this.state;
  }
  fireTrigger() {
    const index = this.y * this.width + this.x;
    const id = this.triggerAt(this.x, this.y);
    const script = this.level.scripts.get(id);
    if (!id || !script || ((id & 0x80) && this.fired.has(index))) return false;
    if (id & 0x80) this.fired.add(index);
    this.runScript(script);
    return true;
  }
  runScript(script) {
    const stack = [];
    const ram = new Uint8Array(256);
    let pc = 0;
    let budget = 4096;
    const pop = () => stack.pop();
    while (pc < script.length && budget-- > 0) {
      const op = script[pc++];
      if (op === 0x00) continue;
      if (op === 0x01) return;
      if (op === 0x10) stack.push(script[pc++]);
      else if (op === 0x11) { stack.push(script[pc] | (script[pc + 1] << 8)); pc += 2; }
      else if (op === 0x12) pop();
      else if (op === 0x13) stack.push(stack.at(-1));
      else if (op === 0x20) { const b = pop(); stack.push((pop() + b) & 0xffff); }
      else if (op === 0x21) { const b = pop(); stack.push((pop() - b) & 0xffff); }
      else if (op === 0x30) { const y = pop(), x = pop(); stack.push(this.getBit(0, x, y) ? 1 : 0); }
      else if (op === 0x31 || op === 0x32) { const y = pop(), x = pop(); this.setBit(0, x, y, op === 0x31); }
      else if (op === 0x40) stack.push(this.x);
      else if (op === 0x41) stack.push(this.y);
      else if (op === 0x42) { const y = pop(), x = pop(); stack.push(this.getBit(1, x, y) ? 1 : 0); }
      else if (op === 0x43) stack.push(this.score & 0xffff);
      else if (op === 0x44) { const y = pop(), x = pop(); stack.push(this.getBit(2, x, y) ? 1 : 0); }
      else if (op === 0x60 || op === 0x61) {
        let offset = script[pc++]; if (offset > 127) offset -= 256;
        if (op === 0x60 || pop() === 0) pc += offset;
      } else if (op === 0x70) stack.push(ram[pop() & 0xff]);
      else if (op === 0x71) { const address = pop(); ram[address & 0xff] = pop() & 0xff; }
      else return;
      if (stack.length > 64 || pc < 0 || pc > script.length) return;
    }
  }
}

function blit(ctx, tx, ty, rows, ink, paper, scale) {
  for (let y = 0; y < 8; y += 1) for (let x = 0; x < 8; x += 1) {
    const set = ((rows[y] >> (7 - x)) & 1) === 1;
    if (set || paper) {
      ctx.fillStyle = set ? ink : paper;
      ctx.fillRect((tx * 8 + x) * scale, (ty * 8 + y) * scale, scale, scale);
    }
  }
}

export function drawGame(ctx, game, scale = 8) {
  ctx.imageSmoothingEnabled = false;
  for (let y = 0; y < game.height; y += 1) for (let x = 0; x < game.width; x += 1) {
    if (game.getBit(0, x, y)) blit(ctx, x, y, SPRITES.wall, PALETTE.wallInk, PALETTE.wallPaper, scale);
    else {
      blit(ctx, x, y, SPRITES.floor, PALETTE.floorInk, PALETTE.floorPaper, scale);
      if (game.triggerAt(x, y)) blit(ctx, x, y, SPRITES.plate, PALETTE.plate, null, scale);
      if (game.getBit(2, x, y) && !(game.x === x && game.y === y)) blit(ctx, x, y, SPRITES.hazard, PALETTE.hazard, null, scale);
      if (game.getBit(1, x, y) && !(game.x === x && game.y === y)) blit(ctx, x, y, SPRITES.item, PALETTE.item, null, scale);
      if (game.x === x && game.y === y) blit(ctx, x, y, SPRITES.player, PALETTE.player, null, scale);
    }
  }
}
