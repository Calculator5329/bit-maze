import { copyFile, mkdir, writeFile } from "node:fs/promises";

await mkdir("dist/server", { recursive: true });
await mkdir("dist/.openai", { recursive: true });
await copyFile(".openai/hosting.json", "dist/.openai/hosting.json");

const server = `import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const types = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png", ".ico": "image/x-icon" };

createServer(async (request, response) => {
  try {
    const url = new URL(request.url ?? "/", "http://localhost");
    let pathname = decodeURIComponent(url.pathname);
    if (pathname.endsWith("/")) pathname += "index.html";
    let file = path.resolve(root, "." + pathname);
    if (!file.startsWith(root + path.sep)) throw new Error("bad path");
    try { if ((await stat(file)).isDirectory()) file = path.join(file, "index.html"); }
    catch { file = path.join(root, "404.html"); response.statusCode = 404; }
    const body = await readFile(file);
    response.setHeader("content-type", types[path.extname(file)] ?? "application/octet-stream");
    response.setHeader("x-content-type-options", "nosniff");
    response.setHeader("referrer-policy", "same-origin");
    response.end(request.method === "HEAD" ? undefined : body);
  } catch {
    response.statusCode = 404;
    response.end("Not found");
  }
}).listen(Number(process.env.PORT ?? 3000), "0.0.0.0");
`;

await writeFile("dist/server/index.js", server);
