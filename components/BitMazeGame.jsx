"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { BitMazeGame as Engine, decodeGateScript, drawGame, inspectTile } from "../lib/bitmaze";

const DIRECTIONS = {
  ArrowUp: [0, -1], w: [0, -1], W: [0, -1],
  ArrowDown: [0, 1], s: [0, 1], S: [0, 1],
  ArrowLeft: [-1, 0], a: [-1, 0], A: [-1, 0],
  ArrowRight: [1, 0], d: [1, 0], D: [1, 0],
};

const LEVELS = {
  trial: {
    label: "TRIAL",
    file: "TRIAL.BM",
    description: "Recover 3 cyan bits. One violet plate opens the sealed gate.",
  },
  circuit: {
    label: "CIRCUIT 24×16",
    file: "CIRCUIT.BM",
    description: "Recover 12 bits across three sectors. Two plates open the divider gates.",
  },
};

function noticeFor(game, levelId) {
  if (game.state === "won") return "MAZE RESOLVED // all bits recovered";
  if (game.state === "lost") return "SIGNAL LOST // the red spike is fatal";
  if (game.fired.size) {
    const totalGates = levelId === "circuit" ? 2 : 1;
    return `Gate bit flipped: ${game.fired.size} / ${totalGates} passage${totalGates > 1 ? "s" : ""} open.`;
  }
  return LEVELS[levelId].description;
}

const hex = (value, width = 2) => `0x${value.toString(16).toUpperCase().padStart(width, "0")}`;

function BitPlane({ game, plane, label, tone, flash }) {
  const address = inspectTile(game);
  const data = address.planes[plane];
  const tileCount = game.width * game.height;
  return (
    <article className={`plane-card ${tone}`}>
      <header>
        <div><span>PLANE {plane}</span><strong>{label}</strong></div>
        <code>BYTE[{address.byteIndex}] = {data.binary}</code>
      </header>
      <div
        className="bit-grid"
        style={{ "--bit-cols": game.width, "--bit-size": game.width > 12 ? "13px" : "22px" }}
        aria-label={`${label} bitplane`}
      >
        {Array.from({ length: tileCount }, (_, index) => {
          const x = index % game.width;
          const y = Math.floor(index / game.width);
          const value = game.getBit(plane, x, y) ? 1 : 0;
          const classes = [
            "bit-cell",
            value ? "set" : "",
            index === address.index ? "current" : "",
            flash.has(`${plane}:${index}`) ? "changed" : "",
          ].filter(Boolean).join(" ");
          return <span className={classes} key={index}>{value}</span>;
        })}
      </div>
    </article>
  );
}

function BitInspector({ game, flash, lastVm, events }) {
  const tile = inspectTile(game);
  const currentBytes = tile.script ? [...tile.script].map((byte) => hex(byte)).join(" ") : "—";
  const vm = lastVm ?? (tile.script ? {
    id: tile.trigger,
    bytes: currentBytes,
    decoded: decodeGateScript(tile.script),
  } : null);

  return (
    <section className="inspector" aria-label="Live bit inspector">
      <div className="inspector-heading">
        <div>
          <p className="eyebrow">LIVE MEMORY VIEW</p>
          <h2>Watch the bits work</h2>
        </div>
        <p>The amber outline is your current tile. Cyan, red, and slate <b>1</b>s are set bits. Mutations flash when a collectible clears or BitVM opens a gate.</p>
      </div>

      <div className="address-strip">
        <div><small>COORDINATE</small><strong>({tile.x}, {tile.y})</strong></div>
        <div><small>TILE INDEX</small><strong>{tile.index}</strong><code>y × {game.width} + x</code></div>
        <div><small>BYTE ADDRESS</small><strong>{tile.byteIndex}</strong><code>floor(index / 8)</code></div>
        <div><small>BIT / MASK</small><strong>{tile.bitIndex} / {hex(tile.mask)}</strong><code>MSB-first</code></div>
        <div><small>TRIGGER BYTE</small><strong>{hex(tile.trigger)}</strong><code>trigger[{tile.index}]</code></div>
      </div>

      <div className="inspector-planes">
        <BitPlane game={game} plane={0} label="WALLS" tone="walls" flash={flash} />
        <BitPlane game={game} plane={1} label="ITEMS" tone="items" flash={flash} />
        <BitPlane game={game} plane={2} label="HAZARDS" tone="hazards" flash={flash} />
      </div>

      <div className="inspector-lower">
        <article className="vm-card">
          <header><span>BITVM TRACE</span><strong>{vm ? `SCRIPT ${hex(vm.id)}` : "IDLE"}</strong></header>
          <code className="vm-bytes">{vm?.bytes ?? "Step onto a violet plate to execute bytecode."}</code>
          <p>{vm?.decoded?.text ?? "No gate program has executed in this run."}</p>
          {vm?.decoded && <p className="mutation">WALLS[{vm.decoded.x},{vm.decoded.y}] &nbsp; 1 → 0</p>}
        </article>
        <article className="event-card">
          <header><span>EVENT TRACE</span><strong>NEWEST FIRST</strong></header>
          <ol>
            {events.map((event) => <li key={event.id}><i>&gt;</i> {event.text}</li>)}
          </ol>
        </article>
      </div>
    </section>
  );
}

export default function BitMazeGame({ levelData }) {
  const canvasRef = useRef(null);
  const engineRef = useRef(null);
  const eventSequence = useRef(0);
  const flashTimer = useRef(null);
  if (!engineRef.current) engineRef.current = new Engine(Uint8Array.from(levelData.circuit));
  const [activeLevel, setActiveLevel] = useState("circuit");
  const [snapshot, setSnapshot] = useState({ score: 0, total: 12, state: "playing", moves: 0, width: 24, height: 16 });
  const [notice, setNotice] = useState(LEVELS.circuit.description);
  const [events, setEvents] = useState([{ id: 0, text: "LOAD CIRCUIT.BM · 555 bytes · 3 bitplanes" }]);
  const [flash, setFlash] = useState(new Set());
  const [lastVm, setLastVm] = useState(null);

  const paint = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const game = engineRef.current;
    const scale = Math.min(8, Math.max(2, Math.floor(768 / (game.width * 8))));
    canvas.width = game.width * 8 * scale;
    canvas.height = game.height * 8 * scale;
    drawGame(canvas.getContext("2d"), game, scale);
  }, []);

  const sync = useCallback(() => {
    const game = engineRef.current;
    setSnapshot({ score: game.score, total: game.totalItems, state: game.state, moves: game.moves, width: game.width, height: game.height });
    setNotice(noticeFor(game, activeLevel));
    paint();
  }, [activeLevel, paint]);

  const move = useCallback((dx, dy) => {
    const game = engineRef.current;
    const before = { x: game.x, y: game.y, score: game.score, fired: game.fired.size };
    const result = game.move(dx, dy);
    const entries = [];
    const changed = [];
    if (result === "blocked") {
      entries.push(`BLOCK (${before.x + dx},${before.y + dy}) · wall or edge`);
    } else if (game.x !== before.x || game.y !== before.y) {
      const tile = inspectTile(game);
      entries.push(`MOVE (${game.x},${game.y}) · idx ${tile.index} · byte ${tile.byteIndex} · mask ${hex(tile.mask)}`);
      if (game.score > before.score) {
        changed.push(`1:${tile.index}`);
        entries.push(`ITEMS[${game.x},${game.y}] 1 → 0 · score ${game.score}/${game.totalItems}`);
      }
      if (game.fired.size > before.fired) {
        const id = game.triggerAt(game.x, game.y);
        const script = game.level.scripts.get(id);
        const decoded = decodeGateScript(script);
        setLastVm({ id, bytes: [...script].map((byte) => hex(byte)).join(" "), decoded });
        entries.push(`VM ${hex(id)} · ${decoded?.text ?? "script executed"}`);
        if (decoded) changed.push(`0:${decoded.y * game.width + decoded.x}`);
      }
      if (game.state === "lost") entries.push("HAZARDS bit = 1 · state PLAYING → LOST");
      if (game.state === "won") entries.push("remaining ITEMS bits = 0 · state PLAYING → WON");
    }
    if (entries.length) {
      const tagged = entries.map((text) => ({ id: ++eventSequence.current, text })).reverse();
      setEvents((previous) => [...tagged, ...previous].slice(0, 8));
    }
    if (changed.length) {
      setFlash(new Set(changed));
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
      flashTimer.current = window.setTimeout(() => setFlash(new Set()), 900);
    }
    sync();
  }, [sync]);

  const reset = useCallback(() => {
    engineRef.current = new Engine(Uint8Array.from(levelData[activeLevel]));
    setEvents([{ id: ++eventSequence.current, text: `RESET ${LEVELS[activeLevel].file} · runtime bits restored` }]);
    setFlash(new Set());
    setLastVm(null);
    sync();
  }, [activeLevel, levelData, sync]);

  const selectLevel = useCallback((levelId) => {
    const game = new Engine(Uint8Array.from(levelData[levelId]));
    engineRef.current = game;
    setActiveLevel(levelId);
    setSnapshot({ score: 0, total: game.totalItems, state: "playing", moves: 0, width: game.width, height: game.height });
    setNotice(LEVELS[levelId].description);
    setEvents([{ id: ++eventSequence.current, text: `LOAD ${LEVELS[levelId].file} · ${levelData[levelId].length} bytes · 3 bitplanes` }]);
    setFlash(new Set());
    setLastVm(null);
    requestAnimationFrame(paint);
  }, [levelData, paint]);

  useEffect(() => { paint(); }, [paint]);
  useEffect(() => {
    const onKey = (event) => {
      const direction = DIRECTIONS[event.key];
      if (!direction) return;
      event.preventDefault();
      move(...direction);
    };
    window.addEventListener("keydown", onKey, { passive: false });
    return () => window.removeEventListener("keydown", onKey);
  }, [move]);

  const statusLabel = snapshot.state === "won" ? "COMPLETE" : snapshot.state === "lost" ? "FAILED" : "RUNNING";

  return (
    <main className="shell">
      <header className="masthead">
        <a className="brand" href="#game" aria-label="Bit Maze home">
          <span className="brand-mark" aria-hidden="true"><i /><i /><i /><i /><i /><i /></span>
          <span>bit-maze</span>
        </a>
        <div className="build-tag"><span className="pulse" /> WORLD_BUILD 0.3</div>
      </header>

      <section className="hero">
        <div className="intro">
          <p className="eyebrow">A WORLD MADE OF BITS</p>
          <h1>Find the path.<br /><em>Flip the world.</em></h1>
          <p className="lede">Every wall, item, hazard, trigger, and pixel below comes from compact binary data. Choose the 48-tile trial or the new 384-tile circuit.</p>
          <div className="legend" aria-label="Game legend">
            <span><b className="swatch player" /> YOU</span>
            <span><b className="swatch item" /> BIT</span>
            <span><b className="swatch plate" /> PLATE</span>
            <span><b className="swatch hazard" /> HAZARD</span>
          </div>
        </div>

        <div className={`terminal ${snapshot.state}`} id="game">
          <div className="terminal-bar">
            <span>BM://LEVELS/{LEVELS[activeLevel].file}</span>
            <span className="terminal-state">● {statusLabel}</span>
          </div>
          <div className="level-tabs" aria-label="Select level">
            {Object.entries(LEVELS).map(([id, level]) => (
              <button key={id} className={activeLevel === id ? "active" : ""} onClick={() => selectLevel(id)}>{level.label}</button>
            ))}
          </div>
          <div className="game-stage">
            <canvas ref={canvasRef} width="512" height="384" aria-label={`${LEVELS[activeLevel].label} Bit Maze game board`} />
            {snapshot.state !== "playing" && (
              <div className="outcome">
                <strong>{snapshot.state === "won" ? "YOU WIN" : "GAME OVER"}</strong>
                <span>{snapshot.state === "won" ? "all bits recovered" : "hazard collision"}</span>
                <button onClick={reset}>RUN AGAIN</button>
              </div>
            )}
          </div>
          <div className="readout">
            <div><small>BITS</small><strong>{String(snapshot.score).padStart(2, "0")} / {String(snapshot.total).padStart(2, "0")}</strong></div>
            <div><small>MOVES</small><strong>{String(snapshot.moves).padStart(3, "0")}</strong></div>
            <button className="reset" onClick={reset}>↻ RESET</button>
          </div>
          <p className="notice"><span>&gt;</span> {notice}</p>
        </div>
      </section>

      <section className="controls" aria-label="Game controls">
        <div className="control-copy">
          <p className="eyebrow">INPUT DEVICE</p>
          <h2>Move through the grid</h2>
          <p>Use W/A/S/D, arrow keys, or the pad. Walls block movement. The violet trigger is safe; the red spikes aren’t.</p>
        </div>
        <div className="keypad" aria-label="Touch direction pad">
          <button aria-label="Move up" onClick={() => move(0, -1)}>↑<span>W</span></button>
          <button aria-label="Move left" onClick={() => move(-1, 0)}>←<span>A</span></button>
          <button aria-label="Move down" onClick={() => move(0, 1)}>↓<span>S</span></button>
          <button aria-label="Move right" onClick={() => move(1, 0)}>→<span>D</span></button>
        </div>
      </section>

      <BitInspector game={engineRef.current} flash={flash} lastVm={lastVm} events={events} />

      <footer>
        <span>FORMAT: BM/V1</span><span>GRID: {snapshot.width}×{snapshot.height}</span><span>PLANES: 3</span><span>LOGIC: BITVM</span><span>RUNTIME: DETERMINISTIC</span>
      </footer>
    </main>
  );
}
