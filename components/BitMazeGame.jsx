"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { BitMazeGame as Engine, drawGame } from "../lib/bitmaze";

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

export default function BitMazeGame({ levelData }) {
  const canvasRef = useRef(null);
  const engineRef = useRef(null);
  if (!engineRef.current) engineRef.current = new Engine(Uint8Array.from(levelData.circuit));
  const [activeLevel, setActiveLevel] = useState("circuit");
  const [snapshot, setSnapshot] = useState({ score: 0, total: 12, state: "playing", moves: 0, width: 24, height: 16 });
  const [notice, setNotice] = useState(LEVELS.circuit.description);

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
    engineRef.current.move(dx, dy);
    sync();
  }, [sync]);

  const reset = useCallback(() => {
    engineRef.current = new Engine(Uint8Array.from(levelData[activeLevel]));
    sync();
  }, [activeLevel, levelData, sync]);

  const selectLevel = useCallback((levelId) => {
    const game = new Engine(Uint8Array.from(levelData[levelId]));
    engineRef.current = game;
    setActiveLevel(levelId);
    setSnapshot({ score: 0, total: game.totalItems, state: "playing", moves: 0, width: game.width, height: game.height });
    setNotice(LEVELS[levelId].description);
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
        <div className="build-tag"><span className="pulse" /> WORLD_BUILD 0.2</div>
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

      <footer>
        <span>FORMAT: BM/V1</span><span>GRID: {snapshot.width}×{snapshot.height}</span><span>PLANES: 3</span><span>LOGIC: BITVM</span><span>RUNTIME: DETERMINISTIC</span>
      </footer>
    </main>
  );
}
