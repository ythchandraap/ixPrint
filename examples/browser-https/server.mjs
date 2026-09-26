import { readFileSync } from "node:fs";
import { createServer } from "node:https";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const port = Number(process.env.PORT || 4173);
const key = readFileSync(join(here, "localhost-key.pem"));
const cert = readFileSync(join(here, "localhost.pem"));
const html = readFileSync(join(here, "index.html"));

createServer({ key, cert }, (request, response) => {
  if (request.url !== "/" && request.url !== "/index.html") {
    response.writeHead(404).end("Not found");
    return;
  }
  response.writeHead(200, {
    "Content-Type": "text/html; charset=utf-8",
    "Cache-Control": "no-store",
    "X-Content-Type-Options": "nosniff",
  });
  response.end(html);
}).listen(port, "127.0.0.1", () => {
  console.log(`Open https://localhost:${port}`);
  console.log("Configure that exact origin in ix.print → Settings → API → Allowed websites.");
});
