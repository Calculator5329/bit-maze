import fs from "node:fs";
import path from "node:path";
import BitMazeGame from "../components/BitMazeGame";

export default function Home() {
  const load = (name) => [...fs.readFileSync(path.join(process.cwd(), "levels", name))];
  return <BitMazeGame levelData={{ trial: load("trial.bm"), circuit: load("circuit.bm") }} />;
}
