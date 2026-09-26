# ix.print release verification matrix

Status is **Belum diuji** until the exact installer is installed and the
scenario is run on the named OS, browser, and physical printer. Unit tests,
simulated adapters, and successful builds do not count as device verification.
Attach evidence such as installer checksum, OS build, printer model/driver,
media, ix.print version, request/job ID, timestamp, and sanitized logs.

## Installer and first-run checks

| Platform and architecture | Installer / signature | Clean install → configure → first print | Upgrade | Uninstall / data preservation | Status / evidence |
|---|---|---|---|---|---|
| Windows x64 | NSIS + MSI; Authenticode required | Record result | Record result | Record result | **Belum diuji** — signed artifact and Windows device unavailable |
| macOS Intel | DMG; Developer ID signing and notarization required | Record result | Record result | Record result | **Belum diuji** — no signing credentials supplied to this task and no Intel Mac |
| macOS Apple Silicon | DMG; Developer ID signing and notarization required | Record result | Record result | Record result | **Belum diuji** — no signing credentials supplied to this task and no Apple Silicon Mac |
| Linux x64 | AppImage + DEB; verify resources and install behavior | Record result | Record result | Record result | **Belum diuji** — final Linux installers and clean target unavailable |

For each row, verify the app launches without bypass steps, bridge defaults and
permissions are understandable, a dedicated API token can be created, the
printer can be selected, and the first print completes using only published
instructions. Upgrade and uninstall checks must confirm whether config,
presets, API tokens, and retained History remain available. Do not claim data
preservation until those checks run against the actual installer and OS.

## Physical printer and job matrix

Run each platform against at least one A4/Letter document printer and one
thermal/label printer. Record printer model, driver and version, media size,
connection type, OS version, ix.print artifact checksum, and History/job IDs.
For every printer, execute PDF single-page, PDF multi-page, designed-preset
JSON, test page, and remote print. Exercise printer offline, paper out, driver
error, network interruption, and restart while a job is active. A setup not
applicable to a device must be justified in its evidence rather than silently
marked passed.

| OS | Printer class | PDF + multi-page | Designed JSON | Test page | Remote print | Offline / paper / driver / network / restart | Status / evidence |
|---|---|---|---|---|---|---|---|
| Windows x64 | A4/Letter | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Windows device or printer attached |
| Windows x64 | Thermal/label | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Windows device or printer attached |
| macOS Intel | A4/Letter | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Intel Mac or printer attached |
| macOS Intel | Thermal/label | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Intel Mac or printer attached |
| macOS Apple Silicon | A4/Letter | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Apple Silicon Mac or printer attached |
| macOS Apple Silicon | Thermal/label | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Apple Silicon Mac or printer attached |
| Linux x64 | A4/Letter | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Linux device or printer attached |
| Linux x64 | Thermal/label | Belum diuji | Belum diuji | Belum diuji | Belum diuji | Belum diuji | **Belum diuji** — no Linux device or printer attached |

## HTTPS browser matrix

Use the actual deployed HTTPS web origin, not only `localhost`. Record the
browser channel and exact version, OS, origin, bridge address/port, permission
choice, request/job ID, and sanitized console/network result. Exercise allowed
origin, wrong origin, LNA allow and deny, bridge disabled, wrong port/firewall,
printer offline, 202 polling, fingerprint-conflict 409, outcome-unknown 409,
and 429 lockout handling.

| Browser | Stable version used by target customer | HTTPS origin | Scenarios | Result / evidence |
|---|---|---|---|---|
| Chrome | Not recorded | Not selected | Belum diuji | **Belum diuji** — no target deployment/browser session available |
| Microsoft Edge | Not recorded | Not selected | Belum diuji | **Belum diuji** — no target deployment/browser session available |

## Resource and stability measurements

Use representative production document sizes and burst patterns; record the
input PDF size, page count, job rate, and concurrency. Do not fill in estimates.

| Metric | Workload / measurement method | Actual result | Status / evidence |
|---|---|---|---|
| Request-to-spooler latency | Measure request start to accepted/spooler job ID | Not measured | **Belum diuji** |
| p95 latency | Representative burst workload, report sample count | Not measured | **Belum diuji** |
| Memory, large job | Peak process memory for a documented large PDF | Not measured | **Belum diuji** |
| Memory, burst jobs | Peak process memory and queue behavior at documented rate | Not measured | **Belum diuji** |
| History disk growth | Retention enabled; measure metadata and content over time | Not measured | **Belum diuji** |
| Error rate | Classify validation, network, printer, and unknown outcomes | Not measured | **Belum diuji** |
| 72-hour soak | Keep application running; exercise cleanup, restart/recovery, and recurring jobs | Not run | **Belum diuji** |

## Current evidence boundary

Automated Rust/frontend tests and static build checks are recorded in the
follow-up report. They can verify code paths and packaging gates but do not
change any device, browser, signing, install, or performance result above to
“Lulus”.
