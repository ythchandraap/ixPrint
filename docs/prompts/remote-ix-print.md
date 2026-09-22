# Prompt implementasi: Printer dari ix.print lain (Opsi 2), tetap kompatibel dengan Opsi 1

> Berikan seluruh isi file ini ke AI coding agent yang bekerja di repo ini.
> Semua nama file, struct, fungsi, endpoint, dan perilaku di bawah diambil dari
> kode saat prompt ini ditulis. Kalau ada yang tidak cocok dengan kode yang kamu
> baca, **berhenti dan laporkan perbedaannya**, jangan menyesuaikan diam-diam.

---

## 0. Aturan kerja (wajib dibaca dulu)

1. **Jangan mengarang.** Setiap nama fungsi/struct/field yang kamu pakai harus
   sudah ada di kode atau didefinisikan di prompt ini. Kalau butuh API crate
   (mis. `ureq`), buka dulu sumbernya di
   `~/.cargo/registry/src/*/ureq-3.*/src/` dan pakai signature yang benar-benar
   ada di sana. Jangan menebak API dari ingatan.
2. **Baca dulu, baru ubah.** Sebelum mengedit file apa pun, baca file itu
   utuh. Minimal baca: `src-tauri/src/job.rs`, `server.rs`, `callback.rs`,
   `history.rs`, `config.rs`, `commands.rs`, `lib.rs`, `printer/mod.rs`,
   `settings.rs` (bagian `PrintSettings`), `src/api.ts`,
   `src/components/PrintersTab.vue`, `src/components/SettingsView.vue`,
   `src/components/HistoryTab.vue`.
3. **Working tree berisi banyak perubahan yang belum di-commit dari pemilik
   repo.** Jangan revert, format ulang, atau "merapikan" kode yang tidak
   berhubungan dengan tugas ini. Jangan commit kecuali diminta.
4. **Ikuti gaya kode sekitar**: komentar singkat bahasa Inggris, pesan error
   dan teks UI **bahasa Inggris** (UI saat ini sudah bahasa Inggris), idiom
   yang sama (`web::block`, `blocking(...)`, `state.config.update(...)`, dst).
5. Kalau ada keputusan yang **tidak** tercakup di prompt ini, tanyakan ke
   pengguna. Jangan memilih sendiri.
6. Selesai berarti: `cd src-tauri && cargo test --lib` lulus tanpa warning
   baru, `bun run build` (di root) lulus, dan semua item di §11 tercentang.

---

## 1. Latar belakang

ix.print adalah aplikasi desktop Tauri 2 (Rust + Vue 3) yang menjalankan HTTP
API di port `18032` (`server::PORT`). Web app mengirim PDF atau data ke
`POST /print`, lalu ix.print mencetaknya ke printer lokal.

Masalah yang diselesaikan: web app di cloud (halaman HTTPS) yang dibuka di
**PC1** hanya bisa memanggil `http://127.0.0.1:18032` (localhost dikecualikan
dari blokir *mixed content*). Printer tujuannya ada di **PC2**, yang juga
menjalankan ix.print. Browser tidak bisa memanggil `http://<IP PC2>:18032`
dari halaman HTTPS.

Ada dua cara yang harus **bisa berjalan berdampingan**:

- **Opsi 1 (sudah bisa, jangan dirusak):** printer PC2 di-share di level OS
  lalu ditambahkan di PC1, misalnya lewat Windows sharing, IPP/CUPS, atau form
  printer ix.print (`printer::manage::Connection::Network`). Printer ini
  muncul di `printer::list_printers()` PC1 sebagai **printer lokal biasa**, dan
  ix.print PC1 mencetak ke sana lewat jalur lokal yang sudah ada.
- **Opsi 2 (yang kamu bangun):** ix.print PC1 menyimpan koneksi ke ix.print
  PC2 (URL + token API PC2). Printer PC2 muncul di PC1 dengan nama
  `"<nama remote> / <nama printer di PC2>"`. Job ke printer itu **diteruskan
  server-ke-server** ke API ix.print PC2. Status job dan callback tetap
  berjalan.

```
Opsi 1: Browser PC1 ─127.0.0.1─▶ ix.print PC1 ─(printer share OS)─▶ PC2 ─▶ Printer
Opsi 2: Browser PC1 ─127.0.0.1─▶ ix.print PC1 ─HTTP LAN + token─▶ ix.print PC2 ─▶ Printer
```

---

## 2. Istilah (pakai persis)

| Istilah | Arti |
|---|---|
| **remote** | Satu koneksi tersimpan dari PC1 ke ix.print lain. Struct `Remote`. |
| **nama remote** | Label pendek pilihan pengguna, misalnya `PC2` atau `Gudang`. |
| **printer remote** | Printer milik remote, tampil di PC1 sebagai `"<nama remote> / <nama printer>"`. |
| **printer lokal** | Printer dari `printer::list_printers()` di mesin ini, termasuk printer share OS (Opsi 1). |
| **SEPARATOR** | Konstanta `pub const SEPARATOR: &str = " / ";` (spasi, garis miring, spasi). |
| **hop header** | Header HTTP `X-IxPrint-Hop: 1` yang selalu dikirim PC1 ke PC2. |

---

## 3. Batasan keras (tidak boleh dilanggar)

1. **Printer lokal selalu menang.** Kalau sebuah nama cocok persis dengan
   printer lokal, itu printer lokal, walaupun namanya kebetulan berbentuk
   `"X / Y"` dan ada remote bernama `X`. Jalur cetak lokal di `job::run`
   (layout PDFium, `print_ready`, `check_paper_fits`, dst) **tidak berubah
   perilakunya sama sekali**. Ini yang menjamin Opsi 1 tetap jalan.
2. **Tidak ada rantai.** PC1 hanya meneruskan satu lompatan. Request yang
   membawa hop header:
   - `GET /list-printer` → hanya printer lokal (tanpa printer remote);
   - `POST /print`, `POST /test-page`, `POST /print-template` yang menuju
     printer remote → `400` dengan pesan
     `"Forwarding between ix.print computers is limited to one hop"`;
   - `GET /queue`, `GET /printer-capabilities` untuk printer remote → `400`
     dengan pesan yang sama.
3. **Token remote tidak pernah dikirim ke webview.** Tidak lewat `get_config`,
   tidak lewat command lain, tidak masuk log, tidak masuk riwayat.
4. **Tidak meneruskan `callback_url`/`callback_ref` ke PC2.** Callback selalu
   ditangani PC1 (lihat §7.5).
5. **Tidak ada fitur baru di luar prompt ini**: tidak ada auto-discovery, mDNS,
   sinkronisasi preset, retry antrean offline, atau HTTPS server. Kalau
   menurutmu perlu, tanyakan dulu.
6. **Tidak menambah dependensi baru** selain yang sudah ada di
   `src-tauri/Cargo.toml` (`ureq = "3"` sudah ada; `uuid`, `serde_json`,
   `chrono`, `sanitize-filename` juga sudah ada). Kalau benar-benar perlu
   crate baru, tanyakan dulu.

---

## 4. Model data

### 4.1 File penyimpanan

Remote disimpan di **file terpisah** `remotes.json` di folder data aplikasi
(`app.path().app_data_dir()`, folder yang sama dengan `config.json`), **bukan**
di `Config`. Alasannya, `Config` dikirim utuh ke webview oleh banyak command,
sedangkan token remote tidak boleh ikut terkirim.

### 4.2 Modul baru `src-tauri/src/remote.rs`

Daftarkan di `lib.rs` (`mod remote;`). Isi minimal dengan signature berikut.
Boleh menambah fungsi privat pendukung, tapi jangan mengganti nama yang
publik.

```rust
pub const SEPARATOR: &str = " / ";
pub const HOP_HEADER: &str = "X-IxPrint-Hop";
const MAX_REMOTES: usize = 20;
const MAX_NAME: usize = 32;

/// Stored in remotes.json only. Never serialized to the webview.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Remote {
    pub id: String,          // uuid v4
    pub name: String,        // label, see validate_name
    pub url: String,         // normalized base URL, see normalize_url
    pub token: String,       // PC2's API token (plain, e.g. "fxp_…")
    /// Preset on the other computer to layer under the settings sent (None = its default preset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub created_at: String,  // RFC 3339, chrono::Local::now().to_rfc3339()
}

/// What the window sees.
#[derive(Serialize, Clone, Debug)]
pub struct RemoteView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub preset: Option<String>,
    /// First 10 characters of the token + "…", like auth::ApiToken::hint.
    pub token_hint: String,
    pub created_at: String,
    /// Result of the last /list-printer call: None = never tried yet.
    pub online: Option<bool>,
    pub last_error: Option<String>,
    pub printers: usize,
}

pub struct RemoteStore { /* path: PathBuf, data: RwLock<Vec<Remote>> */ }
impl RemoteStore {
    pub fn load(dir: &Path) -> RemoteStore;           // missing/corrupt file → empty list (log with eprintln!)
    pub fn list(&self) -> Vec<Remote>;
    pub fn update<F: FnOnce(&mut Vec<Remote>) -> Result<(), String>>(&self, f: F) -> Result<Vec<Remote>, String>;
}

pub fn validate_name(name: &str) -> Result<(), String>;
pub fn normalize_url(url: &str) -> Result<String, String>;

/// Where a printer name points. Pure: takes the local names, no I/O.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Local(String),
    Remote { remote: Remote, printer: String },
    Unknown,
}
pub fn resolve(local: &[String], remotes: &[Remote], name: &str) -> Target;
```

Aturan detail:

- **`RemoteStore::update`** mengikuti pola `ConfigStore::update` di
  `config.rs`: salin, ubah, validasi seluruh daftar, tulis atomik
  (`.tmp` lalu rename), baru ganti data di memori. Di Unix set permission file
  `0o600`, sama seperti `write_atomic` di `config.rs`. Validasi daftar:
  maksimal `MAX_REMOTES`; `name` unik tanpa membedakan huruf besar/kecil;
  setiap `name`, `url`, dan `token` valid.
- **`validate_name`**: dipotong spasi (trim) lalu harus sama dengan aslinya,
  panjang 1–`MAX_NAME` karakter, tidak mengandung `/`, tidak mengandung
  karakter kontrol. Pesan error bahasa Inggris, contoh:
  `"Remote name must be 1–32 characters without '/'"`.
- **`normalize_url`**: trim, buang `/` di akhir, lowercase skema dan host.
  Wajib diawali `http://` atau `https://`. Tidak boleh ada path, query,
  fragment, atau `user@`. Port opsional. Kalau port tidak ada dan skemanya
  `http`, tambahkan `:18032`. Contoh:
  - `"http://192.168.1.7"` → `"http://192.168.1.7:18032"`
  - `"HTTP://PC2.local:18032/"` → `"http://pc2.local:18032"`
  - `"https://print.contoh.id"` → `"https://print.contoh.id"` (tidak ditambah port)
  - ditolak: `"192.168.1.7"`, `"http://a/b"`, `"http://u:p@a"`, `"ftp://a"`.
- **Token**: trim, harus diawali `auth::TOKEN_PREFIX` (`"fxp_"`), panjang
  maksimal 200.
- **`resolve`**:
  1. Kalau `local` berisi `name` persis → `Target::Local(name)`.
  2. Kalau tidak, cari remote `r` dengan `name.starts_with(&format!("{}{}", r.name, SEPARATOR))`
     (pembandingan nama remote **peka huruf besar/kecil**) dan sisa nama tidak
     kosong → `Target::Remote { remote: r.clone(), printer: sisa }`.
  3. Selain itu → `Target::Unknown`.

### 4.3 Cache status remote

Tambahkan ke `job::AppState` field:

```rust
pub remotes: crate::remote::RemoteStore,
/// Last /list-printer answer per remote id; see remote::printers.
pub remote_cache: crate::remote::RemoteCache,
```

`RemoteCache` = `Mutex<HashMap<String, CacheEntry>>` dengan `CacheEntry {
fetched: Instant, printers: Vec<printer::Printer>, error: Option<String>, ok_once: bool }`.
Perbarui **semua** tempat yang membangun `AppState`: `lib.rs`, `server.rs`
(`mod tests::state`), dan `callback.rs` (test `delivers_signed_result_of_failed_job`).
Cari dengan `grep -rn "failed_auth: Default::default()" src-tauri/src`.

---

## 5. Klien HTTP ke remote (di `remote.rs`)

Semua request ke PC2 memakai `ureq` 3 dengan agent yang dibuat seperti di
`callback::deliver` (`ureq::Agent::config_builder()`,
`.http_status_as_error(false)`, `.max_redirects(0)`, dan
`.max_redirects_will_error(false)`). Periksa nama method-nya di sumber ureq
sebelum dipakai. Setiap request **wajib** membawa:

- `Authorization: Bearer <remote.token>`
- `X-IxPrint-Hop: 1`
- `User-Agent: ix.print/<CARGO_PKG_VERSION>` (via `.user_agent(...)` pada config)

Timeout global per jenis request:

| Fungsi | Request ke PC2 | Timeout |
|---|---|---|
| `fetch_printers(&Remote)` | `GET {url}/list-printer` | 3 dtk |
| `capabilities(&Remote, printer)` | `GET {url}/printer-capabilities?printer=` | 5 dtk |
| `queue(&Remote, printer)` | `GET {url}/queue?printer=` | 5 dtk |
| `job_status(&Remote, remote_job_id)` | `GET {url}/job-status?job_id=` | 5 dtk |
| `print_pdf(&Remote, pdf, file_name, settings)` | `POST {url}/print?…` multipart | 120 dtk |
| `test_page(&Remote, pages, settings)` | `POST {url}/test-page?count=…&…` | 60 dtk |

Pemetaan respons (pakai **persis** ini). `RemoteError` adalah enum baru:
`BadRequest(String)` atau `Failed(String)`.

| Respons PC2 | Hasil di PC1 |
|---|---|
| 2xx + JSON valid | `Ok(...)` |
| 400 / 404 | `RemoteError::BadRequest(format!("{}: {}", remote.name, message))`, dengan `message` = field `message` dari JSON PC2 (atau `"HTTP <code>"` kalau tidak ada) |
| 401 | `RemoteError::Failed(format!("{}: token rejected — create a new token on that computer", remote.name))` |
| 429 | `RemoteError::Failed(format!("{}: too many wrong tokens, try again later", remote.name))` |
| 5xx lainnya | `RemoteError::Failed(format!("{}: {}", remote.name, message))` |
| error koneksi/timeout | `RemoteError::Failed(format!("{} cannot be reached ({}): {}", remote.name, remote.url, err))` |

`RemoteError` dipetakan ke `JobError::BadRequest` / `JobError::Failed` di
`job.rs`.

**Settings ke query string.** `PrintSettings` punya
`#[serde(skip_serializing_if = "Option::is_none")]` di setiap field, dan tipe
seperti `Paper`/`Margins` diserialisasi ke bentuk string (macro
`string_serde!`). Ubah ke query dengan: `serde_json::to_value(&settings)` → untuk
setiap pasangan di object: `String` apa adanya, `Number` → `to_string()`,
`Bool` → `"true"`/`"false"`; lewati `null`. Kirim dengan `.query(key, value)`
dari ureq (sudah diverifikasi ada: `RequestBuilder::query` di
`ureq-3.4.2/src/request.rs`). Hasilnya harus bisa dibaca ulang oleh
`PrintSettings::from_query` di `settings.rs`. Buat unit test round-trip untuk
ini (§10).

**Multipart manual** (ureq 3 tidak dipakai untuk multipart di repo ini):
boundary `format!("ixprint-{}", uuid::Uuid::new_v4().simple())`; body:

```
--{b}\r\n
Content-Disposition: form-data; name="file"; filename="{sanitize_filename::sanitize(file_name)}"\r\n
Content-Type: application/pdf\r\n
\r\n
{bytes PDF}\r\n
--{b}--\r\n
```

Header `Content-Type: multipart/form-data; boundary={b}`. Tolak kalau file
lebih besar dari `server::MAX_UPLOAD_BYTES`. Jadikan konstanta itu `pub(crate)`
kalau perlu, tapi jangan ubah nilainya.

**Daftar printer + cache** — `pub fn printers(state: &AppState, remote: &Remote) -> (Vec<printer::Printer>, Option<String>)`:

- Kalau entri cache lebih muda dari **5 detik**, kembalikan isi cache.
- Kalau tidak, panggil `fetch_printers`. JSON PC2 adalah array
  `PrinterView` (`name, status, message, shared, os_default, is_default`, dan
  setelah fitur ini juga `remote`). **Buang entri yang punya field `remote`
  tidak null**, sebagai pengaman kedua terhadap rantai. Petakan ke
  `printer::Printer { name: format!("{}{}{}", remote.name, SEPARATOR, p.name), status: p.status, message: p.message, shared: p.shared, os_default: false }`.
- Kalau gagal dan pernah berhasil (`ok_once`), kembalikan printer terakhir
  dengan `status = "Offline"` dan `message = Some(error)`. Kalau belum pernah
  berhasil, kembalikan list kosong + error.
- Satu remote yang lambat tidak boleh menunda remote lain lebih dari
  timeout-nya. Ambil semua remote paralel (satu thread per remote, join).

---

## 6. Perubahan di backend yang sudah ada

### 6.1 `server.rs`

- `PrinterView` mendapat field
  `#[serde(skip_serializing_if = "Option::is_none")] pub remote: Option<String>`
  (nama remote).
- `printer_views(state)` menjadi: printer lokal (seperti sekarang,
  `remote: None`) **ditambah** printer remote dari `remote::printers` untuk
  setiap remote (`remote: Some(r.name)`), kecuali kalau request membawa hop
  header. Karena `printer_views` juga dipanggil command Tauri `get_printers`,
  beri parameter `include_remote: bool`: `true` dari window, dan dari HTTP
  `true` hanya kalau hop header **tidak** ada. `is_default` dihitung dengan
  aturan yang sama untuk semua printer.
- `fn installed(name)` (dipakai `/queue` dan `/printer-capabilities`) diganti
  dengan resolusi `remote::resolve`:
  - `Local` → perilaku sekarang;
  - `Remote` → kalau ada hop header: 400 (§3.2); kalau tidak, teruskan ke
    `remote::queue` / `remote::capabilities` dan kembalikan JSON PC2 apa
    adanya. Untuk `/queue`, ganti field `printer` dengan nama versi PC1;
  - `Unknown` → 404 `"Printer '<name>' was not found on this computer"`
    (pesan ini sudah ada di `job.rs`; samakan).
- Handler cetak meneruskan info hop ke `job::run` lewat field baru
  `JobRequest.hop: bool` (`true` kalau header `X-IxPrint-Hop` ada). Command
  Tauri (`test_print`) mengisi `false`.
- `job_status_json` → lihat §7.4.

### 6.2 `job.rs` — `run`

Tepat setelah `printer_name` ditentukan (kode saat ini:
`merged.printer.clone().or_else(|| state.default_printer())…`), ganti blok
`if !printer::list_printers().iter().any(...)` dengan:

```text
local names = printer::list_printers() names
match remote::resolve(&local, &state.remotes.list(), &printer_name) {
    Local(_)  => lanjut jalur lokal yang sudah ada, TANPA perubahan
    Unknown   => Err(BadRequest("Printer '<name>' was not found on this computer"))   // pesan sama seperti sekarang
    Remote{..} if req.hop => Err(BadRequest("Forwarding between ix.print computers is limited to one hop"))
    Remote{remote, printer} => return forward(...)   // §7
}
```

`record.printer` tetap berisi nama versi PC1 (`"PC2 / EPSON"`). Penulisan
riwayat di akhir `run` (`state.history.append(&record)`) harus tetap terjadi
untuk job remote, baik berhasil maupun gagal. Jadi `forward` dijalankan di
dalam closure `result` yang sama, bukan di luarnya.

### 6.3 `history.rs`

Tambahkan ke `HistoryEntry`:

```rust
/// Set when the job was forwarded to another ix.print.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub remote: Option<RemoteJob>,
```

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoteJob {
    pub remote_id: String,
    pub remote_name: String,
    /// job_id on the other computer.
    pub job_id: String,
}
```

Perbarui semua konstruksi `HistoryEntry` (`job.rs`, test di `history.rs`,
test di `callback.rs`) dengan `remote: None`. Cari dengan
`grep -rn "callback: None" src-tauri/src`.

### 6.4 `commands.rs` + `lib.rs`

Command baru (daftarkan di `tauri::generate_handler![…]` di `lib.rs`):

| Command | Argumen | Hasil | Perilaku |
|---|---|---|---|
| `remote_list` | – | `Vec<RemoteView>` | Dari store + cache (`online`/`last_error`/`printers` diambil dari cache; `None` kalau belum pernah dicoba). |
| `remote_add` | `name: String, url: String, token: String, preset: Option<String>` | `Vec<RemoteView>` | Normalisasi & validasi → **tes koneksi dulu** (`fetch_printers`). Kalau gagal, kembalikan `Err` dengan pesan §5 dan **jangan simpan**. Kalau berhasil, simpan, isi cache, lalu kembalikan list. |
| `remote_update` | `id: String, name: String, url: String, token: Option<String>, preset: Option<String>` | `Vec<RemoteView>` | `token: None` = token lama dipakai. Tes koneksi seperti `remote_add`. Kalau `name` berubah, ganti prefix `"<lama> / "` → `"<baru> / "` di `config.default_printer` dan `presets[*].settings.printer` (pola sama seperti rename di `printer_update`). |
| `remote_remove` | `id: String` | `Vec<RemoteView>` | Hapus remote dan entri cache-nya. `default_printer` yang menunjuk ke remote itu dikosongkan (pola `printer_remove`). Preset **tidak** diubah. |
| `remote_test` | `id: String` | `RemoteView` | Paksa refresh cache (abaikan TTL). |

Semua yang melakukan I/O jaringan dijalankan lewat helper `blocking(...)`
yang sudah ada.

Command yang sudah ada dan **harus** paham printer remote:

- `get_capabilities`, `get_queue` → pakai resolusi yang sama seperti §6.1
  (tanpa hop).
- `printer_details`, `printer_update`, `printer_remove`,
  `printer_set_os_default` → kalau nama resolve ke `Remote`, kembalikan
  `Err("Printers of another ix.print are managed on that computer")`.

---

## 7. Meneruskan job (`remote.rs::forward`, dipanggil dari `job::run`)

### 7.1 Settings yang dikirim

- `merged` = `preset.settings.overlay(&req.settings)`. Ini **sudah** dihitung
  di `job::run`, jangan dihitung ulang dengan cara lain.
- Kirim `merged` **dengan `printer` diganti** nama printer di PC2 (bagian
  setelah SEPARATOR).
- Kirim `preset = remote.preset` sebagai query `preset` hanya kalau `Some`.
  Artinya: field yang tidak diisi di PC1 diambil dari preset PC2 (atau
  default preset PC2 kalau `remote.preset` kosong). Di sinilah kalibrasi
  offset PC2 ikut terpakai.
- **Jangan** lakukan layout PDFium, `check_paper_fits`, atau
  `printer::default_paper_mm` untuk job remote. PC2 yang melakukannya.

### 7.2 Per jenis dokumen (`job::Document`)

| Dokumen | Yang dikirim ke PC2 |
|---|---|
| `File(path)` | `print_pdf` dengan file itu dan `req.file_name`. |
| `Data { records, header }` | Render di PC1 dengan `crate::template::render(&generated, template, records, header)`, sama persis seperti jalur lokal (termasuk cek `MAX_RECORDS` dan error "tidak punya desain"). Lalu `print_pdf` dengan settings `merged` **ditimpa**: `paper = design_paper(template)`, `scale = Scale::None`, `margins = Margins::default()`, `align = Align::Center`, `orientation = Orientation::Portrait`, `pages = None`. Nilainya sama dengan yang dipasang jalur lokal. |
| `TestPage { pages }` + preset punya desain | Render sampel di PC1 persis seperti jalur lokal (`vec![design.sample.clone(); pages.clamp(1, MAX_TEST_PAGES)]`), lalu `print_pdf` dengan override yang sama seperti baris di atas dan `copies = 1`. |
| `TestPage { pages }` tanpa desain | `test_page` ke PC2 dengan `count = pages` dan settings `merged`. |

`design_paper` saat ini adalah fungsi privat di `job.rs`. Ubah menjadi
`pub(crate)` dan panggil dari `remote.rs`, **jangan** menyalin logikanya.
Begitu juga `testpage::MAX_TEST_PAGES` (sudah `pub`) dan
`crate::template::render` (sudah `pub`).

File sementara di `state.jobs_dir` wajib dihapus setelah dikirim, termasuk
saat gagal. Ikuti pola `generated` + `remove_file` di `job::run`.

### 7.3 Hasil

Respons sukses PC2 berbentuk
`{message, job_id, printer, preset, sheets, spooler_job, status}` (lihat
`server::submit`). Isi:

- `record.sheets = sheets`, `record.spooler_job = None`,
  `record.remote = Some(RemoteJob { remote_id, remote_name, job_id: <job_id PC2> })`.
- `JobResult { job_id: <id PC1>, printer: <nama versi PC1>, preset: preset_name (PC1), sheets, spooler_job: None }`.

### 7.4 Status job (`server::job_status_json`)

Kalau `entry.remote` ada:

1. Cari remote berdasarkan `remote_id` di `state.remotes`. Kalau sudah
   dihapus: `{"state":"unknown","message":"Remote '<remote_name>' was removed"}`.
2. Panggil `remote::job_status(remote, job_id_pc2)`. Kalau berhasil, pakai
   JSON PC2 dan **timpa** `job_id` (id PC1) dan `printer` (nama versi PC1).
   Pertahankan field lainnya (`state`, `position`, `ahead`, `message`,
   `printer_status`, `printer_message`, `spooler_job`).
3. Kalau gagal:
   `{"state":"unknown","message":"<pesan RemoteError>","remote_unreachable":true}`.
4. Selalu tambahkan `"remote": "<remote_name>"` di hasil.

Kalau `entry.success == false`, perilakunya tetap seperti sekarang
(`"failed"`), tanpa memanggil PC2.

### 7.5 Callback (`callback.rs`)

Tidak perlu jalur baru, karena `callback::watch` memakai `job_status_json`.
**Satu perubahan wajib:** aturan yang saat ini berbunyi

```rust
"unknown" if status.get("spooler_job").is_some_and(|v| v.is_null()) => … "sent"
```

hanya berlaku kalau `status.get("remote").is_none()`. Untuk job remote,
`"unknown"` artinya "belum tahu / PC2 tidak terjangkau sementara", jadi
teruskan polling sampai `GIVE_UP_AFTER`.

---

## 8. Frontend (Vue 3, `src/`)

Teks UI dalam **bahasa Inggris**, mengikuti gaya komponen yang ada. Ikon dari
`lucide-vue-next` (sudah dipakai). Jangan menambah library.

### 8.1 `src/api.ts`

- `PrinterView`: tambahkan `remote?: string;` dengan komentar
  `/** Name of the other ix.print this printer belongs to. */`.
- `HistoryEntry`: tambahkan
  `remote?: { remote_id: string; remote_name: string; job_id: string };`.
- Tipe baru `RemoteView`, sama persis dengan struct Rust §4.2.
- Di objek `api`: `remotes`, `remoteAdd`, `remoteUpdate`, `remoteRemove`,
  `remoteTest`. Nama argumen harus camelCase sesuai konvensi Tauri
  (`{ name, url, token, preset }`, `{ id }`). Lihat contoh `createToken`,
  `printerUpdate` yang sudah ada.

### 8.2 Komponen baru `src/components/RemotesPanel.vue`

Ditampilkan di `SettingsView.vue`, **di bawah** `ApiTab` dengan judul
`<h3 class="section-title">Other ix.print computers</h3>`. Isi:

- Penjelasan satu paragraf (`class="hint"`): printer di komputer lain yang
  menjalankan ix.print bisa dipakai dari sini. Buat token di komputer itu
  (Settings → API), lalu tempelkan di sini.
- Daftar remote (`class="card row"`):
  - nama;
  - URL;
  - `token_hint`;
  - preset;
  - status: `online === true` → "Online · N printers", `false` → teks
    `last_error` dengan `class="error small-text"`, `null` → "Not checked yet";
  - tombol ikon **Test** (`RefreshCw`), **Edit** (`Pencil`), **Remove**
    (`Trash2`, konfirmasi lewat `confirm` dari `@tauri-apps/plugin-dialog`,
    seperti `ApiTab.revoke`).
- Form tambah/edit: `Name`, `Address` (placeholder
  `http://192.168.1.7:18032`), `Token` (input `type="password"`,
  `autocomplete="off"`; saat edit boleh kosong = token tidak diganti, dengan
  hint "Leave empty to keep the current token"), `Preset on that computer`
  (opsional, teks bebas). Tombol submit menampilkan "Connecting…" selama
  proses.
- Error dari command ditampilkan lewat `emit("notify", errorText(e), true)`,
  pola yang sama dengan `ApiTab.vue`.

`SettingsView.vue` meneruskan event `notify` seperti untuk `ApiTab`. Setelah
remote berubah, minta `App.vue` me-refresh printer dengan emit baru
`printers-changed` dari `SettingsView`. Di `App.vue`, tangani event itu dengan
`refreshPrinters()`.

### 8.3 `PrintersTab.vue`

- Kalau `printer.remote` ada, tampilkan badge `<span class="badge subtle">via {{ printer.remote }}</span>`.
- Sembunyikan tombol **Edit / remove printer** (`Pencil`) untuk printer
  remote. Bintang default dan tombol salin tetap tampil.
- Panel antrean tetap memakai `api.queue(name)`. Backend yang meneruskan ke
  PC2.

### 8.4 `HistoryTab.vue`

Di baris meta entri, kalau `e.remote` ada, tambahkan
` · via {{ e.remote.remote_name }}`. Saat entri dibuka (expanded), tampilkan
`Job on {{ e.remote.remote_name }}: <code>{{ e.remote.job_id }}</code>`.

---

## 9. Keamanan

- `remotes.json`: permission `0o600` di Unix. Folder data sudah `0o700` (lihat
  `lib.rs`).
- Token remote tidak boleh muncul di: webview (hanya `token_hint`), `eprintln!`,
  pesan error, riwayat, atau respons HTTP.
- `max_redirects(0)` di semua request ke remote. Token tidak boleh ikut
  terbawa ke host lain.
- Di sisi PC2 tidak ada pengecualian keamanan baru. PC1 adalah klien API biasa
  dengan token, tanpa header `Origin`, sehingga CORS/allowlist origin tidak
  berlaku dan lockout token salah (`auth::FailedAttempts`) tetap berlaku.
- Tambahkan di `docs.html` dan `README.md`: lalu lintas PC1→PC2 lewat HTTP
  biasa di LAN, dan token terlihat oleh siapa pun yang bisa menyadap LAN itu.
  Gunakan jaringan tepercaya, atau URL `https://` kalau PC2 ada di belakang
  reverse proxy TLS.

---

## 10. Test yang wajib ada

Semua di `cargo test --lib`, tanpa jaringan eksternal dan tanpa printer
sungguhan.

**Unit (`remote.rs`)**

1. `normalize_url`: semua contoh valid dan tidak valid di §4.2.
2. `validate_name`: kosong, spasi di tepi, `/`, lebih dari 32 karakter →
   error; `"PC2"`, `"Gudang Lt 2"` → ok.
3. `resolve`:
   - lokal `["PC2 / EPSON"]` + remote `PC2` → `Local` (**test Opsi 1**);
   - lokal `[]` + remote `PC2` + nama `"PC2 / EPSON"` →
     `Remote { printer: "EPSON" }`;
   - `"PC2 / "` → `Unknown`; `"pc2 / EPSON"` dengan remote `PC2` →
     `Unknown`; `"Other / X"` → `Unknown`.
4. Round-trip settings → query → `PrintSettings::from_query` untuk
   `{"copies":2,"paper":"80mm","offset_x":1.5,"margins":"5,3","collate":false,"scale":"90%"}`
   menghasilkan struct yang sama.
5. `RemoteStore`: simpan, load ulang dari disk, nama duplikat (beda huruf
   besar/kecil) ditolak, lebih dari 20 remote ditolak, file rusak → list
   kosong tanpa panic.

**Integrasi dengan server tiruan.** Pakai `std::net::TcpListener` di
`127.0.0.1:0`, dengan pola yang sama seperti `receive()` di test
`callback.rs`. Server tiruan menjawab dengan JSON kalengan dan **menyimpan
request** untuk diperiksa.

6. `fetch_printers`:
   - request membawa `authorization: bearer fxp_…` dan `x-ixprint-hop: 1`;
   - entri dengan `"remote":"X"` dibuang;
   - nama dipetakan menjadi `"PC2 / …"`.
7. `forward` untuk `Document::File`:
   - request `POST /print` berisi query `printer=EPSON` dan `copies=2`;
   - ada `preset=kalibrasi` kalau `remote.preset` diisi;
   - body multipart berisi byte PDF;
   - `JobResult.printer == "PC2 / EPSON"`;
   - `HistoryEntry.remote.job_id` = job_id dari server tiruan.
8. Pemetaan error: 401 → `JobError::Failed` berisi `"token rejected"`; 400
   `{"message":"Preset 'x' was not found"}` → `JobError::BadRequest` berisi
   `"PC2: Preset 'x' was not found"`; port tertutup → `Failed` berisi
   `"cannot be reached"`.
9. `job_status_json` untuk entri remote: meneruskan `state: "queued"` dari
   server tiruan, menimpa `job_id`/`printer`, dan menambahkan `remote`. Saat
   server tiruan mati, hasilnya `remote_unreachable: true`.
10. Hop:
    - `actix_web::test` dengan `routes`: `GET /list-printer` dengan header
      `X-IxPrint-Hop: 1` → tidak ada entri yang punya `remote`;
    - `POST /print` JSON ke printer remote dengan hop → 400 berisi
      `"one hop"`.
11. `callback.rs`: status `{"state":"unknown","spooler_job":null,"remote":"PC2"}`
    **tidak** menghasilkan payload `sent`. Uji fungsi keputusan yang
    dipisahkan dari loop. Kalau perlu, pecah `match` di `watch` menjadi fungsi
    murni `fn decide(status: &Value) -> Option<(…)>`, tanpa mengubah
    perilakunya untuk job lokal.

Semua test lama harus tetap lulus **tanpa diubah**, kecuali penambahan field
baru di literal struct (`remote: None`, `remotes`, `remote_cache`).

---

## 11. Checklist penerimaan

### Otomatis

- [ ] `cd src-tauri && cargo test --lib` lulus, tanpa warning baru dari
  `cargo build`.
- [ ] `bun run build` lulus (termasuk `vue-tsc`).

### Manual Opsi 1 (harus tetap jalan)

- [ ] Di PC2, share printer lewat OS. Di PC1, tambahkan printer itu lewat
  pengaturan OS atau form printer ix.print (koneksi Network/IPP ke IP PC2).
- [ ] Printer itu muncul di tab Printers PC1 **tanpa** badge "via …", dan
  tombol Edit tetap ada.
- [ ] `POST http://127.0.0.1:18032/print?printer=<nama printer share>` dari
  PC1 tercetak. Riwayat PC1 tidak menampilkan "via".

### Manual Opsi 2

- [ ] Di PC2: tab API → buat token "PC1". Kalau "Require token" mati,
  aktifkan.
- [ ] Di PC1: Settings → Other ix.print computers → tambah `PC2`,
  `http://<IP PC2>`, dan token itu. Status "Online · N printers".
- [ ] Token salah → gagal dengan pesan "token rejected", dan remote tidak
  tersimpan.
- [ ] Tab Printers PC1 menampilkan `PC2 / <printer>` dengan badge
  "via PC2". Tombol Edit tidak ada, antrean tampil.
- [ ] `POST /print` (PDF) ke PC1 dengan `printer=PC2 / <printer>` tercetak di
  PC2. Riwayat PC1 menampilkan "via PC2", riwayat PC2 mencatat klien = IP PC1
  dan token "PC1".
- [ ] Cetak data JSON ke preset berdesain di PC1 dengan printer remote →
  tercetak sesuai desain.
- [ ] Test print dari window PC1 ke printer remote → tercetak.
- [ ] `GET /job-status?job_id=<id dari PC1>` mengikuti status di PC2 sampai
  `completed`.
- [ ] Dengan `callback_url`: callback datang dari **PC1** dengan `success`
  yang benar.
- [ ] Matikan ix.print PC2:
  - printer remote tetap tampil sebagai "Offline" dengan pesan;
  - cetak gagal dengan "cannot be reached";
  - job-status yang sedang berjalan menjadi `unknown` +
    `remote_unreachable`;
  - callback tidak mengirim `sent`.
- [ ] Tambahkan PC1 sebagai remote di PC2, sehingga keduanya saling menunjuk:
  - `GET /list-printer` di keduanya tidak berputar;
  - daftar di PC1 hanya berisi printer lokal PC2;
  - mencetak ke `PC2 / PC1 / X` dari PC1 → 400 "one hop" dari PC2.
- [ ] Rename remote `PC2` → `Gudang`: default printer dan preset PC1 ikut
  berubah menjadi `Gudang / …`.
- [ ] `get_config` / DevTools tidak memperlihatkan token remote di mana pun.

### Dokumentasi

- [ ] `README.md`: subbagian "Printers of another ix.print" (cara pakai,
  aturan nama `"<remote> / <printer>"`, printer lokal menang, satu lompatan,
  catatan keamanan LAN).
- [ ] `src-tauri/src/docs.html`: di bagian `/list-printer` sebutkan field
  `remote`, dan di Pengantar satu kalimat tentang meneruskan job.

---

## 12. Yang harus kamu laporkan saat selesai

1. Daftar file yang diubah atau dibuat, masing-masing satu baris alasan.
2. Output ringkas `cargo test --lib` dan `bun run build`.
3. Setiap penyimpangan dari prompt ini, beserta alasannya. Kalau tidak ada,
   tulis "tidak ada".
4. Item checklist manual §11 yang belum bisa kamu uji sendiri (misalnya
   butuh dua komputer dan printer). Tulis terus terang, jangan klaim sudah
   diuji.
