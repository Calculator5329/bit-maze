"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { BitMazeGame as Engine, drawGame } from "../lib/bitmaze";

const DIRECTIONS = {
  ArrowUp: [0, -1], w: [0, -1], W: [0, -1],
  ArrowDown: [0, 1], s: [0, 1], S: [0, 1],
  ArrowLeft: [-1, 0], a: [-1, 0], A: [-1, 0],
  ArrowRight: [1, 0], d: [1, 0], D: [1, 0],
};

export default function BitMazeGame() {
  const canvasRef = useRef(null);
  const engineRef = useRef(new Engine());
  const [snapshot, setSnapshot] = useState({ score: 0, total: 3, state: "playing", moves: 0 });
  const [notice, setNotice] = useState("Collect all 3 bits. The violet plate opens the sealed gate.");

  const paint = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    drawGame(canvas.getContext("2d"), engineRef.current, 8);
  }, []);

  const sync = useCallback(() => {
    const game = engineRef.current;
    setSnapshot({ score: game.score, total: game.totalItems, state: game.state, moves: game.moves });
    if (game.state === "won") setNotice("MAZE RESOLVED // all bits recovered");
    else if (game.state === "lost") setNotice("SIGNAL LOST // the red spike is fatal");
    else setNotice(game.fired.size ? "Gate bit flipped: 1 → 0. The passage is open." : "Collect all 3 bits. The violet plate opens the sealed gate.");
    paint();
  }, [paint]);

  const move = useCallback((dx, dy) => {
    engineRef.current.move(dx, dy);
    sync();
  }, [sync]);

  const reset = useCallback(() => {
    engineRef.current = new Engine();
    setNotice("Collect all 3 bits. The violet plate opens the sealed gate.");
    sync();
  }, [sync]);

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
        <div className="build-tag"><span className="pulse" /> TRIAL_BUILD 0.1</div>
      </header>

      <section className="hero">
        <div className="intro">
          <p className="eyebrow">A WORLD MADE OF BITS</p>
          <h1>Find the path.<br /><em>Flip the world.</em></h1>
          <p className="lede">Every wall, item, hazard, trigger, and pixel below comes from compact binary data. Recover the cyan bits without touching the red signal.</p>
          <div className="legend" aria-label="Game legend">
            <span><b className="swatch player" /> YOU</span>
            <span><b className="swatch item" /> BIT</span>
            <span><b className="swatch plate" /> PLATE</span>
            <span><b className="swatch hazard" /> HAZARD</span>
          </div>
        </div>

        <div className={`terminal ${snapshot.state}`} id="game">
          <div className="terminal-bar">
            <span>BM://LEVELS/TRIAL.BM</span>
            <span className="terminal-state">● {statusLabel}</span>
          </div>
          <div className="game-stage">
            <canvas ref={canvasRef} width="512" height="384" aria-label="Bit Maze game board" />
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
        <span>FORMAT: BM/V1</span><span>PLANES: 3</span><span>LOGIC: BITVM</span><span>RUNTIME: DETERMINISTIC</span>
      </footer>
    </main>
  );
}
