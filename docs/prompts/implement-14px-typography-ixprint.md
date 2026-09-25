# Prompt implementasi tipografi dasar 14px pada ix.print

Salin seluruh isi file ini ke AI coding agent pada repo `fxPrint`. Kerjakan langkah 0–6 berurutan. Ini adalah spesifikasi implementasi, bukan permintaan untuk mencari atau memilih desain baru.

## 0. Kontrak dan keputusan yang sudah ditetapkan

Tujuan: menjadikan **14px / 21px** ukuran teks dasar UI ix.print, lalu menyesuaikan semua peran teks yang relevan tanpa merusak keterbacaan, target sentuh, atau alur kerja. Perubahan ini mencakup aplikasi Vue dan halaman API `/docs`. Perubahan ini **hanya** mengenai tipografi dan penyesuaian layout minimal yang langsung diperlukan karena ukuran teks.

1. **Jangan mencari jawaban sendiri.** Jangan browsing, mencari referensi desain lain, meminta AI lain memilih ukuran, membuat skala tipografi alternatif, atau mengganti keputusan di bawah. Baca `AGENTS.md`, `docs/BRAND.md` (terutama bagian 6, 16, 17, 22), `docs/ARCHITECTURE.md`, file target pada bagian 1, dan file lain hanya jika pemakaian font atau overflow-nya langsung terdampak. Gunakan `rg` terarah untuk mencari `font-size`, shorthand `font`, `line-height`, atau media query. Jangan scan seluruh repo untuk inspirasi.
2. Jika spesifikasi ini tidak menentukan keputusan yang diperlukan, jangan menebak. Sebutkan file dan selector, dua opsi konkret beserta dampaknya, lalu ajukan satu pertanyaan spesifik. Lanjutkan bagian independen sambil menunggu jawaban. Jangan berhenti untuk persetujuan per langkah yang sudah jelas.
3. Font family tetap: **Instrument Sans** untuk UI aplikasi; **IBM Plex Mono** untuk data teknis dan label pendek. Jangan memasang/mengunduh font lain. Stack system/mono yang saat ini dipakai di `/docs` tetap seperti sekarang; hanya ukurannya yang berubah. Jangan menambah endpoint font atau mengubah CSP.
4. Palet Acid Yellow/Obsidian sudah ada di working tree. **Jangan jalankan ulang migrasi warna** dari prompt sebelumnya. Jangan mengubah token warna, status, logo, ikon produk, SVG, favicon, atau aset Tauri. Jangan mengubah API, Rust, business logic, data, copy, komponen, navigasi, atau fitur.
5. Jangan menyamaratakan semua teks menjadi 14px. Ikuti tabel peran di bagian 2. Tidak ada teks informasi yang tampil di bawah 12px. Ukuran target interaksi tetap 40/32/36px pada desktop dan minimal 44px pada perangkat sentuh.
6. Periksa `git status --short` sebelum mengedit. Working tree bisa berisi pekerjaan pengguna. Pakai patch kecil; jangan reset, checkout paksa, reformat seluruh file, menghapus perubahan lain, atau commit kecuali diminta.
7. Jangan mengklaim QA visual selesai jika layar tidak dibuka. Laporkan keterbatasan secara jujur.

## 1. File dalam scope

Utama:

- `src/app/app.css`
- `src/features/design-editor/design-editor.css`
- `src/features/settings/OnboardingDialog.vue`
- `src/features/diagnostics/ErrorLogPanel.vue`
- `src/features/diagnostics/ApiAccessLogPanel.vue`
- `src-tauri/src/interfaces/http/assets/docs.html`

Audit deklarasi ukuran teks pada scoped style Vue lain di `src/features/` dan `src/shared/ui/`. Ubah hanya deklarasi yang melanggar tabel bagian 2 atau menyebabkan kontrol/teks penting tidak terbaca. Jangan menyentuh markup/logic bila CSS cukup. Jangan mengedit `docs/BRAND.md`: panduannya sudah ditetapkan.

## 2. Skala final, tanpa keputusan tambahan

| Peran ix.print | Ukuran | Line-height | Penerapan |
|---|---:|---:|---|
| Judul halaman/dialog utama | 20px | 28px | Judul onboarding, judul utama tampilan |
| Subjudul dokumentasi | 18px | 26px | `h2` pada `/docs` |
| Judul bagian/wordmark produk | 16px | 24px | `.section-title`, heading dialog, `ix.print` app bar, `h3` `/docs` |
| Teks dasar | 14px | 21px | Body aplikasi dan `/docs`, tab, tombol, kontrol, pilihan, baris daftar, pesan error dan toast |
| Label/petunjuk sekunder singkat | 13px | 19px | Field label, helper text yang bukan pesan kesalahan |
| Metadata dan data teknis padat | 12px | 18px | Badge, statusline mono, timestamp, kode aplikasi, ruler, indeks layer |

Pengecualian yang **wajib**:

- `input`, `select`, `textarea`, dan field editable lain: **14px / 21px pada desktop**, **minimal 16px / 24px saat `(any-pointer: coarse)`**. Ini menghindari zoom otomatis browser mobile dan menjaga nilai input terbaca. Tinggi kontrol/touch target **tidak** ikut mengecil.
- Paragraf instruksi panjang dalam onboarding dan paragraf isi utama `/docs`: **14px / 21px desktop**, **16px / 24px pada `(any-pointer: coarse)`**. Lead onboarding yang sudah 16px tetap 16px desktop dan mobile.
- Kode blok/tabel padat `/docs`: **13px / sekitar 19px** boleh dipertahankan supaya contoh API tetap mudah dibaca. Label tombol `Copy` tetap 14px.
- Metadata boleh 12px. Statusline mono yang ringkas juga boleh 12px karena selalu memiliki label teks; pesan error/warning, label tindakan, dan instruksi berikutnya di konten utama harus minimal 14px. Tidak ada teks informasi 10–11px.
- Jangan mengubah `width`, `height`, `min-height`, padding target klik, atau spacing global hanya karena angka font dasar turun. Koreksi lokal boleh dilakukan hanya untuk teks yang terbukti terpotong/bertumpuk.

## 3. Langkah implementasi

### Langkah 0 — Baseline dan inventaris tipografi

1. Baca file instruksi yang disebut bagian 0 dan periksa `git status --short`. Catat file target yang sudah diubah oleh pengguna; jangan mengembalikannya ke keadaan lama.
2. Pastikan primitive dan semantic color di `src/app/app.css` sudah Acid Yellow/Obsidian. Jika belum, **jangan** improvisasi migrasi warna; laporkan ketidaksesuaian dan lanjutkan pekerjaan tipografi yang aman.
3. Jalankan pencarian terarah `rg -n 'font-size|font:|line-height|@media' src/app/app.css src/features src/shared/ui src-tauri/src/interfaces/http/assets/docs.html` dan catat deklarasi 10/11/12/13/14/16/17/21/22px yang berperan dalam UI. Jangan mengubah ukuran ilustrasi, ukuran ikon, atau nilai `font-weight`.
4. Catat baseline build bila tersedia. Jika build sudah gagal sebelum perubahan, simpan error tepatnya agar tidak keliru menganggapnya akibat tipografi.

**Gate:** punya inventaris selector yang perlu berubah dan daftar file target dirty. Lanjutkan ke langkah 1 tanpa meminta persetujuan.

### Langkah 1 — Skala global di `src/app/app.css`

Terapkan nilai berikut secara persis pada selector yang sudah ada:

| Selector | Nilai final |
|---|---|
| `:root` | `font-size: 14px; line-height: 1.5` |
| `.wordmark` | `16px / 24px` |
| `.bar-btn`, `.bar-icon`, `.tabs button`, `button.primary`, `button.ghost`, `button.small` | `14px / 21px` |
| `.statusline`, `.badge`, `.meta`, `.mono`, `code`, `pre.settings`, `.colophon` | `12px / 18px` |
| `.section-title` | `16px / 24px` |
| `.banner`, `.error`, `.toast`, `.check`, `.muted`, `.hint` | `14px / 21px` |
| `.field` | `13px / 19px` |
| `legend` | `14px / 21px` |
| `input:not([type="radio"]):not([type="checkbox"]), select, textarea` | `14px / 21px` desktop |

Catatan implementasi:

1. Gunakan inheritance bila selector sudah mendapat ukuran yang tepat dari `:root`; jangan menambah deklarasi berulang tanpa kebutuhan. Jangan membuat sistem token tipografi baru hanya untuk enam angka ini.
2. `.small-text` tetap 13px hanya untuk penjelasan sekunder; jika digunakan sebagai label aksi atau pesan kesalahan, naikkan selector lokal itu ke 14px.
3. Di media query `(any-pointer: coarse)` yang sudah ada, tambahkan override **16px / 24px** untuk input/select/textarea. Pertahankan nilai ukuran target kontrol saat ini.
4. `.statusline` sekarang `white-space: nowrap` dan berpotensi terpotong ketika menjadi 12px. Bila konten melebihi lebar, pakai `overflow-x: auto; overflow-y: hidden` pada statusline agar alamat/status tetap dapat diakses; jangan menyembunyikan informasi atau menghapusnya. Pertahankan teks dan urutan status.

**Gate:** root 14px, semua kontrol/teks dasar 14px, metadata minimal 12px, input mobile 16px, dan tidak ada perubahan warna atau alur kerja.

### Langkah 2 — Editor, onboarding, dan log

Ubah scoped CSS berikut; nilai yang tidak disebut tetap.

**`src/features/design-editor/design-editor.css`**

- `.ruler-corner`, `.ruler`: shorthand font menjadi **12px IBM Plex Mono** dengan line-height yang terbaca.
- `.band-label` dan `.layer-index`: **12px / 18px**.
- `.segmented button`: **14px / 21px**.
- Textarea editor: **14px / 21px** desktop dan **16px / 24px** pada `(any-pointer: coarse)`; selector lokal tidak boleh mengalahkan override mobile global.
- Jangan ubah ukuran kertas, koordinat, grid, handle, atau hasil cetak. Jika teks ruler/indeks bertabrakan setelah naik ke 12px, perbaiki **hanya** lebar/tinggi area ruler atau indeks yang diperlukan dan catat nilai lama→baru; jangan mengubah satuan desain.

**`src/features/settings/OnboardingDialog.vue`**

- `h1`: **20px / 28px**; `h2`: **16px / 24px**; `.eyebrow`: **12px / 18px**.
- `.onboarding-lead`: tetap **16px / 24px**.
- `.onboarding-section p`, `.onboarding-item span`, `.onboarding-source`, `.onboarding-safe`, `.onboarding-warning`: **14px / 21px** desktop; **16px / 24px** pada `(any-pointer: coarse)` karena isinya instruksi/penjelasan panjang.
- `.onboarding-hint` dan `.onboarding-ack`: **14px / 21px** karena memandu persetujuan; `.onboarding-item strong`: **14px / 21px**.
- Jangan ubah teks disclosure, alur scroll, syarat persetujuan, tombol Continue, atau warna status. Bila paragraf lebih panjang membuat dialog bergulir, pertahankan scroll internal yang sudah ada dan pastikan tombol tetap dapat dicapai.

**`src/features/diagnostics/ErrorLogPanel.vue`**

- `.error-entry-level`, `.detail-header .eyebrow`, `.detail-content pre`: **12px / 18px**.
- `.error-entry-message`, `.detail-section h4`: **14px / 21px**.
- `.detail-header h3`: **16px / 24px**.
- `.settings-group-title h2` tetap **16px / 24px**; teks penjelasan pendek di bawahnya boleh tetap **13px / 19px**. Timestamp/meta tetap 12px/18px melalui global `.meta`.

**`src/features/diagnostics/ApiAccessLogPanel.vue`**

- `.api-method`, `.api-status`: **12px / 18px**. Jangan mengubah nilai HTTP method/status atau warna semantiknya.

**Gate:** tidak ada teks informasi 10–11px pada file target; pesan log dan instruksi tetap terbaca; editor masih berfungsi sama.

### Langkah 3 — Audit scoped styles lain

1. Baca hasil `rg` dari langkah 0, **bukan** seluruh isi setiap komponen. Untuk setiap deklarasi ukuran teks di `src/features/` dan `src/shared/ui/` yang belum disentuh, klasifikasikan berdasarkan tabel bagian 2. Nilai 12, 13, 14, 16 yang sudah sesuai dibiarkan.
2. Bila ada teks 10–11px yang menampilkan informasi, naikkan ke 12px/18px. Bila label tombol/aksi atau pesan error 13px, naikkan ke 14px/21px. Jangan menaikkan semua metadata 13px ke 14px secara otomatis.
3. Bila `font-size: 16px` adalah judul bagian, pertahankan. Bila 16px adalah nilai input desktop, ubah ke 14px dan pastikan override 16px pada pointer kasar berlaku. Bila 17/21px adalah judul, petakan masing-masing ke 16/20px sesuai tabel.
4. Setiap perubahan scoped CSS harus dicatat dengan peran kontennya. Jangan mengubah CSS visual lain kecuali untuk memperbaiki overflow yang muncul langsung akibat ukuran teks.

**Gate:** audit tertulis menunjukkan tidak ada ukuran teks di luar skala yang tersisa tanpa alasan konkret. Tidak ada perubahan pada logic/markup fitur.

### Langkah 4 — Halaman API `/docs`

Edit **hanya CSS** di `src-tauri/src/interfaces/http/assets/docs.html`:

| Selector | Nilai final |
|---|---|
| `body` | `font: 14px/1.5` dengan stack font sekarang |
| `header h1` | `20px / 28px` |
| `h2` | `18px / 26px` |
| `h3`, `.path` | `16px / 24px` |
| `nav a`, `button`, `.copy`, `.job`, isi paragraf desktop, `table` | `14px / 21px` |
| `.field`, `.note`, `.response .meta` | `13px / 19px` |
| `.badge`, `.method`, `th`, `nav .group` | `12px / 18px` |
| `code`, `pre`, `.mono`, `textarea.json-body` | `13px / sekitar 19px` desktop |
| `input`, `select` | `14px / 21px` desktop |

Tambahkan override `(any-pointer: coarse)`: `input`, `select`, `textarea.json-body` **16px / 24px**; paragraf utama dalam `main` **16px / 24px**. Tabel dan blok kode tetap 13–14px serta dapat digulir/diperbesar. Jangan mengubah stack font, script interaktif, HTML endpoint, respons HTTP, CSS warna, atau endpoint Rust.

**Gate:** form docs dapat dipakai pada desktop/mobile, hierarki h1/h2/h3 jelas, tidak ada teks informasi di bawah 12px, dan seluruh contoh API tetap utuh.

### Langkah 5 — Verifikasi terarah

1. Jalankan `npm run build`. Bila gagal, laporkan apakah gagal sudah ada pada baseline atau akibat patch ini; perbaiki hanya yang kamu sebabkan.
2. Jalankan pencarian `rg -n 'font-size:\s*(10|11)px|font:\s*(10|11)px|font:\s*[0-9]+\s+(10|11)px'` pada file scope. Hasil informasi UI harus kosong; jika ada pengecualian non-informasi, sebutkan selector dan alasannya.
3. Lihat UI **light dan dark** pada sekitar **375px** dan **560px**: app bar/statusline, tab, form printer/preset, error log/API access log, onboarding, editor ruler/label, dan `/docs`. Periksa label panjang, error panjang, state loading/empty, tombol disabled, serta focus keyboard. Jangan mengklaim screenshot/QA visual bila tidak tersedia.
4. Periksa mobile/touch: input tidak di bawah 16px, paragraf instruksi panjang 16px, target utama minimal 44px, tidak ada horizontal clipping yang menghilangkan status atau aksi. Pada zoom 200%, teks dan tombol harus tetap dapat dicapai; overflow lokal boleh scroll.
5. Jalankan `git diff --check` untuk file tracked, lalu tinjau diff. Pastikan warna, logo, ikon, teks fitur, dan business logic tidak berubah. Jangan menjalankan seluruh Rust test suite untuk perubahan CSS ini.

**Gate:** build lulus, audit ukuran lulus, dan QA visual dilaporkan apa adanya. Jika `/docs` atau UI desktop tidak dapat dijalankan, tandai sebagai **belum diverifikasi**, bukan lulus.

### Langkah 6 — Laporan akhir

Tulis ringkas dalam bahasa Indonesia:

1. File yang diubah dan ukuran lama→baru untuk setiap peran.
2. Hasil `npm run build`, hasil audit ukuran, viewport/tema yang benar-benar diperiksa.
3. Overflow atau keterbatasan yang tersisa, dengan lokasi spesifik.
4. Konfirmasi bahwa palet, logo/ikon, font family, target sentuh, dan perilaku cetak tidak berubah.

Jangan mengklaim “semua font 14px”; keberhasilan adalah **base 14px dengan hierarki 20/18/16/14/13/12px yang konsisten** dan pengecualian mobile 16px yang sudah ditetapkan.
