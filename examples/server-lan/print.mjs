import { readFile } from "node:fs/promises";

const base = (process.env.IXPRINT_URL || "").replace(/\/+$/, "");
const token = process.env.IXPRINT_TOKEN || "";
const key = process.env.IXPRINT_KEY || "";
const payloadFile = process.argv[2] || new URL("./payload.json", import.meta.url);

if (!base) throw new Error("Set IXPRINT_URL, for example http://192.168.1.20:18032");
if (!token) throw new Error("Set IXPRINT_TOKEN to a token created in ix.print → Settings → API");
if (!key || key.length > 128 || !/^[!-~]+$/.test(key)) {
  throw new Error("Set IXPRINT_KEY to a stable 1–128 character visible ASCII key for this logical job");
}

const payload = JSON.parse(await readFile(payloadFile, "utf8"));
const headers = { Authorization: `Bearer ${token}`, "Content-Type": "application/json" };

async function request(path, extraHeaders = {}) {
  const response = await fetch(`${base}${path}`, {
    method: "POST",
    headers: { ...headers, ...extraHeaders },
    body: JSON.stringify(payload),
  });
  const body = await response.json().catch(() => ({}));
  return { response, body };
}

const preflight = await request("/preflight");
if (!preflight.response.ok || preflight.body.valid !== true) {
  console.error("Preflight failed:", preflight.response.status, preflight.body);
  process.exit(1);
}

const result = await request("/print", { "Idempotency-Key": key });
console.log(JSON.stringify({ http_status: result.response.status, response: result.body }, null, 2));

if (result.response.status === 409 && result.body.code === "outcome_unknown") {
  console.error("Check History and the printer queue before deciding whether to create a new logical job.");
  process.exitCode = 2;
} else if (result.response.status === 409) {
  console.error("This key was used with different request content. Keep the original key only for the original payload.");
  process.exitCode = 1;
} else if (result.response.status === 429) {
  console.error("Authentication is rate limited. Check the token and wait before another attempt.");
  process.exitCode = 1;
} else if (!result.response.ok) {
  console.error("Print request failed; inspect the HTTP response before retrying.");
  process.exitCode = 1;
} else if (result.response.status === 202) {
  console.log("Job is pending. Keep IXPRINT_KEY unchanged and check the returned job_id.");
}
