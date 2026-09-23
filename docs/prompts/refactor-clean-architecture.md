# PROMPT: Perbaikan menyeluruh + Clean Architecture ix.print (Tauri 2 / Rust / Vue 3)

> Salin seluruh isi file ini sebagai prompt ke AI coding agent. Jalankan **satu FASE per sesi/turn**
> dan jangan lanjut ke fase berikutnya sebelum GATE fase itu hijau dan saya bilang "lanjut".

---

## 0. PERAN DAN ATURAN MAIN

Kamu adalah senior engineer Rust + Vue yang mengerjakan refactor pada repo `fxPrint` (produk **ix.print**).
Tugasmu: memperbaiki daftar masalah di Bagian 2 **dan** mengubah backend Tauri menjadi Clean Architecture
**tanpa mengurangi/mengubah satu pun fitur, endpoint, perintah Tauri, format file, atau perilaku yang ada**.

### Aturan keras (melanggar = pekerjaan ditolak)

1. **Jangan menebak.** Semua keputusan desain sudah ditulis di dokumen ini. Jika ada hal yang tidak dijelaskan
   dan kamu ragu, **BERHENTI dan tanya saya** dengan satu pertanyaan spesifik. Jangan menciptakan solusi sendiri,
   jangan menambah fitur, jangan mengganti library, jangan mengubah nama publik.
2. **Refactor = memindahkan kode, bukan menulis ulang.** Pindahkan fungsi/struct apa adanya (isi tubuh fungsi
   tidak berubah). Perubahan perilaku hanya di Fase 3 (perbaikan keamanan) dan hanya sesuai spesifikasinya.
3. **Satu langkah = satu commit kecil** yang lolos GATE. Jangan menggabungkan langkah. Jangan `git commit --amend`
   commit lama, jangan force-push, jangan `git reset --hard`, jangan hapus file yang tidak disebut di dokumen ini.
4. **Kontrak eksternal dibekukan** (lihat Bagian 1). Test kontrak harus hijau di setiap GATE.
5. **Jangan menyentuh** `bun.lock`/`Cargo.lock` kecuali langkah itu memang menambah/menghapus dependency.
   Jangan upgrade versi dependency apa pun. `tauri` tetap `=2.11.5`.
6. **Dilarang** membuat/menyimpan/mencetak secret, kunci penandatanganan, sertifikat, atau token. Hal yang butuh
   secret dikerjakan saya sendiri (lihat Bagian 9).
7. Setiap akhir langkah, jalankan perintah GATE di Bagian 3 dan **tempel output ringkasnya** (jumlah test lulus,
   exit code). Jika merah: perbaiki di langkah yang sama; jika tidak bisa dalam 3 percobaan, berhenti dan laporkan.
8. Gaya kode: ikuti kode sekitar. Komentar hanya jika alasan tidak jelas dari kode. Bahasa pesan error tetap
   **Inggris** (lihat Langkah 2.5). Jangan mengubah teks pesan error yang sudah ada kecuali yang disebut di 2.5.
9. Bahasa komunikasi denganku: Indonesia. Nama kode/commit: Inggris.

---

## 1. KONTRAK YANG DIBEKUKAN (TIDAK BOLEH BERUBAH)

**1.1 HTTP API** (semua path, method, query, body, status code, bentuk JSON, header):
`GET /`, `GET /docs`, `POST /print` (multipart & JSON), `POST /test-page`, `GET /job-status`, `GET /templates`,
`POST /print-template`, `GET /queue`, `GET /list-printer`, `GET /printer-capabilities`, `GET /presets`,
`GET /presets/export`. Header keamanan (`X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`,
`Cache-Control`, `Content-Security-Policy`) dan perilaku CORS (`cors()` di `server.rs`) tidak berubah.
Auth: `Authorization: Bearer`, `X-Api-Token`, query `token`; lockout 10 gagal/60 dtk → 429 selama 5 menit.

**1.2 Perintah Tauri (51 command)** — nama & signature argumen/return tidak berubah:
`this_local_ip open_docs data_folder_path open_data_folder os_detected engine_status server_status bridge_enabled
set_bridge_enabled set_server_port remote_list remote_add remote_update remote_remove remote_test get_printers
get_capabilities printer_devices printer_drivers printer_details printer_add printer_update printer_remove
printer_set_os_default get_queue job_status app_print preview_pdf_sheet get_config set_default_printer
set_default_preset save_preset delete_preset preview_template preview_print set_history_retention create_token
revoke_token set_require_token set_allowed_origins token_usage export_presets preview_import apply_import
history_days history_page error_log record_error clear_error_log open_system_logs test_print`
Event `tray-navigate` dan id menu tray (`bridge-toggle`, `autostart-toggle`, `view-printers`, `view-history`,
`view-errors`, `open`, `quit`) tidak berubah.

**1.3 Format data di disk** — `config.json` (`CONFIG_VERSION = 2`, `config.json.bak`, atomic write temp+rename,
permission 0600 di Unix), file remotes, file history, file diagnostics: nama file, nama field serde, dan
default tidak berubah. Field baru **wajib** `#[serde(default)]` supaya config lama tetap terbaca.
Format `BUNDLE_FORMAT = "ixprint-presets"` dan tanda tangan callback HMAC (`X-...` header yang sudah ada) tidak berubah.

**1.4 Frontend ↔ backend** — nama command yang dipanggil `invoke()` di `src/api.ts` tidak berubah.

---

## 2. DAFTAR PERBAIKAN (SEMUA WAJIB DIKERJAKAN)

| # | Perbaikan | Fase/Langkah |
|---|---|---|
| A | Tidak ada CI test → buat `ci.yml` (test, clippy, fmt, vue-tsc, vitest) | 1.4 |
| B | Tidak ada test frontend → Vitest untuk logika murni | 6.7 |
| C | Server bind `0.0.0.0` tanpa opsi → opsi "LAN access" (default tetap ON) | 3.2 |
| D | Callback rentan DNS rebinding → resolve + validasi IP tiap pengiriman | 3.1 |
| E | Belum ada signing/updater/ARM64 → ARM64 di workflow + env signing pass-through + `docs/RELEASE.md` (updater = keputusan saya, JANGAN diimplementasi) | 7 |
| F | Dependency tak terpakai di `package.json` | 1.2 |
| G | File terlalu besar → dipecah oleh Fase 4–6 (batas ≤ 400 baris per file, kecuali tests & `docs.html`) | 4–6 |
| H | Polling printer 3 dtk terus-menerus → hanya saat window terlihat | 6.5 |
| I | `Mutex::lock().unwrap()` → helper poison-tolerant | 1.3 |
| J | Belum ada batas beban per klien → limit request in-flight per token/IP | 3.3 |
| K | Format campur tab/spasi → `cargo fmt` + `.editorconfig` | 1.1 |
| L | Bahasa campur di pesan error | 2.5 |
| M | Metadata `Cargo.toml` default | 1.5 |
| N | Working tree kotor → aturan Langkah 0.1 | 0.1 |
| O | Clean Architecture backend + test penegak dependency | 4–5 |
| P | Struktur frontend berbasis fitur | 6 |

---

## 3. GATE (dijalankan di akhir SETIAP langkah)

Backend (`cd src-tauri`):
```bash
cargo fmt --all -- --check          # setelah Langkah 1.1
cargo clippy --all-targets -- -D warnings   # setelah Langkah 1.1 (sebelum itu: cargo check --all-targets)
cargo test --lib                    # jumlah test tidak boleh TURUN dibanding baseline (Langkah 0.3)
cargo test --test '*'               # setelah Langkah 4.1 (test arsitektur & kontrak)
```
Frontend (root repo):
```bash
bun run build                       # = vue-tsc --noEmit && vite build
bun run test                        # setelah Langkah 6.7
```
Aturan: **warning baru = gagal**. `#[allow(...)]` baru dilarang kecuali `dead_code` untuk fungsi
yang memang platform-specific (`#[cfg_attr(not(windows), allow(dead_code))]` seperti yang sudah ada).

---

## FASE 0 — Persiapan & baseline (tanpa mengubah kode)

**0.1 Working tree.** Jalankan `git status --porcelain`. Saat ini ada perubahan belum ter-commit di 19 file
(`src-tauri/...`, `src/...`, `src/components/PrintTab.vue` untracked) pada branch `codex/installer-all-platforms`.
- Jika tidak kosong: **BERHENTI** dan minta saya meng-commit dulu. Jangan commit/stash/checkout atas namaku.
- Jika kosong: `git switch -c refactor/clean-architecture`.

**0.2 Jangan baca ulang seluruh repo.** Pemetaan modul sudah ada di Bagian 5. Baca hanya file yang
sedang dipindahkan pada langkah berjalan.

**0.3 Baseline.** Jalankan dan simpan hasil ke `docs/refactor/baseline.md` (buat folder; commit):
1. `cd src-tauri && cargo test --lib 2>&1 | tail -15` → catat **jumlah test lulus** (angka ini = lantai).
2. `bun run build` → catat sukses/gagal. Jika gagal sejak awal, laporkan dan berhenti.
3. Daftar route: dari fungsi `routes()` di `server.rs` (baris ±670) salin semua pasangan method+path ke file itu.
4. Daftar 51 command dari `tauri::generate_handler![...]` di `lib.rs` — harus sama dengan daftar di 1.2.
5. `git rev-parse HEAD`.

**0.4 Test kontrak (dibuat SEKARANG, sebelum refactor, agar melindungi seluruh langkah berikutnya).**
Modul crate masih privat sekarang, jadi jangan buat folder `tests/` dulu. Tambahkan **satu test di `server.rs`** (dalam `mod tests`
yang sudah ada, pola `actix_web::test` yang sudah dipakai) bernama `contract_all_routes_registered` yang, untuk
setiap pasangan method+path di baseline.md, memanggil app dan memastikan status **bukan 404** (401/400 diterima).
Commit: `test: freeze HTTP route contract`.

---

## FASE 1 — Tooling, kebersihan, dan perbaikan mekanis (perilaku tidak berubah)

**1.1 Format & lint (K).**
1. Buat `src-tauri/rustfmt.toml` berisi persis: `edition = "2021"` dan `hard_tabs = false`, `tab_spaces = 4`, `max_width = 120`.
2. Buat `.editorconfig` di root: `root = true`; `[*]` `charset = utf-8`, `end_of_line = lf`, `insert_final_newline = true`,
   `trim_trailing_whitespace = true`; `[*.rs]` `indent_style = space`, `indent_size = 4`; `[*.{vue,ts,json,yml,yaml,md}]`
   `indent_style = space`, `indent_size = 2`.
3. Jalankan `cargo fmt --all` **sekali**. Commit **hanya format** (`style: apply rustfmt`), tanpa perubahan lain.
4. Jalankan `cargo clippy --all-targets -- -D warnings`. Perbaiki setiap warning dengan perubahan minimal
   (commit `chore: fix clippy warnings`). Jika suatu warning butuh perubahan perilaku, berhenti dan tanya.

**1.2 Hapus dependency frontend tak terpakai (F).** Di `package.json`:
- Hapus dari `dependencies`: `iconsax-reactjs`, `react-icons`, `vue-iconsax`. Hapus dari `devDependencies`: `@iconify/vue`.
- Pindahkan `prettier` dari `dependencies` ke `devDependencies`.
- Tetap: `lucide-vue-next` (satu-satunya library ikon yang dipakai; 14 file meng-import-nya).
- Jalankan `bun install` (memperbarui `bun.lock`), lalu `grep -rn "iconsax\|react-icons\|@iconify" src` harus **kosong**.
- Tambahkan `.prettierrc`: `{ "printWidth": 120, "semi": true, "singleQuote": false, "tabWidth": 2 }`. **Jangan** menjalankan prettier ke seluruh repo pada langkah ini.
- Gate: `bun run build`. Commit `chore(deps): drop unused icon libraries`.

**1.3 Mutex tahan-poison (I).**
1. Buat `src-tauri/src/sync.rs` dengan isi persis:
```rust
use std::sync::{Mutex, MutexGuard};

/// Locks `m`, recovering the data if a previous holder panicked.
/// Our shared state is always left consistent between statements, so a
/// poisoned lock must not take every later request down with it.
pub trait LockExt<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}
```
2. Daftarkan `mod sync;` di `lib.rs`. (Di Fase 4 file ini pindah ke `shared/sync.rs`.)
3. Ganti **semua** `.lock().unwrap()` di: `auth.rs` (5), `commands.rs` (8), `history.rs` (3), `server.rs` (4), `remote.rs` (4)
   menjadi `.lock_safe()` (tambah `use crate::sync::LockExt;`). Ganti juga dua pola manual di `layout.rs:65` dan
   `diagnostics.rs:127,144` (`.lock().unwrap_or_else(|e| e.into_inner())`) ke `.lock_safe()`.
   Untuk `RwLock` (jika ada; cek `grep -n "RwLock\|\.read()\|\.write()" src-tauri/src -r`) — jika ada `.unwrap()`
   pada read/write, buat `RwLockExt` yang analog dengan nama `read_safe/write_safe`.
4. Tambah test di `sync.rs`: thread yang panic saat memegang lock, lalu `lock_safe()` dari thread lain tetap mengembalikan data.
5. Gate lalu commit `fix: tolerate poisoned mutexes`.

**1.4 CI (A).** Buat `.github/workflows/ci.yml`:
- `on`: `pull_request` dan `push` ke `main`; `permissions: contents: read`; `concurrency` group per ref, cancel-in-progress.
- Job `rust` (matrix `ubuntu-24.04`, `macos-15`, `windows-latest`), langkah berurutan: checkout@v4; (Linux saja) install
  paket yang **sama persis** dengan daftar di `build-installers.yml` langkah "Install Linux dependencies"; `oven-sh/setup-bun@v2`;
  `dtolnay/rust-toolchain@stable` dengan `components: clippy, rustfmt`; `Swatinem/rust-cache@v2` dengan `workspaces: src-tauri`;
  `bun install --frozen-lockfile`; `bun run fetch-pdfium <target>` dengan target `linux-x64` / `mac-arm64` / `win-x64` sesuai matrix
  (test layout butuh PDFium asli); lalu `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` dan `cargo test --test '*'` (dua terakhir dengan `working-directory: src-tauri`).
- Job `frontend` (ubuntu): checkout, setup-bun, `bun install --frozen-lockfile`, `bun run build`, `bun run test` (job ini
  akan merah sampai Langkah 6.7; **tambahkan langkah `bun run test` baru pada 6.7**, bukan sekarang).
- Jangan mengubah `build-installers.yml` di langkah ini.
Commit `ci: run tests, clippy and fmt on every PR`.

**1.5 Metadata (M).** `src-tauri/Cargo.toml`: `description = "ix.print — local print bridge for web apps"`,
`authors = ["IX Systems"]`. Tidak ada perubahan lain. (Nama crate/lib `ixprint`/`ix_print_lib` tetap.)

---

## FASE 2 — Rapikan tanpa mengubah struktur

**2.1** Buat `docs/refactor/` sudah ada dari 0.3. Tidak ada aksi lain.

**2.5 Bahasa pesan (L).** Ubah **hanya** string berikut ke Inggris (cari dengan `grep -rn "maksimal\|tidak\|harus\|gagal" src-tauri/src --include=*.rs`
— tindak lanjuti hanya hasil yang berupa pesan error/validasi ke pengguna, bukan komentar/nama field):
- `callback.rs`: `"callback_url maksimal {} karakter"` → `"callback_url must be at most {} characters"`.
- Jika grep menemukan pesan Indonesia lain pada pesan error, terjemahkan dengan pola kalimat Inggris yang sama dengan pesan sekitarnya.
- **Jangan** ubah `AUTO_FIELDS` (`no, halaman, total_halaman, jumlah_data`) — itu nama field kontrak, bukan pesan.
- Perbarui test yang meng-assert pesan lama (jika ada). Gate, commit `chore: use English for validation messages`.

---

## FASE 3 — Perbaikan keamanan & ketahanan (SATU-SATUNYA fase yang boleh mengubah perilaku)

Kerjakan **setelah** Fase 1–2 tetapi **sebelum** refactor arsitektur (Fase 4), agar perilaku baru punya test sebelum kode dipindah.

### 3.1 Callback anti-DNS-rebinding (D)
File: `src-tauri/src/callback.rs` (`validate_url` ±baris 64, `deliver` ±baris 240).

Kebijakan (tetap): **IP LAN privat BOLEH** (10/8, 172.16/12, 192.168/16, fc00::/7) karena SIMRS/rumah sakit ada di LAN.
Yang **DITOLAK** setelah resolusi DNS: loopback (127/8, ::1), link-local (169.254/16, fe80::/10), unspecified (0.0.0.0, ::),
broadcast (255.255.255.255), multicast, dan alamat IPv4-mapped-IPv6 (`::ffff:a.b.c.d`) yang IPv4 di dalamnya masuk daftar tolak tsb.

Langkah:
1. Buat fungsi murni `fn is_blocked_ip(ip: IpAddr) -> bool` (memakai aturan di atas; untuk IPv6 mapped, panggil ulang dengan IPv4-nya
   via `Ipv6Addr::to_ipv4_mapped`). Ganti blok `match ip {...}` di `validate_url` agar memanggil fungsi ini (perilaku untuk IP literal
   sekarang **lebih ketat** — ditambah unspecified/broadcast/multicast/mapped; itu disengaja).
2. Hapus komentar `// DNS-rebinding protection is deferred...` dan implementasikan: fungsi baru
   `fn resolve_checked(host: &str, port: u16) -> Result<Vec<SocketAddr>, String>` memakai `std::net::ToSocketAddrs`.
   Jika hasil kosong → error `"callback_url host could not be resolved"`. Jika **ada satu pun** alamat hasil resolusi yang `is_blocked_ip`
   → error `"callback_url resolves to a loopback or link-local address"`.
3. Panggil `resolve_checked` **di dalam `deliver()` pada setiap percobaan pengiriman** (bukan hanya di `validate_url`), tepat sebelum
   koneksi. Port default: 80 untuk http, 443 untuk https bila URL tak menyebut port.
4. Pin koneksi ke alamat yang sudah divalidasi. Urutan yang HARUS dicoba:
   - **Opsi A (utama):** ureq 3 menyediakan resolver kustom di `ureq::unversioned::resolver` (trait `Resolver`) dan
     `Agent::with_parts(config, connector, resolver)`. Implementasikan `Resolver` yang mengembalikan **hanya** alamat hasil `resolve_checked`
     (bukan me-resolve ulang). Coba compile.
   - **Opsi B (jika Opsi A tidak compile dalam 2 percobaan):** biarkan ureq me-resolve sendiri, tetapi panggil `resolve_checked`
     tepat sebelum `agent.post(...)` pada tiap percobaan. Tulis komentar 1 baris: `// Residual TOCTOU window between check and connect; mitigated by per-attempt re-check.`
   - Laporkan opsi mana yang dipakai. Jangan mencoba opsi ketiga (mis. mengganti host dengan IP literal — merusak SNI/HTTPS).
5. Test baru (di `callback.rs` `mod tests`, pola yang sudah ada dengan `TcpListener`):
   - `is_blocked_ip` untuk: `127.0.0.1`, `::1`, `169.254.169.254`, `fe80::1`, `0.0.0.0`, `::`, `::ffff:127.0.0.1`, `224.0.0.1` → true;
     `10.1.2.3`, `192.168.1.5`, `172.16.0.9`, `8.8.8.8`, `fd00::1` → false.
   - `resolve_checked("localhost", 80)` → Err. `resolve_checked("127.0.0.1", 80)` → Err.
6. Test yang sudah ada `delivers_signed_result_of_failed_job` menembak `127.0.0.1` — **jangan diubah perilakunya**; jika sekarang gagal karena
   aturan baru, beri test itu jalur khusus test: `#[cfg(test)] static ALLOW_LOOPBACK_FOR_TESTS: AtomicBool` yang membuat `is_blocked_ip`
   mengembalikan false untuk loopback **hanya saat `cfg(test)`**. Jangan ada jalur bypass di build non-test.
Commit `fix(callback): validate resolved addresses on every delivery`.

### 3.2 Opsi "LAN access" (C)
1. `config.rs` struct `Config` (±baris 118): tambah field `#[serde(default = "default_true")] pub lan_access: bool` (gunakan helper
   `default_true` bila sudah ada di file; kalau belum, buat `fn default_true() -> bool { true }`). Default `Config::default()` → `true`
   (perilaku sekarang: bind `0.0.0.0`). `CONFIG_VERSION` **tidak** dinaikkan. Tambah ke test default yang ada
   (`assert!(fresh.require_token ...)`) satu assert `fresh.lan_access`.
2. `server.rs` `run()`: ganti `.bind(("0.0.0.0", port))` dengan alamat `if lan_access { "0.0.0.0" } else { "127.0.0.1" }`
   (baca `state.config.get().lan_access` bersama `port`).
3. Command Tauri baru **`set_lan_access(state, enabled: bool) -> Result<Config, String>`** di `commands.rs`, meniru persis pola
   `set_server_port` (±baris 128): simpan config lalu restart server lewat mekanisme yang sama yang dipakai `set_server_port`
   (hentikan `server_handle`, `server::spawn`). Daftarkan di `generate_handler!`. (Ini menambah command ke-52; 51 lama tak berubah.)
4. Frontend: `src/api.ts` tambah `setLanAccess(enabled: boolean)` → `invoke("set_lan_access", { enabled })`; tambah `lan_access: boolean`
   ke `interface Config`. `SettingsView.vue`: pada bagian "Application bridge" tambah satu toggle **"Allow other computers on the network"**
   dengan teks bantu: "Turn off to accept requests from this computer only. Printer forwarding from other ix.print computers stops working."
   Gunakan komponen toggle/gaya yang sudah dipakai untuk `bridge_enabled` di file yang sama (tiru, jangan buat komponen baru).
5. `lan_url()` (server.rs ±159) dan tampilan IP di UI: jika `lan_access == false`, UI menampilkan `http://127.0.0.1:<port>` saja
   (ubah di frontend berdasarkan `config.lan_access`; jangan ubah `lan_url`).
6. Test: (a) Config lama tanpa field `lan_access` di-deserialize → `true`. (b) `Config` dengan `lan_access=false` round-trip serde.
Commit `feat: optional localhost-only bridge`.

### 3.3 Batas beban per klien (J)
1. Konstanta di `auth.rs`: `pub const MAX_IN_FLIGHT_PER_CLIENT: usize = 8;`.
2. Struct baru `InFlight(Mutex<HashMap<String, usize>>)` di `auth.rs` dengan `fn acquire(&self, key: &str) -> Option<InFlightGuard>`;
   `InFlightGuard` menghapus/mengurangi hitungan saat `Drop` (RAII; wajib, supaya panic/early-return tidak bocor).
   Kunci = `token.id` bila ada token valid, selain itu `ip:<alamat>`. Request dari **desktop window** (memakai `desktop_key`) **dikecualikan**.
3. Tambah field `in_flight: crate::auth::InFlight` di `AppState` (`job.rs`) dan di **semua** tempat `AppState { ... }` dibuat
   (`lib.rs` dan helper `state()` di test `server.rs` ±baris 772; cari dengan `grep -n "AppState {" -r src-tauri/src`).
4. Terapkan **hanya** pada handler yang membuat job/beban: `print`, `submit`/`submit_data`, `print_template`, `test_page` (di `server.rs`).
   Endpoint baca (`/list-printer`, `/queue`, dll.) tidak dibatasi. Bila `acquire` mengembalikan `None`: respons `429` dengan header `Retry-After: 2`
   dan body `{"message":"Too many print requests in progress for this client; retry shortly"}` (pakai helper `error()` yang ada + header).
   Guard dipegang sampai handler selesai.
5. Test: (a) 8 acquire berhasil, ke-9 `None`, setelah satu guard di-drop acquire berhasil lagi. (b) Kunci berbeda tidak saling mempengaruhi.
Commit `feat: cap concurrent print requests per client`.

---

## FASE 4 — Clean Architecture backend

### 4.0 Keputusan arsitektur (FINAL, jangan diubah)
Tetap **satu crate** (`ix_print_lib`); lapisan = modul. Aturan dependensi (panah = boleh mengimpor):

```
interfaces ──► application ──► domain
     │              ▲              ▲
     └────► infrastructure ────────┘   (infrastructure mengimplementasi port milik application)
shared  ◄── boleh diimpor semua lapisan (hanya util kecil tanpa logika bisnis)
```
- **domain**: tipe & aturan bisnis murni. **Dilarang** mengimpor: `tauri`, `actix_*`, `ureq`, `pdfium_render`, `std::fs`,
  `std::process`, `std::net::TcpStream`, `crate::application`, `crate::infrastructure`, `crate::interfaces`.
  Serde, chrono, uuid, sha2, hmac, base64 **boleh**.
- **application**: use case + **port** (trait). Dilarang mengimpor: `tauri`, `actix_*`, `ureq`, `pdfium_render`, `std::fs`, `std::process`,
  `crate::infrastructure`, `crate::interfaces`.
- **infrastructure**: implementasi port (PDFium, CUPS/Windows GDI, file JSON, HTTP client ureq). Dilarang mengimpor `crate::interfaces`, `tauri`, `actix_*`.
- **interfaces**: adapter masuk — `http` (actix) dan `desktop` (Tauri command + tray). Tidak boleh berisi logika bisnis; hanya
  parse request → panggil use case → format respons.
- **Composition root** = `lib.rs::run()` saja: membuat infrastruktur, membungkusnya jadi port, membuat `AppState`, lalu memulai server & Tauri.

### 4.1 Struktur target (pemetaan LENGKAP file → lokasi baru)

```
src-tauri/src/
  main.rs                      (tetap)
  lib.rs                       composition root; ≤ 120 baris (tray → interfaces/desktop/tray.rs)
  shared/
    mod.rs
    sync.rs                    ← sync.rs (Fase 1.3)
  domain/
    mod.rs
    settings/                  ← settings.rs dipecah: mod.rs (re-export), units.rs (mm_to_pt, pt_to_mm, MAX_PAPER_MM),
                                  paper.rs (Paper, STANDARD_PAPERS, Orientation), layout_opts.rs (Scale, Align, Margins),
                                  page_range.rs (PageRange), device_opts.rs (Duplex, ColorMode),
                                  print_settings.rs (PrintSettings, JobSettings, MAX_COPIES, MIN_DPI, MAX_DPI)
    template/                  ← template.rs bagian MODEL saja: mod.rs, model.rs (Template, Layout, FooterMode, ImageFit, HAlign, VAlign,
                                  AUTO_FIELDS, MAX_RECORDS), element.rs (Element + validasinya), record.rs (Record, fill, record_from_json)
    preset.rs                  ← config.rs: Preset, LegacyTemplate, validate_preset_name, PresetBundle, ImportItem, ImportPreview,
                                  ImportAction, ImportDecision, ImportOptions, preview_import, apply_import, BUNDLE_FORMAT
    settings_config.rs         ← config.rs: struct Config, CONFIG_VERSION, DEFAULT_SERVER_PORT, default_server_port, ANY_ORIGIN, normalize_origin
    auth.rs                    ← auth.rs: ApiToken, TOKEN_PREFIX, generate, hash, find, ct_eq, extract
    printer.rs                 ← printer/mod.rs tipe: Printer, PaperInfo, Capabilities, JobState, QueueJob, PrinterQueue, JobStatus,
                                  DeviceOptions, paper_size_from_name
    printer_admin.rs           ← printer/manage.rs tipe & fungsi murni: NetProtocol, Connection, NewPrinter, PrinterEdit, PrinterDetails,
                                  Device, Driver, validate_name, network_uri
    layout.rs                  ← Sheet, LayoutResult (data murni dari layout.rs)
    remote.rs                  ← remote.rs murni: Remote, RemoteView, Target, RemoteError, SEPARATOR, HOP_HEADER, validate_name,
                                  normalize_url, validate_token, resolve, settings_query, RemotePrintResult, CacheEntry
    history.rs                 ← history.rs tipe: HistoryEntry, RemoteJob, CallbackStatus, HistoryDay, HistoryPage
    diagnostics.rs             ← diagnostics.rs tipe: DiagnosticEntry, ActiveJobMarker
    callback.rs                ← callback.rs murni: Callback, MAX_URL, validate_url, is_blocked_ip, signature, Payload, Attempt, should_mark_sent
    job.rs                     ← job.rs tipe: Document, JobRequest, JobResult, JobError, design_paper
  application/
    mod.rs
    ports.rs                   (semua trait, lihat 4.2)
    state.rs                   AppState (dipindah dari job.rs) — field bertipe port/Arc<dyn ...> bila perlu
    job_service.rs             ← job.rs::run(...)
    printer_service.rs         ← logika non-HTTP di commands.rs & server.rs untuk list/capabilities/queue/job_status/printer_views
    preset_service.rs          ← logika save/delete/default preset/import/export dari commands.rs
    token_service.rs           ← create/revoke/require/allowed_origins/usage
    remote_service.rs          ← remote.rs non-I/O: forward(), refresh_printers(), views(), printers() (memakai RemoteGateway)
    callback_service.rs        ← callback.rs::watch (pemantauan job & keputusan retry), memakai CallbackSender
    history_service.rs         ← retention, days/page
    diagnostics_service.rs     ← record/clear/list error log
  infrastructure/
    mod.rs
    pdf/                       ← layout.rs (build, pdfium(), PdfiumGuard, init, Affine, Placement) → layout_engine.rs;
                                  canvas.rs; template.rs bagian RENDER (render, pages_png, preview_png, render_page_png, semua fn pdfium) → template_render.rs;
                                  testpage.rs → test_page.rs
    printing/
      mod.rs                   ← printer/mod.rs: list_printers, os_default_printer, capabilities, default_paper_mm, print, queue, job_status
      cups.rs                  ← printer/cups.rs         (cfg(not(windows)))
      windows.rs               ← printer/windows.rs      (cfg(windows))
      admin/mod.rs, admin_cups.rs (manage_cups.rs), admin_windows.rs (manage_windows.rs)  ← printer/manage*.rs bagian I/O (devices, drivers, details, add, update, remove, set_os_default)
    persistence/
      config_store.rs          ← config.rs::ConfigStore (load/get/update/save atomik, .bak, 0600)
      remote_store.rs          ← remote.rs::RemoteStore, RemoteCache
      history_store.rs         ← history.rs::History (I/O file + Mutex<()>)
      diagnostics_store.rs     ← diagnostics.rs::Diagnostics
    net/
      remote_client.rs         ← remote.rs::fetch_printers, capabilities, queue, job_status, print_pdf, test_page, forward (bagian HTTP ureq)
      callback_client.rs       ← callback.rs::deliver (+ resolver Fase 3.1)
    os.rs                      ← lan_url/local-ip (local-ip-address), open folder helpers bila ada di commands.rs
  interfaces/
    mod.rs
    http/
      mod.rs                   run(), spawn(), routes()
      cors.rs, security.rs (security_headers), auth_guard.rs (authorize, unauthorized, in-flight guard)
      dto.rs                   query/body struct milik HTTP (mis. callback_url query) dan helper error()/internal()
      handlers/{meta.rs (hello, docs), printers.rs (list_printers, capabilities, queue), jobs.rs (print, submit, submit_data, save_upload, print_template, test_page, job_status), presets.rs (presets, export_presets, templates)}.rs
      assets/docs.html         ← src/docs.html (perbarui include_str!)
    desktop/
      mod.rs                   register_commands() → berisi generate_handler![...] (51+1 command)
      tray.rs                  ← lib.rs setup_tray, focus_main
      commands/{app.rs, bridge.rs, printers.rs, printer_admin.rs, presets.rs, tokens.rs, remotes.rs, history.rs, diagnostics.rs, print.rs}.rs
                               ← commands.rs dipecah PERSIS sesuai kelompok nama di 1.2:
                               app: open_docs data_folder_path open_data_folder this_local_ip os_detected engine_status server_status
                               bridge: bridge_enabled set_bridge_enabled(+_inner) set_server_port set_lan_access
                               remotes: remote_*; printers: get_printers get_capabilities get_queue job_status printer_set_os_default
                               printer_admin: printer_devices printer_drivers printer_details printer_add printer_update printer_remove
                               presets: get_config set_default_* save_preset delete_preset preview_template preview_print export_presets preview_import apply_import
                               tokens: create_token revoke_token set_require_token set_allowed_origins token_usage
                               history: set_history_retention history_days history_page
                               diagnostics: error_log record_error clear_error_log open_system_logs
                               print: app_print preview_pdf_sheet test_print
  assets/RSUD.pdf              tetap di tempat; perbarui path include_* jika ada (cek dengan grep "RSUD" sebelum memindah)
```
Aturan tambahan:
- Modul tingkat atas di `lib.rs` menjadi `pub mod shared; pub mod domain; pub mod application; pub mod infrastructure; pub mod interfaces;`
  (dibuat `pub` supaya `tests/*.rs` bisa mengaksesnya). Modul lama dihapus setelah semua isinya dipindah.
- Selama transisi, tiap file lama diganti sementara oleh `pub use crate::<lokasi_baru>::*;` supaya kompilasi tetap hijau tiap langkah;
  shim dihapus pada langkah **4.9**.
- `cfg(windows)`/`cfg(not(windows))` dan `cfg_attr` dipertahankan persis.
- Jika sebuah fungsi mencampur logika murni dan I/O (mis. `preview_import` membaca file lalu memproses), **pisahkan**: bagian membaca file
  tinggal di `infrastructure`/`interfaces`, bagian murni ke `domain`/`application`. Jangan mengubah urutan efek samping atau pesan error.

### 4.2 Port (trait) — didefinisikan di `application/ports.rs`
Semua trait: `pub trait X: Send + Sync`. Tipe argumen/return = tipe `domain` yang sudah ada. **Jangan** mengubah signature fungsi yang dibungkus;
port hanya mengangkat signature fungsi yang sudah ada menjadi method.

| Port | Method (diangkat dari fungsi yang ada) | Implementasi |
|---|---|---|
| `ConfigRepository` | `get() -> Config`, `update(FnOnce(&mut Config)->Result<(),String>)` (gunakan `fn update(&self, f: &mut dyn FnMut(&mut Config) -> Result<(), String>) -> Result<Config,String>` agar object-safe), `data_dir()` | `ConfigStore` |
| `RemoteRepository` | `list()`, `add/update/remove` (sesuai method `RemoteStore` yang ada) | `RemoteStore` |
| `HistoryRepository` | semua method publik `History` yang dipakai `job.rs`, `callback.rs`, `commands.rs`, `server.rs` | `History` |
| `DiagnosticsRepository` | `record`, `list`, `clear`, `begin_job`/marker (sesuai `Diagnostics`) | `Diagnostics` |
| `PrinterGateway` | `list_printers`, `os_default_printer`, `capabilities`, `default_paper_mm`, `print`, `queue`, `job_status` | modul `infrastructure/printing` |
| `PrinterAdmin` | `devices, drivers, details, add, update, remove, set_os_default` | `infrastructure/printing/admin` |
| `PdfEngine` | `build_layout(source, output, &JobSettings, default_paper_mm) -> Result<LayoutResult,String>`, `render_template(path,&Template,&[Record],&Record)`, `pages_png(...)`, `preview_png(...)`, `render_page_png(...)`, `test_page(...)` | `infrastructure/pdf` |
| `RemoteGateway` | `fetch_printers, capabilities, queue, job_status, print_pdf, test_page` | `infrastructure/net/remote_client` |
| `CallbackSender` | `deliver(&Callback,&Payload, progress: &mut dyn FnMut(&str,u32,&Attempt))` | `infrastructure/net/callback_client` |
| `Clock` (baru, kecil) | `now_rfc3339() -> String` | `SystemClock` (chrono::Local) — dipakai use case yang sekarang memanggil `chrono::Local::now()` langsung, agar bisa dites |

`AppState` (application/state.rs) menyimpan `Arc<dyn Port>` untuk masing-masing di atas ditambah field non-port yang sudah ada
(`desktop_key`, `server_error`, `server_handle`, `token_usage`, `failed_auth`, `in_flight`, `jobs_dir`).
`server_handle` (tipe actix) **tidak boleh** ada di `application`; pindahkan ke struct `ServerRuntime { handle, error }` di
`interfaces/http` dan simpan di Tauri managed state terpisah **atau** bungkus sebagai `Arc<dyn ServerControl>` port
(`start()`, `stop()`, `last_error()`); pilihan FINAL: **port `ServerControl`** di `application/ports.rs`, implementasi di `interfaces/http`.

### 4.3 Urutan langkah (masing-masing = 1 commit, GATE hijau, tanpa perubahan perilaku)
Penomoran di bagian ini (**L4.1–L4.12**) adalah *langkah kerja*, berbeda dari nomor sub-bagian 4.0–4.3. Semua rujukan "Langkah 4.x" di
dokumen ini berarti langkah L4.x di sini. Untuk semua langkah: bila kode lama masih dipakai file lain, biarkan shim `pub use` sampai L4.9.

- **4.1** Buat kerangka folder + `mod.rs` kosong lima lapisan, daftar di `lib.rs`. Buat `src-tauri/tests/architecture.rs` (lihat 4.10). Commit.
- **4.2** Pindahkan `settings.rs` → `domain/settings/*` (pecahan sesuai peta), tinggalkan shim `pub use`. Pindahkan `sync.rs` → `shared/sync.rs`.
- **4.3** `auth.rs` → `domain/auth.rs`. Struct stateful (`FailedAttempts`, `TokenUsage`, `InFlight`) **tetap** di `application/` sebagai `application/security.rs`
  (mereka in-memory state, bukan I/O). `TokenUsage`/`FailedAttempts`/`InFlight` dipindah bersama test terkait.
- **4.4** `template.rs` dipecah: model → `domain/template/*`; render pdfium → `infrastructure/pdf/template_render.rs`. Lalu `canvas.rs`,
  `layout.rs` (Sheet/LayoutResult → `domain/layout.rs`; sisanya → `infrastructure/pdf/layout_engine.rs`), `testpage.rs` → `infrastructure/pdf/test_page.rs`.
  Pastikan test layout PDFium tetap berjalan (jumlah test tidak turun).
- **4.5** `config.rs` dipecah: `Config`/`Preset`/`PresetBundle`/import → `domain/*`; `ConfigStore` → `infrastructure/persistence/config_store.rs`.
  `history.rs`, `diagnostics.rs`, `remote.rs`(store) → `infrastructure/persistence/*`; tipenya → `domain/*`.
- **4.6** `printer/*` → `domain/printer.rs`, `domain/printer_admin.rs`, `infrastructure/printing/*`. Pertahankan struktur `cfg`.
- **4.7** Definisikan semua port di `application/ports.rs`; buat implementasi (`impl Port for ConfigStore`, dst.) di file infrastrukturnya.
  Pindahkan `AppState` ke `application/state.rs` dengan field port; ubah `lib.rs` (composition root) merakit semuanya.
  Semua test yang membuat `AppState { ... }` (di `server.rs`, `commands.rs`, dll.) diubah memakai **helper test tunggal**
  `application::state::test_support::state()` (di `#[cfg(test)]`) — hapus duplikasi helper `state()` di tiap file test.
- **4.8** Use case: `job.rs::run` → `application/job_service.rs`; callback watch → `callback_service.rs`; forward/refresh remote → `remote_service.rs`;
  logika non-transport dari `commands.rs`/`server.rs` → `printer_service.rs`, `preset_service.rs`, `token_service.rs`, `history_service.rs`,
  `diagnostics_service.rs`. Setiap use case menerima `&AppState` (atau port yang dibutuhkan) — bukan tipe konkret infrastruktur.
  **Tambahkan test dengan fake port** (lihat 4.11).
- **4.9** `server.rs` → `interfaces/http/*` (pecahan sesuai peta), `commands.rs` + tray → `interfaces/desktop/*`. `generate_handler!` dipindah ke
  `interfaces/desktop/mod.rs`. Handler hanya: ekstrak input → panggil service → petakan hasil ke respons/`Result<_, String>`.
  Setelah selesai, **hapus semua shim** `pub use` dan file lama. `grep -rn "pub use crate::" src-tauri/src` untuk shim harus kosong.
- **4.10** Test penegak arsitektur `src-tauri/tests/architecture.rs`: baca semua `*.rs` di `src/domain`, `src/application`, `src/infrastructure`
  (rekursif, `std::fs`), dan untuk tiap lapisan pastikan tidak ada baris (abaikan baris komentar `//`) yang mengandung token terlarang
  dari 4.0: `tauri`, `actix_web`, `actix_cors`, `actix_multipart`, `ureq`, `pdfium_render`, `std::fs`, `std::process`, `crate::interfaces`
  (untuk domain/application/infrastructure), `crate::infrastructure` (untuk domain/application), `crate::application` (untuk domain).
  Kecualikan blok `#[cfg(test)]` **tidak** — test di domain pun tidak boleh memakai `std::fs`; gunakan `tempfile`-free pendekatan
  `std::env::temp_dir()` hanya di `infrastructure`. Jika test lama di domain memakai `fs`, pindahkan test-nya ke infrastructure.
  Test harus gagal bila ada pelanggaran (uji sekali secara manual dengan menambah `use tauri;` sementara di domain, lalu hapus).
  Tambah juga assert: tidak ada file `.rs` di `src/` selain `docs.html` yang > 450 baris (kecuali file yang namanya `tests.rs`).
- **4.11** Test baru wajib (fake port, tanpa PDFium/printer/jaringan):
  1. `job_service`: printer tidak ada → `JobError::BadRequest`; preset tak dikenal → `BadRequest`; `PdfEngine` gagal → `Failed`;
     sukses → `HistoryRepository` menerima 1 entri `success=true` dan `PrinterGateway::print` dipanggil sekali dengan jumlah sheet yang benar.
  2. `token_service`: create → hash tersimpan, plain token tak tersimpan; revoke menghapus; `require_token` toggle.
  3. `preset_service`: import dengan aksi `overwrite/skip/rename` (sudah dites di `config.rs` — pindahkan, jangan hapus).
  4. `remote_service::forward`: batas satu hop (`HOP_HEADER`) → error yang sama seperti sekarang.
- **4.12** Dokumentasi: `docs/ARCHITECTURE.md` (diagram lapisan dari 4.0 + tabel peta 4.1 + aturan "di mana menaruh kode baru").
  Perbarui README bagian Development dengan perintah GATE. Commit.

Gate akhir Fase 4: semua test lama lulus (jumlah ≥ baseline + test baru), `cargo test --test '*'` hijau, aplikasi berjalan (`bun run tauri dev`)
dan **uji manual** daftar di Bagian 8.

---

## FASE 5 — Verifikasi kontrak backend
1. Jalankan test `contract_all_routes_registered` (Langkah 0.4) — harus tetap ada & hijau (pindahkan ke `interfaces/http/tests.rs`).
2. Bandingkan `generate_handler!` dengan daftar 1.2 + `set_lan_access`. Tambah test `all_tauri_commands_registered` yang memeriksa daftar nama
   (simpan sebagai `const COMMANDS: &[&str]` di `interfaces/desktop/mod.rs` dan pakai konstanta itu juga di test; jangan duplikasi manual — jika
   `generate_handler!` tak bisa dibuat dari konstanta, cukup grep-test: baca file itu dan pastikan tiap nama muncul).
3. Deserialize fixture `config.json` lama (ambil dari struct default **sebelum** Fase 3, simpan di `src-tauri/tests/fixtures/config_v2.json`;
   buat dengan `serde_json::to_string_pretty(&Config::default())` di test satu kali lalu hard-code nilainya tanpa field `lan_access`) → harus terbaca.
Commit `test: freeze desktop command and config contracts`.

---

## FASE 6 — Frontend: struktur berbasis fitur, tanpa mengubah tampilan/perilaku

### 6.0 Keputusan (FINAL)
Struktur target `src/`:
```
src/
  main.ts
  app/
    App.vue                    (≤ 250 baris: shell, layout, <router-less> tab switch)
    useAppShell.ts             (tab aktif, event `tray-navigate`, onboarding flag)
    usePolling.ts              (lihat 6.5)
  shared/
    api/
      client.ts                (helper invoke, isInApp, errorText)
      types.ts                 (semua interface: PrintSettings, Config, Preset, Printer*, Job*, History*, Import*, Diagnostic*, dst.)
      commands/{config.ts, printers.ts, printerAdmin.ts, presets.ts, tokens.ts, remotes.ts, history.ts, diagnostics.ts, print.ts, app.ts}
      index.ts                 (mengekspor objek `api` dengan **kunci & signature yang sama persis** seperti `api` lama; implementasi = gabungan spread)
    lib/
      template.ts              (AUTO_FIELDS, templateAutoFields, templateFields)
      job.ts                   (FINAL_STATES, describeJob)
      callback.ts              (CALLBACK_PENDING, describeCallback)
      preset.ts                (summarize)
    ui/                        (komponen umum: AutocompleteField.vue, EmptyState.vue, SettingInfo.vue)
  features/
    printers/   (PrintersTab.vue, PrinterDeviceForm.vue, usePrinters.ts)
    presets/    (PresetsTab.vue, PresetForm.vue, ImportDialog.vue)
    design-editor/ (lihat 6.4)
    print/      (PrintTab.vue)
    history/    (HistoryTab.vue)
    api-tokens/ (ApiTab.vue)
    remotes/    (RemotesPanel.vue)
    settings/   (SettingsView.vue, OnboardingDialog.vue)
    diagnostics/(ErrorLogPanel.vue)
```
Aturan: `shared` tidak boleh mengimpor dari `features` atau `app`. `features/x` tidak boleh mengimpor `features/y`
(komunikasi lewat props/emit atau `shared`). Gunakan alias `@/` → `src/` (tambah `resolve.alias` di `vite.config.ts` dan `paths` di `tsconfig.json`).
Import lama `import { api } from "../api"` diganti `import { api } from "@/shared/api"`. Tidak ada pustaka state/router baru.

### 6.1 Langkah `api.ts` (528 baris)
Pisahkan sesuai peta 6.0: tipe → `types.ts`; fungsi murni (`templateFields`, `describeJob`, `describeCallback`, `summarize`) → `shared/lib/*`;
objek `api` (baris 424–503) → dipecah per domain lalu digabung di `index.ts`. **Semua nama export lama tetap tersedia** lewat `src/api.ts`
sebagai shim `export * from "./shared/api"; export * from "./shared/lib/..."` sampai Langkah 6.6, lalu shim dihapus.

### 6.2 Pindahkan komponen ke `features/*` dan `shared/ui` sesuai peta (hanya `git mv` + perbaiki import). Satu commit per fitur.

### 6.3 `App.vue` (1048 baris) → ekstrak ke `app/useAppShell.ts`, `app/usePolling.ts`, dan komponen anak
`app/AppSidebar.vue`, `app/AppHeader.vue` **hanya jika** bagian tersebut berupa blok `<template>` mandiri di file sekarang.
Aturan: logika `<script setup>` dikelompokkan per tanggung jawab (tab/navigasi, printers+IP, server status, onboarding, tray event)
menjadi composable; **nama state yang dipakai template tidak berubah** (destructure dari composable dengan nama sama).
Jika suatu bagian tidak bisa dipisah tanpa mengubah perilaku, biarkan di App.vue dan laporkan.

### 6.4 `DesignEditor.vue` (1024 baris) → `features/design-editor/`
Pecah menjadi (hanya yang benar-benar ada di file; jangan mengarang perilaku): `DesignEditor.vue` (orkestrator), `DesignCanvas.vue`,
`DesignToolbar.vue`, `DesignLayersPanel.vue`, `DesignPropertiesPanel.vue`, dan composable `useDesignLayers.ts`, `useDesignClipboard.ts`
(copy/paste layer), `useDesignHistory.ts` (undo/redo bila ada), `useDesignPointer.ts` (drag/resize). Kontrak props/emit `DesignEditor`
ke `PresetForm` **tidak berubah**. Tiap file ≤ 400 baris.

### 6.5 Polling hanya saat terlihat (H)
`app/usePolling.ts`:
```ts
export function usePolling(fn: () => void | Promise<void>, ms: number) { /* start/stop */ }
```
Perilaku FINAL: timer berjalan hanya bila `document.visibilityState === "visible"`; saat kembali visible, jalankan `fn` **segera** sekali lalu lanjutkan interval;
saat hidden, `clearInterval`. Bersihkan di `onBeforeUnmount`. Ganti `window.setInterval(() => refreshPrinters(), 3000)` dan `refreshIp` 10000 (App.vue ±277)
dengan dua pemanggilan `usePolling`. Interval tetap 3000 dan 10000 ms. Jangan mencegah pemanggilan awal `onMounted`.
Catatan: window Tauri yang di-minimize/disembunyikan ke tray memicu `visibilitychange`; tidak perlu API Tauri tambahan.

### 6.6 Hapus shim `src/api.ts`; pastikan tidak ada import relatif lama (`grep -rn "from \"\.\./api\"\|from \"\./api\"" src` kosong).

### 6.7 Vitest (B)
1. `bun add -d vitest @vue/test-utils jsdom` (versi terbaru yang kompatibel dengan vite 6; jangan mengubah dependency lain).
2. `vite.config.ts` tambah `test: { environment: "jsdom", include: ["src/**/*.test.ts"] }`; `package.json` scripts: `"test": "vitest run"`.
3. Tulis test (semua di sebelah file-nya, `*.test.ts`) untuk: `templateFields`/`templateAutoFields` (AUTO_FIELDS tidak ikut sebagai field input),
   `describeJob` untuk tiap `state` (`queued` dengan/ tanpa `ahead`, `printing`, `paused`, `error`, `completed`, `gone`), `describeCallback`
   untuk tiap status (`watching, retrying, completed, sent, failed, gone, timeout, error`), `summarize` (preset kosong & terisi), `errorText`
   (string, `Error`, objek `{message}`, lainnya), dan `usePolling` (fake timers: berjalan saat visible, berhenti saat hidden, langsung fire saat visible lagi).
4. Tambah langkah `bun run test` ke job `frontend` di `ci.yml`.
5. Commit `test(frontend): cover pure helpers and polling`.

### 6.8 Format
Jalankan `bunx prettier --write "src/**/*.{ts,vue}"` **hanya** pada file yang Anda pindahkan/pecah di Fase 6 (bukan seluruh repo), dalam commit terpisah `style: format moved frontend files`.

Gate akhir Fase 6: `bun run build` & `bun run test` hijau; uji manual Bagian 8.

---

## FASE 7 — Rilis & distribusi (E)

**Kerjakan (tanpa secret):**
1. `build-installers.yml`: tambah dua entri matrix — `Windows ARM64` (`os: windows-11-arm`, `pdfium: win-arm64`, `bundles: nsis,msi`) dan
   `Linux ARM64` (`os: ubuntu-24.04-arm`, `pdfium: linux-arm64`, `bundles: appimage,deb`). Langkah lain sama. Jika `bun run fetch-pdfium win-arm64`
   / `linux-arm64` gagal di CI karena target tak dikenal, **berhenti dan laporkan** (jangan menulis ulang skrip fetch).
2. Tambah langkah **awal** job build: `cargo test --lib` (working-directory `src-tauri`) supaya installer tak pernah dibuat dari kode yang test-nya merah.
3. Pada langkah "Build installer" tambah blok `env:` yang hanya meneruskan secret bila ada (nilai kosong = no-op oleh Tauri):
   `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`
   dari `${{ secrets.NAMA }}`. Untuk Windows: `TAURI_SIGNING_PRIVATE_KEY` **tidak** dipakai (itu untuk updater, bukan code signing).
   Jangan mencetak env di log.
4. Buat `docs/RELEASE.md`: (a) cara merilis (tag `v*`), (b) daftar secret GitHub yang harus saya isi beserta artinya untuk macOS notarization,
   (c) opsi code signing Windows (sertifikat OV/EV atau Azure Trusted Signing) sebagai **keputusan saya**, (d) bagian "Auto-update" berisi:
   "Belum diimplementasi. Butuh: keypair `tauri signer generate`, `pubkey` di `tauri.conf.json`, endpoint `latest.json`, plugin `tauri-plugin-updater`."

**JANGAN dikerjakan** (butuh keputusan/secret saya): membuat keypair, menambah `tauri-plugin-updater`, mengubah `tauri.conf.json` untuk updater,
membeli/mengonfigurasi sertifikat. Jika kamu tergoda mengerjakannya — berhenti dan tanya.

---

## 8. UJI MANUAL WAJIB (di akhir Fase 4 dan Fase 6; laporkan hasil centang satu per satu)

Jalankan `bun run tauri dev`, lalu:
1. Window terbuka, tab: Print, Printers, Presets, History, API, Settings semua tampil; tray muncul dengan 7 item menu; klik "View error log" membuka tab error.
2. `curl http://127.0.0.1:18032/` → 200. `curl http://127.0.0.1:18032/list-printer` tanpa token → 401 (Require token ON default).
3. Buat token di tab API → `curl -H "Authorization: Bearer <token>" .../list-printer` → JSON daftar printer.
4. Upload PDF ke `/print` dengan token dan printer virtual/PDF (macOS: "Save as PDF"/CUPS-PDF) → 200 dengan `job_id`; muncul di History.
5. `POST /print-template` dengan preset ber-desain (buat dari tab Presets) → 200; preview di editor tampil; **copy-paste layer** di editor berfungsi.
6. Import/export preset (ekspor lalu impor kembali dengan `rename`).
7. Ubah port di Settings → server pindah port; toggle bridge off/on lewat tray → port menutup/membuka.
8. (Fase 3) Matikan "Allow other computers on the network" → `curl http://<ip-LAN>:18032/` gagal, `127.0.0.1` berhasil; hidupkan lagi → LAN berhasil.
9. (Fase 3) `callback_url=http://localhost:9/x` ditolak 400; `callback_url=http://192.168.x.x:PORT/hook` diterima.
10. Fase 6.5: buka DevTools → Network/Console: saat window di-minimize, pemanggilan `get_printers` berhenti; saat dibuka kembali langsung memanggil sekali.

---

## 9. BATAS TUGAS & KELUARAN

Di luar cakupan (jangan dikerjakan): fitur baru selain `lan_access` & limit in-flight, perubahan UI/visual, upgrade dependency, perubahan versi config,
updater, signing, i18n penuh, migrasi ke workspace multi-crate, perubahan skema HTTP/Tauri.

Format laporan **di akhir setiap fase**:
```
FASE N selesai
- Commit: <hash> <pesan> (satu baris per commit)
- Test: cargo test --lib <lulus>/<total> (baseline <n>), tests/* <lulus>, bun test <lulus>
- Clippy/fmt/build: OK/GAGAL
- File terbesar sekarang: <5 file terbesar + jumlah baris>
- Penyimpangan dari dokumen ini: <tidak ada | uraian + alasan>
- Pertanyaan tertunda: <tidak ada | daftar>
```
Berhenti setelah laporan dan tunggu "lanjut".
