# ix.print HTTP integration

This guide describes the supported network paths for the print bridge. The API
submits work to an operating-system print service. A successful submission is
not proof of physical output.

## Supported topologies

1. **HTTPS web app → ix.print on the user's computer or LAN.** The browser calls
   the bridge directly. Configure the web app's exact origin in **Settings →
   API → Allowed websites** and allow Local Network Access in the browser when
   prompted. The browser token is visible to that web app, so use a dedicated,
   revocable API token and protect the app origin against script injection.
2. **Application server on the same LAN → ix.print on the LAN.** The server
   calls the bridge directly with a bearer token. CORS and website origin
   allowlisting apply to browser requests; server-to-server requests without an
   `Origin` header are not subject to that allowlist.
3. **Cloud application server → customer's private network.** This is not
   automatic. A cloud host cannot route to a customer's private IP unless the
   customer provides a separate network path such as a VPN or managed relay.
   ix.print does not create that path.

The bridge must be enabled. For LAN access, enable LAN access in ix.print and
use the computer's current LAN address and configured port (default `18032`).
Allow that port through the host firewall only on the trusted private network.
If the bridge is loopback-only, a remote browser or LAN server cannot reach it.
The standard bridge uses HTTP; use a trusted LAN or a TLS reverse proxy where
the network is not trusted. Use `Authorization: Bearer <token>` for new
integrations. The `?token=` query form remains accepted for compatibility, but
avoid it: URLs can be exposed through browser history, copied links, or
intermediary logs. Do not include real API tokens in examples or source control.

## Browser origin, token, and Local Network Access

The allowlist entry is an **origin**: scheme, host, and port, with no path. For
the local example below, enter exactly `https://localhost:4173`. A deployed app
must use its own HTTPS origin, for example `https://orders.example.com`.
Requests from a different origin are refused before reaching the handler.

Current browser documentation describes Local Network Access (LNA) as a
permission separate from CORS. Chrome introduced its permission prompt in
Chrome 142; Microsoft Edge began enabling LNA restrictions by default in Edge
143. Both require a secure context for the permission flow. The prompt and
mixed-content behavior depend on browser version, destination address, and
network topology. Users should grant access to the intended application origin
through the browser's normal permission UI. Do not instruct them to disable
browser protections. See [Chrome's LNA guide](https://developer.chrome.com/blog/local-network-access)
and [Microsoft Edge's LNA guide](https://learn.microsoft.com/en-us/deployedge/ms-edge-local-network-access).

The version notes above describe rollout milestones, not a supported-browser
claim. Record the actual stable Chrome and Edge versions and test against a
deployed HTTPS origin before claiming compatibility; see the
[verification matrix](../release-readiness/verification-matrix.md).

## Submit, retry, and interpret status

Call `POST /preflight` with the same designed-preset JSON that will be printed.
Only call `POST /print` if the HTTP request succeeds and the response says
`valid: true`. For each deliberate logical print, use one unique
`Idempotency-Key` (1–128 visible ASCII characters). On timeout or a lost
response, retry the same payload and settings with the **same key**. A changed
payload or settings with that key returns HTTP 409 fingerprint conflict. Keys
are retained for 24 hours and scoped to the API token (or client IP when tokens
are optional). Make a new key only when the user deliberately requests a new
print.

| Response | Meaning and client action |
|---|---|
| `202` | The matching logical job is still active. Keep the key and inspect `job_id`/`/job-status`; do not submit a second job. |
| `409` with the fingerprint-conflict message | The key was already used for different content/settings. Fix the client key/payload pairing; do not retry altered content with the old key. |
| `409`, `code: "outcome_unknown"` | ix.print cannot establish the final result. Check History and the printer queue before deciding whether to create a new job. ix.print will not automatically resubmit it. |
| `429` | Repeated invalid-token attempts temporarily locked the client IP (currently five minutes after ten failures in one minute). Correct the token and wait before retrying; do not loop requests. |

An initial success means ix.print accepted the job from the API request and
submitted it to the operating-system print service. Queue state `completed`
means the spooler reports completion; it still does not prove paper was
physically produced. A queue job can disappear with unknown outcome, and
printer, paper, media, or driver failures can occur after submission.

Callbacks may be delivered more than once. Deduplicate callback handling on
`job_id` + `event`, and return a 2xx response for a delivery already applied.
If a request times out, preserve its idempotency key while resolving the
outcome; never silently turn an uncertain retry into a new print.

## Runnable examples

### Browser from an HTTPS origin

Requires Node.js and [`mkcert`](https://github.com/FiloSottile/mkcert) for a
locally trusted development certificate. From the repository root:

```sh
cd examples/browser-https
mkcert -install
mkcert -cert-file localhost.pem -key-file localhost-key.pem localhost 127.0.0.1 ::1
node server.mjs
```

Open `https://localhost:4173`, add that exact origin to ix.print's website
allowlist, enter an API token and a published designed-preset payload, then
run preflight and print. The token stays in the page's memory. The logical job
key stays in the current tab's session storage; retain it when repeating the
same request. The **Start a new logical job** button creates a different print
identity, so do not use it to recover an unknown result before checking History
and the queue. Localhost development is not a substitute for testing from the
real deployed HTTPS origin.

### Application server on the LAN

Requires Node.js 18 or newer. Edit `examples/server-lan/payload.json` to match
a published designed preset on the target ix.print. In a terminal on the LAN
server:

```sh
export IXPRINT_URL='http://192.168.1.20:18032'
export IXPRINT_TOKEN='your-token-from-ix.print-settings'
export IXPRINT_KEY='order-1234-label-1'
node examples/server-lan/print.mjs examples/server-lan/payload.json
```

Use the exact same `IXPRINT_KEY` and input file to retry that logical job. Set
a new key only for a deliberate new print. The example runs preflight first,
prints HTTP errors, and exits with a distinct status for an unknown outcome.
It does not retry automatically. Restrict token access to the application
server and use TLS or a trusted LAN.

## Troubleshooting checklist

- **Bridge off:** enable Application bridge in ix.print and verify the configured port.
- **Wrong address or port:** test `GET /` from the same network and confirm the ix.print host address did not change.
- **Firewall:** allow the configured port for the trusted private network; do not expose it to the public internet.
- **Browser CORS:** add the exact HTTPS origin under Allowed websites, including non-default port if present.
- **Browser LNA:** use the browser's site permission UI for the intended origin; a CORS allowlist does not grant this permission.
- **Printer offline, out of paper, or driver error:** inspect `/queue`, the OS spooler, printer display, and ix.print History. Resolve the device condition before creating a new logical job.
- **HTTP 401/429:** verify the dedicated token. After the documented lockout, wait before the next attempt; do not keep retrying a bad credential.
- **HTTP 409 unknown:** inspect History and the OS queue. Keep the old key attached to the old logical job while investigating.
