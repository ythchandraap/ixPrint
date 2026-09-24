# ix.print

*itz.ixxyyzz / ix.print — a product by IX Systems.*

`itz.ixxyyzz` is the parent technology studio brand. `ix.print` is the
printing product, so product-facing actions and technical documentation use
`ix.print`, while the application shell carries the parent brand.

Bridges web apps to local printers: a web app POSTs a PDF to ix.print on the
configured server port (`18032` by default), and ix.print prints it silently.
The port can be changed in **Settings → Application bridge**. Runs on Windows
10+, macOS and Linux (x64 and ARM64).

## How printing works

1. **Settings** are resolved: request parameters → preset (requested, or the
   default preset) → built-in defaults.
2. **Layout** (PDFium): every page becomes exactly one sheet, with content
   already rotated, scaled and positioned. This is the same code on every OS,
   which is why output is identical everywhere.
3. **Print** 1:1: Windows renders sheets with PDFium and sends them via GDI;
   macOS/Linux hand the (vector) PDF to CUPS `lp`.

The printer is chosen as: `printer` in the request → printer in the preset →
ix.print's default printer → the OS default printer.

## Runtime controls

The system-tray menu provides quick access to:

- Application bridge on/off. Its state is persisted and defaults to on.
- Custom server port. The default is `18032`; valid custom ports are `1024`–`65535`.
- Printer list and history log in the main window.
- Run at login to install or remove auto-start for ix.print.

## Development

```bash
bun install
bun run fetch-pdfium        # downloads PDFium for all targets into src-tauri/pdfium/
bun run tauri dev
```

`build.rs` copies the PDFium library for the build target into
`src-tauri/pdfium/lib/`, which is bundled as a resource. To fetch only one
target: `bun run fetch-pdfium win-x64` (`win-x64`, `win-arm64`, `mac-x64`,
`mac-arm64`, `linux-x64`, `linux-arm64`). Linux needs CUPS installed.

### Development gates

Run these Rust checks from `src-tauri`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --test '*'
```

The library tests include layout checks that render real PDFs with PDFium, so
fetch the current platform's PDFium target before running them. Frontend type
checking and production build run at the repository root:

```bash
bun run build
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for backend boundaries and
module placement.

## HTTP API

| Method | Path | Description |
|---|---|---|
| `POST` | `/print?…` | Multipart upload (first file field). Returns `{message, job_id, printer, preset, sheets, spooler_job, status}` |
| `POST` | `/test-page?count=…&…` | Prints the edge-to-edge calibration page: `count` pages (1–50, default 1), other parameters as `/print` (no upload) |
| `GET` | `/job-status?job_id=…` | Where a job is now (see below) |
| `POST` | `/print` (JSON) | Body `{preset, data, settings?}`: fill a preset's design and print it (see below) |
| `GET` | `/templates` | Presets that have a design, with the `fields` each expects and sample data |
| `POST` | `/print-template` | Older form of the JSON print: `{template, data}` where `template` is the preset name |
| `GET` | `/queue?printer=…` | Printer status/problem and its spooler queue `{status, message, jobs: [{id, document, position, state, message, pages, pages_printed}]}` |
| `GET` | `/list-printer` | `[{name, status, message, shared, os_default, is_default, remote}]`; `remote` is present for printers forwarded from another ix.print |
| `GET` | `/printer-capabilities?printer=…` | Papers, trays, duplex, color |
| `GET` | `/presets` | Presets and defaults on this computer (read-only) |
| `GET` | `/presets/export` | Same, as an importable preset file |

Errors are `{message}` with status 400 (bad parameters, unknown preset or
printer), 401 (missing/invalid token) or 500 (document or printer failure).
Presets, tokens and the default printer can only be changed from the ix.print
window. Interactive docs with "try it" forms: `http://<ip>:<server-port>/docs`.

### Authentication

Tokens are created in the ix.print window → **API** tab, one per client app.
Only a SHA-256 hash is stored; the token is shown once. **Require a token** is on
by default: every endpoint except `/` and `/docs` returns 401 without a valid
token (with no token created yet, every API request is refused).
A wrong or revoked token is always rejected; 10 wrong tokens from one IP within
a minute lock that IP out (429) for 5 minutes. Send it as
`Authorization: Bearer fxp_…` (preferred), `X-Api-Token: fxp_…` or `?token=fxp_…`.

Browsers may call the API only from the websites listed under **Allowed websites**
(API tab, e.g. `https://app.example.com`), or from `/docs` itself;
requests from any other `Origin` are refused before they reach a handler.
New installs allow none; configs saved before this setting existed keep `*`
(any website) until it is changed. Requests without an `Origin` (servers,
curl) are unaffected.

```bash
curl -H "Authorization: Bearer fxp_…" -F file=@struk.pdf "http://192.168.1.10:18032/print?preset=struk"
```

### Printers of another ix.print

In **Settings → Other ix.print computers**, add another ix.print instance with
its LAN address and an API token created on that computer. Its printers appear
as `Remote name / Printer name`, for example `Warehouse / EPSON`. A printer
installed locally always wins when its name matches exactly, so OS-shared
printers remain on the normal local path. Jobs sent to a remote printer are
forwarded server-to-server, including designed preset jobs and test prints;
forwarding stops after one hop.

The connection uses ordinary HTTP on the LAN. Anyone able to intercept that
traffic could read the remote token, so use a trusted network or an `https://`
address behind a TLS reverse proxy.

### Presets with a design

A preset can carry an optional **design**: a label/receipt drawn in the preset
form (WYSIWYG — the preview is the real PDFium render): text, lines, boxes,
barcodes (Code 128), QR codes and images/logos. The editor accepts any image
format the system can open (PNG, JPEG, SVG, WebP, GIF, BMP, AVIF, HEIC on
macOS) and stores it as PNG/JPEG. Text, barcode, QR and image values can contain
`{{field}}` placeholders; an image field takes a data URL / base64 image
(PNG, JPEG, GIF, BMP, WebP, TIFF, ICO) — e.g. a photo per record. Clients then
send only the data to the same `/print` endpoint, as JSON:

```bash
curl -H "Authorization: Bearer fxp_…" -H "Content-Type: application/json" \
  -d '{"preset":"label-produk","data":[{"nama":"Product A","kode":"SKU-001"},{"nama":"Product B","kode":"SKU-002"}]}' \
  http://192.168.1.10:18032/print
```

`data` is an array of objects (a single object is also accepted). A design
has one of two layouts:

- **Label**: one page per object.
- **List**: the design has three bands — header, row, footer. Each object is
  a row under the header; when a page is full the rows continue on the next
  page with the header repeated. The footer prints once after the last row
  (totals, signatures) or at the bottom of every page. Optionally the page
  height follows the content (receipt rolls).

`header` (optional) holds values shared by every page — header/footer fields
of a list, or common text on labels:

```json
{ "preset": "laporan",
  "header": { "unit": "Toko Pusat", "total": "2 item" },
  "data": [ { "nama": "Product A", "kode": "SKU-001" }, { "nama": "Product B", "kode": "SKU-002" } ] }
```

`{{no}}`, `{{halaman}}`, `{{total_halaman}}` and `{{jumlah_data}}` are filled in
automatically; values sent in `data` / `header` override them. Missing
fields print empty. The page size is the design's; printer, offsets, copies
etc. come from the preset, overridable with `settings`
(`{"copies": 2, "printer": "…"}`). Test prints of a preset with a design use
its sample data. Exported presets include their designs.

Older configs' separate *templates* are moved into presets with a design on
startup (a name already used by a preset gets " (design)").

### Calibration page

The test page (button in the app, or `POST /test-page`) is generated at the
exact sheet size the job would use (preset/request paper and orientation; a
roll prints 80 mm or its width long), optionally several numbered pages to
check drift from one label to the next. It is printed 1:1 with the preset's
offsets applied. The black frame is the paper edge; mm rulers run inward from
the middle of each edge, so the first number visible on a side is how many mm
the printer clips there. Grey guides mark 5 and 10 mm, full-width strips show
horizontal coverage, and a light center cross shows centering.

### Job status

`/job-status` (and `status` in the `/print` response) returns:

```json
{ "job_id": "…", "printer": "EPSON", "state": "queued", "position": 3, "ahead": 2,
  "message": null, "printer_status": "Error", "printer_message": "Out of paper" }
```

`state` is one of `queued`, `printing`, `paused`, `error` (the job or the printer
reports a problem such as paper out, jam or offline; it may resume once fixed),
`completed`, `gone` (left the queue without a known result, e.g. cancelled),
`failed` (ix.print could not send it; `message` says why) or `unknown`.
Poll it every few seconds until it reaches `completed`, `gone` or `failed`.
Jobs are looked up in today's and yesterday's history.

### Result callback

Instead of polling, send `callback_url` (query of the upload `/print` and
`/test-page`, or a field of the JSON body) and optionally `callback_ref` (your
own id, echoed as `reference`). Requires an API token. When the job is done
ix.print POSTs:

```json
{ "event": "job.finished", "job_id": "…", "reference": "ORDER-42", "success": true,
  "state": "completed", "message": null, "printer": "EPSON", "preset": "struk",
  "sheets": 1, "time": "2026-09-22T10:15:03+07:00" }
```

`state`/`success`: `completed`/true, `sent`/true (spooler gives no job status),
`failed`/false, `gone`/false (left the queue unfinished, e.g. cancelled),
`timeout`/false (not done after 60 min). A printer problem before that sends
one `job.problem` (`state: "error"`, `success: null`). Requests rejected up
front (400/500) get no callback. Delivery needs a 2xx within 10 s, is retried
after 5 s, 30 s and 2 min, never follows redirects, and may repeat — dedupe on
`job_id` + `event`.

Each delivery is signed with the token used to print:
`X-IxPrint-Signature: sha256=<hex HMAC-SHA256("{X-IxPrint-Timestamp}.{raw body}")>`
with key = hex SHA-256 of the token. Examples for Node.js and PHP are on `/docs`.

The **History** tab shows each job's callback: waiting for the job, retrying
(with the error), delivered (with the result sent), failed after all attempts,
or refused. Progress is appended to the job's day file as
`{"callback_for": job_id, "callback": {…}}` lines, merged into the job on read.

### `/print` parameters

All optional; empty values count as not set.

| Parameter | Values | Default |
|---|---|---|
| `preset` | preset name | default preset |
| `printer` (or `printer_name`) | printer name | see above |
| `copies` | 1–999 | 1 |
| `collate` | `true` / `false` | `true` |
| `orientation` | `auto`, `portrait`, `landscape` | `auto` |
| `paper` | `default` (printer's), `A3` `A4` `A5` `A6` `B5` `Letter` `Legal` `F4`, roll `80mm`, custom `100x150mm` / `4x6in` | `default` |
| `scale` | `shrink` (only shrink), `fit`, `none`, percent `90%` | `shrink` |
| `align` | `center`, `top`, `top-left` | `center` |
| `margins` | mm: `5` / `5,3` / `5,3,5,3` (top,right,bottom,left) | `0` |
| `offset_x`, `offset_y` | mm, + = right / down | `0` |
| `pages` | `1-3,5,8-` | all |
| `duplex` | `off`, `long-edge`, `short-edge` | printer's |
| `color` | `color`, `grayscale` | printer's |
| `tray` | name from `/printer-capabilities` | printer's |
| `dpi` | 72–600 (Windows raster resolution) | printer's, max 300 |

Roll paper keeps its width and takes the length of the content; `landscape`
on a roll turns the content sideways.

```bash
curl -F file=@struk.pdf "http://192.168.1.10:18032/print?preset=struk&copies=2"
```

## Managing OS printers

The **Printer** tab can add, edit and remove printers in the operating system
(window only; there is no HTTP endpoint for it):

- **Add** from a detected device (USB / network discovery), an IP address
  (IPP, RAW 9100, LPD) or a manual device URI / Windows port, with a
  driverless (IPP Everywhere) or installed driver.
- **Edit** description, location, sharing; set as OS default. Renaming works on
  Windows only (CUPS cannot rename queues) and updates presets and the default
  printer that use the old name.
- **Remove**, with a warning when presets use the printer.

macOS/Linux use `lpinfo` / `lpadmin` / `lpoptions` (admin user; on Linux a
member of `lpadmin`). Windows uses the PrintManagement cmdlets and usually
needs ix.print to run as administrator. User values are passed as process
arguments / environment variables, never as shell or PowerShell code.

## Local data

Stored in the app data folder (`%APPDATA%\ixPrint`,
`~/Library/Application Support/ixPrint`, `~/.local/share/ixPrint`; the folder keeps the
original app identifier `ixPrint` so existing settings, tokens and history carry over):

- `config.json`: default printer, default preset, presets, templates, token hashes (previous version in `config.json.bak`)
- `history/YYYY-MM-DD.jsonl`: print history, one file per day, pruned after the retention period (default 90 days)
- `jobs/`: temporary files, deleted after each job and on startup
