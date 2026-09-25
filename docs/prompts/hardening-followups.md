# Prompt implementasi: Hardening repo (T1-T7) dan kepastian hasil cetak (T8-T16)

> Berikan seluruh isi file ini ke AI coding agent yang bekerja di repo ini.
> Semua path, nama fungsi, dan teks di bawah diambil dari kode saat prompt ini
> ditulis (branch `refactor/clean-architecture`). Kalau ada yang tidak cocok
> dengan kode yang kamu baca, **berhenti dan laporkan perbedaannya**. Jangan
> menyesuaikan diam-diam dan jangan memilih alternatif sendiri.

---

## 0. Aturan kerja

1. **Kerjakan tugas berurutan, satu commit per tugas** (T1, T2, ...). Setiap
   commit harus lulus gerbang di §9 sebelum lanjut ke tugas berikutnya.
2. **Jangan mengarang.** Semua nama yang kamu pakai sudah ada di kode atau
   ditulis di prompt ini. Kalau butuh API crate atau library, baca sumbernya
   dulu. Jangan menebak dari ingatan.
3. **Baca file target utuh sebelum mengedit.** Jangan mengubah file di luar
   daftar "File yang boleh disentuh" pada tiap tugas.
4. **Perubahan perilaku hanya yang tertulis.** T2 dan T3 adalah refactor murni
   (perilaku identik). T5 dan T8-T16 mengubah perilaku; tiap tugas menjelaskan
   persis apa yang berubah. Tabel ringkasnya ada di awal Bagian B.
5. Gaya kode: ikuti file sekitarnya. Teks UI, pesan error, dan komentar kode
   dalam **bahasa Inggris**. Komentar seperlunya, satu baris.
6. **Jangan** menambah dependency Rust/JS baru, kecuali yang tertulis di
   tugas. **Jangan** mengubah `Cargo.toml`, `package.json`, `bun.lock`
   kecuali tertulis.
7. Kalau ada keputusan yang tidak tercakup di sini, **tanya pengguna**.
8. Format commit: `type(scope): ringkasan` (contoh: `chore: remove stale
   capabilities file`), diakhiri baris:
   `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`.
9. Semua perintah Rust dijalankan dari `src-tauri/`. Semua perintah `bun`
   dijalankan dari root repo.

## Di luar cakupan (JANGAN dikerjakan)

- Mengganti `eprintln!` dengan crate logging. Aplikasi sudah punya
  `Diagnostics` (`infrastructure/persistence/diagnostics_store.rs`); jangan
  menambah sistem log kedua.
- Memecah `PrintTab.vue`, `PresetForm.vue`, `PrinterDeviceForm.vue`.
- Menambah ESLint, test komponen Vue, test E2E, code signing, auto-updater.
- Mengubah `bundle.targets`, CSP `style-src`, atau versi Tauri.
- Membuat klaim bahwa perilaku printer fisik sudah terverifikasi. Empat test
  backend yang ditandai `#[ignore]` (visual/perangkat langsung) **tetap
  ignored**; jangan di-un-ignore dan jangan dihapus.

---

## T1. Hapus `capabilities/default.txt`

**Latar:** `src-tauri/capabilities/default.txt` adalah salinan lama dari
`default.json` (memuat `shell:default`, tanpa `dialog`/`os`/`autostart`).
Tauri hanya membaca `*.json`/`*.toml`, jadi file ini mati dan membingungkan
saat audit permission.

Langkah:

1. Jalankan `git grep -n "default.txt"`. Hasil harus kosong. Kalau ada
   referensi, **berhenti dan laporkan**.
2. `git rm src-tauri/capabilities/default.txt`
3. Jangan sentuh `default.json`.
4. Commit: `chore: remove stale capabilities/default.txt`.

---

## T2. Satukan pengecekan "desktop request" (refactor murni)

**File yang boleh disentuh:** `src-tauri/src/interfaces/http/auth_guard.rs`.

**Latar:** kondisi bypass desktop ditulis dua kali: di `authorize` (baris
awal) dan di `acquire_in_flight` (variabel `is_desktop`). Keduanya harus
selalu identik.

Langkah:

1. Di `auth_guard.rs`, **di atas** `pub(super) fn authorize`, tambahkan:

   ```rust
   /// The desktop UI's own `POST /print` from loopback, proven by the per-run desktop key.
   fn is_desktop_request(req: &HttpRequest, state: &AppState) -> bool {
       let key = req.headers().get("x-ixprint-desktop").and_then(|v| v.to_str().ok());
       req.method() == actix_web::http::Method::POST
           && req.path() == "/print"
           && req.peer_addr().is_some_and(|addr| addr.ip().is_loopback())
           && key.is_some_and(|k| crate::domain::auth::ct_eq(k.as_bytes(), state.desktop_key.as_bytes()))
   }
   ```

2. Di `authorize`: hapus variabel `desktop_key`, dan ganti blok
   `if req.method() == ... { return Ok(None); }` dengan:

   ```rust
   if is_desktop_request(req, state) {
       return Ok(None);
   }
   ```

   Variabel `ip` **tetap dipertahankan** (dipakai di bawahnya untuk
   `failed_auth`).
3. Di `acquire_in_flight`: hapus variabel `desktop_key` dan `is_desktop`,
   ganti dengan:

   ```rust
   if is_desktop_request(req, state) {
       return Ok(None);
   }
   ```

4. Jangan mengubah urutan atau logika lain di kedua fungsi.
5. Verifikasi: `cargo test --lib interfaces::http` lulus tanpa mengubah test.
6. Commit: `refactor(http): share the desktop-request check`.

---

## T3. Hapus dua `.expect("guard checked design")` (refactor murni)

**File yang boleh disentuh:**
`src-tauri/src/application/job_service.rs` dan
`src-tauri/src/application/remote_service.rs`.

**Latar:** di kedua file ada dua arm `match` berurutan:

```rust
Document::TestPage { pages } if preset.design.is_some() => {
    let design = preset.design.as_ref().expect("guard checked design");
    ...
}
Document::TestPage { pages } => { ... }
```

`expect` akan panic kalau guard suatu hari diubah. Gabungkan jadi satu arm
dengan `match` dalam, tanpa `expect`.

Langkah, untuk **masing-masing** file (`job_service.rs` sekitar baris 127,
`remote_service.rs` sekitar baris 293):

1. Baca kedua arm `Document::TestPage` sampai selesai.
2. Ganti keduanya dengan satu arm:

   ```rust
   Document::TestPage { pages } => match preset.design.as_ref() {
       Some(design) => {
           // isi lama arm pertama, TANPA baris `let design = ... expect(...)`
       }
       None => {
           // isi lama arm kedua, apa adanya
       }
   },
   ```

3. Pindahkan isi kedua arm **apa adanya** (hanya indentasi berubah). Nilai
   yang dihasilkan tiap cabang (`generated.clone()` di `job_service.rs`;
   hasil `state.remote_gateway...` di `remote_service.rs`) harus tetap
   sama.
4. Jalankan `cargo fmt --all` (dari `src-tauri`) supaya indentasi sesuai.
5. Verifikasi: `git grep -n 'guard checked design' src-tauri/src` harus
   kosong. `cargo test --lib application` lulus.
6. Commit: `refactor(application): remove expect from test page arms`.

---

## T4. Format frontend + gerbang `prettier --check` di CI

**Latar:** `bunx prettier --check src` saat ini gagal di 9 file:
`src/app/app.css`, `src/app/usePolling.test.ts`,
`src/features/design-editor/design-editor.css`, `src/main.ts`,
`src/shared/api/client.test.ts`, `src/shared/lib/callback.test.ts`,
`src/shared/lib/job.test.ts`, `src/shared/lib/preset.test.ts`,
`src/shared/lib/template.test.ts`.

Langkah:

1. `bunx prettier --write src` (konfigurasi ada di `.prettierrc`; jangan
   diubah).
2. Pastikan `git diff --stat` **hanya** menyentuh file di daftar atas. Kalau
   ada file lain, `git checkout` file itu dan laporkan.
3. Di `package.json`, tambahkan satu script (hanya ini, jangan ubah yang
   lain), setelah `"test": "vitest run",`:

   ```json
   "format:check": "prettier --check src",
   ```

4. Di `.github/workflows/ci.yml`, job `frontend`, tambahkan step **sebelum**
   step `Build frontend`:

   ```yaml
         - name: Check formatting
           run: bun run format:check
   ```

   (indentasi mengikuti step di sekitarnya).
5. Verifikasi: `bun run format:check`, `bun run build`, `bun run test` lulus.
6. Commit: `style(frontend): apply prettier and check it in CI`.

---

## T5. Token via `?token=` hanya untuk GET (PERUBAHAN PERILAKU)

> Ini satu-satunya tugas yang mengubah perilaku. Commit-nya harus terpisah
> supaya mudah di-revert.

**File yang boleh disentuh:**
`src-tauri/src/interfaces/http/auth_guard.rs`,
`src-tauri/src/interfaces/http/tests.rs`,
`src/features/api-tokens/ApiTab.vue`,
`src-tauri/src/interfaces/http/assets/docs.html`, `README.md`.

**Perilaku baru:**

| Request | Header token | `?token=` | Hasil |
|---|---|---|---|
| GET | ada | apa saja | header dipakai (seperti sekarang) |
| GET | tidak ada | valid/invalid | query dipakai (seperti sekarang) |
| POST | ada | apa saja | header dipakai (seperti sekarang) |
| POST | tidak ada | ada | **401**, pesan di bawah |
| POST | tidak ada | tidak ada | seperti sekarang |

Pesan 401 (persis): `Send the token in the Authorization header; ?token= is only accepted on GET requests`

Langkah:

1. Di `authorize` (`auth_guard.rs`), ganti pembentukan `provided` (sekarang
   memanggil `auth::extract(header("authorization"), header("x-api-token"),
   query...)`) dengan:

   ```rust
   let query_token = query.as_ref().and_then(|q| q.get("token")).map(String::as_str);
   let header_token = auth::extract(header("authorization"), header("x-api-token"), None);
   let is_get = req.method() == actix_web::http::Method::GET;
   if header_token.is_none() && !is_get && query_token.is_some_and(|t| !t.trim().is_empty()) {
       return Err(unauthorized(
           "Send the token in the Authorization header; ?token= is only accepted on GET requests",
       ));
   }
   let provided = header_token.or_else(|| {
       is_get
           .then(|| auth::extract(None, None, query_token))
           .flatten()
   });
   ```

   Sisa `match provided { ... }` **tidak diubah**.
2. **Jangan** ubah `domain/auth.rs::extract` (tetap generik, punya test
   sendiri).
3. Di `tests.rs`, fungsi `token_protection`, cari blok yang memakai
   `format!("/presets?token={}", token)` (sekitar baris 382, mengharapkan
   200). Tepat setelah assert itu, tambahkan (pakai helper `status` dan
   variabel `token` yang sudah ada di scope itu):

   ```rust
   // ?token= is refused on POST, so it never reaches a request body handler.
   assert_eq!(
       status(test::TestRequest::post().uri(&format!("/test-page?token={}", token))).await,
       401
   );
   ```

   Kalau setelah dibaca ternyata `token` sudah tidak valid di titik itu
   (mis. sudah di-revoke), pindahkan assert ke titik saat token masih
   valid, tetapi tetap di dalam `token_protection`.
4. `ApiTab.vue` (sekitar baris 195-199): ubah `<pre>` dan hint menjadi:

   ```
   Authorization: Bearer fxp_…
   X-Api-Token: fxp_…
   ?token=fxp_…      (GET only)
   ```

   dan hint: `Avoid <code>?token=</code>: it's recorded in logs and browser history, and POST requests refuse it.`
5. `docs.html` (sekitar baris 245-248): ganti baris terakhir `<pre>` menjadi
   `?token=fxp_…      // GET only; recorded in logs and browser history`.
6. `README.md` (sekitar baris 100): ganti akhir kalimat menjadi
   `` `X-Api-Token: fxp_…`, or `?token=fxp_…` (GET requests only).``
7. Verifikasi: `cargo test --lib interfaces::http` lulus, `bun run build`
   lulus.
8. Commit: `feat(http): accept ?token= on GET requests only`.

---

## T6. Peringatan LAN terbuka tanpa token di Settings

**File yang boleh disentuh:** `src/features/settings/SettingsView.vue`.

**Latar:** `ApiTab.vue` sudah memperingatkan saat token opsional, tapi
pengguna yang hanya membuka Settings (kartu "Allow other computers on the
network") tidak melihat risikonya.

Langkah:

1. Di template, kartu dengan checkbox `config.lan_access` berisi satu
   `<p class="hint">`. **Tepat setelah** `</p>` itu (masih di dalam
   `<div class="card stack tight">`), tambahkan:

   ```vue
   <p v-if="config.lan_access && !config.require_token" class="warn-text small-text">
     ⚠ Anyone on your network can print without a token. Turn on "Require a token to access the API" in the API tab.
   </p>
   ```

2. `config.require_token` sudah ada di tipe `Config`
   (`src/shared/api/types.ts`); jangan ubah tipe.
3. Kelas `warn-text` dan `small-text` sudah ada dan dipakai di `ApiTab.vue`.
   Jangan menambah CSS.
4. Verifikasi: `bunx prettier --check src` dan `bun run build` lulus.
5. Commit: `feat(settings): warn when LAN access is open without a token`.

---

## T7. Dependabot dan audit dependensi

**File baru:** `.github/dependabot.yml`, `.github/workflows/audit.yml`.
Jangan ubah workflow lain.

`.github/dependabot.yml` (isi persis):

```yaml
version: 2
updates:
  - package-ecosystem: cargo
    directory: /src-tauri
    schedule:
      interval: weekly
    ignore:
      # Pinned on purpose (Cargo.toml: tauri = "=2.11.5"); bump by hand.
      - dependency-name: tauri
  - package-ecosystem: bun
    directory: /
    schedule:
      interval: weekly
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
```

`.github/workflows/audit.yml` (isi persis):

```yaml
name: Audit

on:
  schedule:
    - cron: "0 3 * * 1"
  workflow_dispatch:
  pull_request:
    paths:
      - src-tauri/Cargo.lock
      - bun.lock

permissions:
  contents: read
  checks: write

jobs:
  rust:
    name: cargo audit
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
          working-directory: src-tauri

  frontend:
    name: bun audit
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - run: bun install --frozen-lockfile
      - run: bun audit --audit-level=high
```

Catatan:

- Kalau `rustsec/audit-check@v2` tidak menerima input `working-directory`,
  **jangan menebak**: laporkan ke pengguna. (Alternatifnya menjalankan
  `cargo audit` manual, tapi itu keputusan pengguna.)
- Validasi YAML: `bunx --bun yaml-lint` tidak ada; cukup pastikan
  `python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]" .github/dependabot.yml .github/workflows/audit.yml`
  tidak error.
- Jangan jalankan `bun audit` lalu memperbaiki temuan. Cukup laporkan
  hasilnya.
- Commit: `ci: add dependabot and dependency audit`.

---

## 9. Gerbang sebelum SETIAP commit

Dari `src-tauri/`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --test '*'
```

Dari root:

```bash
bun run format:check   # setelah T4 ada
bun run build
bun run test
```

Tugas yang hanya menyentuh sisi frontend (T4, T6) tetap menjalankan
`cargo fmt --check` saja untuk sisi Rust. Kalau satu gerbang gagal karena
sebab di luar perubahanmu (mis. PDFium belum di-fetch), **laporkan**, jangan
menonaktifkan test.

## 10. Laporan akhir

Setelah semua tugas selesai (T1-T16), laporkan:

1. Daftar commit (hash + judul).
2. Hasil tiap perintah di §9 pada commit terakhir (lulus/gagal + ringkasan).
3. Hasil `bun audit` dan hal apa pun yang kamu lewati atau tidak cocok
   dengan prompt ini.
4. Konfirmasi bahwa tidak ada file di luar daftar "boleh disentuh" yang
   berubah (`git diff --stat main..HEAD`).
