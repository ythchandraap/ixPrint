# Prompt implementasi brand .ixxyyss pada ix.print

Salin seluruh prompt ini ke AI coding agent yang bekerja di repo `fxPrint`. Kerjakan langkah 0–7 secara berurutan. Spesifikasi di bawah adalah keputusan implementasi, bukan bahan untuk brainstorming.

## 0. Kontrak kerja

Kamu adalah implementer UI ix.print. Tujuanmu menerapkan warna dan bahasa visual [brand guide](../BRAND.md) ke antarmuka ix.print yang **sudah ada**, tanpa mengubah alur kerja produk. Bahasa laporan: Indonesia. Nama variabel dan komentar kode: ikuti bahasa kode setempat.

Aturan yang tidak boleh ditawar:

1. **Jangan mencari jawaban sendiri.** Jangan browsing internet, mencari desain lain, meminta AI lain memilih palet, membuat moodboard, menafsirkan ulang arah brand, atau mengganti keputusan yang sudah ditetapkan di prompt ini. Baca hanya `AGENTS.md`, `docs/BRAND.md`, `docs/ARCHITECTURE.md`, file target pada bagian 1, dan file lain yang langsung diperlukan untuk memastikan pemakaian token atau menjalankan pemeriksaan. Pencarian kode harus terarah pada token, selector, atau aset yang disebut di sini. Jika ada konflik dengan panduan umum, keputusan khusus ix.print yang tertulis di prompt ini berlaku untuk pekerjaan ini.
2. Jika detail penting tidak ditentukan, ada konflik yang tidak bisa diselesaikan oleh aturan di bawah, atau sebuah perubahan memerlukan keputusan produk baru: **jangan tebak**. Jelaskan file, baris, dua opsi konkret, dampaknya, dan satu pertanyaan untuk pemilik proyek. Lanjutkan pekerjaan lain yang tidak bergantung pada jawaban itu.
3. Jangan menyamakan gambar referensi dengan spesifikasi fitur. Teks `Klaten`, `EST. 2024`, angka dashboard, nama proyek, tata letak desktop lebar, ikon `.ix`, dan foto beton dalam gambar adalah contoh visual. Jangan menyalinnya menjadi data, fitur, atau aset produk.
4. Jangan mengganti font. Desktop UI tetap memakai **Instrument Sans** untuk teks dan **IBM Plex Mono** untuk data teknis/label pendek. Jangan memasang Plus Jakarta Sans, Geist Mono, atau font baru. Halaman API `/docs` tetap memakai stack font yang sekarang karena pekerjaan ini tidak menambah endpoint/font serving.
5. Jangan mengubah `public/ix-print-logo.svg`, `public/ix-print-mark.svg`, favicon, ikon Tauri, bentuk logo, atau warna di dalam aset tersebut. `public/ix-print-mark.svg` memang masih mengandung cobalt; itu **pengecualian yang disengaja** sesuai aturan mempertahankan ikon produk.
6. Jangan membuat dashboard, hero, foto, ornamen, komponen baru, animasi, route, fitur, copy marketing, atau dependency. Jangan mengubah API contract, schema, business logic, atau data. Perubahan markup hanya boleh untuk memperbaiki makna state/aksesibilitas yang disebut di sini.
7. Working tree bisa berisi perubahan milik pengguna. Periksa `git status --short` sebelum bekerja; jangan reset, checkout paksa, reformat seluruh file, atau menimpa perubahan lain. Gunakan patch kecil. Jangan commit kecuali diminta.
8. Jangan berhenti untuk minta persetujuan per langkah bila spesifikasinya jelas. Jika langkah gagal, perbaiki dalam scope yang sama. Jangan menyatakan verifikasi lulus jika belum dijalankan.

## 1. Scope dan urutan file

File target utama:

1. `src/app/app.css` — sumber token dan komponen global.
2. `src/app/AppHeader.vue` — dot status bridge.
3. `src/features/presets/PresetsTab.vue` — dot status Shared/Private.
4. `src/features/design-editor/design-editor.css` — kanvas putih dan state editor.
5. `src/features/printers/PrinterDeviceForm.vue` — segmented control dan aksi lokal.
6. `src-tauri/src/interfaces/http/assets/docs.html` — palet halaman API yang berdiri sendiri.

File Vue lain di `src/features/` atau `src/shared/ui/` boleh diubah **hanya** jika pemakaian token lama di file tersebut gagal memenuhi aturan pada bagian 2–6. Jangan ubah logic, struktur data, atau copy. Jangan sentuh backend Rust untuk pekerjaan ini.

Referensi visual, sesuai urutan halaman 01–04:

- `/Users/datin/Downloads/ChatGPT Image Sep 22, 2026, 10_57_36 PM (1).png`
- `/Users/datin/Downloads/ChatGPT Image Sep 22, 2026, 10_57_36 PM (2).png`
- `/Users/datin/Downloads/ChatGPT Image Sep 22, 2026, 10_57_37 PM (3).png`
- `/Users/datin/Downloads/ChatGPT Image Sep 22, 2026, 10_57_38 PM (4).png`

Gunakan gambar hanya untuk memeriksa hierarki, warna, dan restraint. Nilai hex dan aturan produk yang mengikat ada di prompt ini dan `docs/BRAND.md`.

## 2. Keputusan visual yang sudah final

### 2.1 Primitive brand

| Peran | Nilai final |
|---|---|
| Obsidian | `#111214` |
| Graphite | `#1B1D20` |
| Carbon | `#24262A` |
| Soft Ivory | `#F2F1ED` |
| Muted Grey | `#9B9EA4` |
| Divider dark | `#2B2E33` |
| Acid Yellow | `#EFFF2A` |

Hapus primitive cobalt dari CSS aplikasi dan CSS `/docs`. Jangan hapus cobalt dari aset logo/ikon yang dibekukan di atas. Neutral tetap mendominasi. Yellow hanya untuk aksi utama, pilihan aktif, atau perhatian yang memang perlu. Jangan memakai batas rasio 70/25/5 sebagai hitungan literal pada setiap layar.

### 2.2 Token semantik aplikasi

Gunakan nama token yang sudah ada di `src/app/app.css`; jangan membuat sistem token kedua. Set nilai berikut secara persis. Nilai status (`--ok`, `--warn`, `--danger` dan varian soft), ukuran kontrol, spacing, radius, motion, serta background kertas putih tetap seperti sekarang kecuali ada perbaikan kontras yang diwajibkan di bawah.

| Token | Light `:root` | Dark `prefers-color-scheme` |
|---|---|---|
| `--bg` | `var(--ivory)` | `var(--obsidian)` |
| `--surface` | `#FBFBF9` | `var(--graphite)` |
| `--text` | `var(--obsidian)` | `var(--ivory)` |
| `--muted` | `#5E646E` | `var(--steel)` |
| `--line` | `#DAD8D2` | `var(--border-dark)` |
| `--accent` | `#4F5900` | `var(--acid-yellow)` |
| `--brand` | `#4F5900` | `var(--acid-yellow)` |
| `--brand-fill` | `var(--acid-yellow)` | `var(--acid-yellow)` |
| `--action-hover` | `#DDEB1D` | `#DDEB1D` |
| `--action-text` | `var(--obsidian)` | `var(--obsidian)` |
| `--brand-pale` | `#F2F4D6` | `#303511` |
| `--brand-ink` | `#4F5900` | `var(--acid-yellow)` |
| `--brand-soft` | `#F2F4D6` | `#303511` |

Nama primitive yang dipakai di CSS: `--obsidian`, `--graphite`, `--carbon`, `--ivory`, `--steel`, `--border-dark`, `--acid-yellow`. `#4F5900` adalah turunan **fungsional** untuk teks/fokus di tema terang, bukan warna logo baru. Jangan menampilkan Acid Yellow sebagai teks atau satu-satunya garis/border pada Soft Ivory atau putih: kontrasnya sekitar 1:1. `#4F5900` terhadap Soft Ivory sekitar 6,7:1. Acid Yellow terhadap Obsidian sekitar 17:1.

### 2.3 Prinsip UI produk

- Pertahankan struktur aplikasi ringkas dengan lebar maksimum saat ini, navigasi tab, form, daftar, editor, dan workflow cetak. Jangan meniru mockup dashboard lebar pada gambar.
- Pertahankan pilihan light/dark berdasarkan sistem. App bar tetap gelap pada kedua tema. Jangan paksa seluruh aplikasi ke dark mode.
- Instrument Sans tetap untuk judul, body, tombol, form; IBM Plex Mono tetap untuk ID, angka/teknis, kode, timestamp. Jangan memindahkan semua label ke mono atau menaikkan semua heading ke ukuran hero.
- Divider halus boleh tetap halus. Border yang diperlukan untuk mengenali input, tombol, pilihan, atau fokus harus terlihat jelas; jangan memakai `--border-dark` sebagai satu-satunya petunjuk kontrol.
- Status OK/Shared, warning, error, dan offline harus tetap punya teks dan warna semantik. Yellow tidak menggantikan hijau/amber/merah.
- Pertahankan `40px` kontrol desktop yang ada, `32px` kontrol kecil, `36px` tombol ikon, dan override `44px` untuk pointer kasar. Jangan mengecilkan target sentuh.

## 3. Langkah implementasi

### Langkah 0 — Baseline dan perlindungan perubahan

1. Baca `AGENTS.md` dan `docs/BRAND.md`, termasuk checklist bagian 22.
2. Jalankan `git status --short`; catat file target yang sudah berubah. Jangan menganggap perubahan itu milikmu.
3. Baca bagian token, selector aktif/fokus, dan media query di `src/app/app.css`; baca hanya blok CSS terkait di file target lain. Cari sisa warna lama dengan pola persis `cobalt|#536dfe|#394bb8|#8fa0ff|#3a54e0|#8a9bff|#0b0c0e|#17191d|#22252a|#292c31|#9ca3ad` pada file scope, case insensitive.
4. Simpan baseline perilaku: tab utama, bridge, preset sharing, tombol cetak, editor, dan `/docs` harus tetap memiliki fungsi yang sama. Catat masalah visual yang sudah ada secara terpisah; jangan memperluas scope untuk memperbaikinya.

**Gate:** laporkan daftar file target yang sudah dirty dan selector/token yang perlu disentuh. Lanjutkan langsung ke langkah 1.

### Langkah 1 — Migrasi token di `src/app/app.css`

1. Ganti tujuh primitive sesuai tabel 2.1. Pertahankan nama `--steel` dan `--border-dark` agar pemakaian lama tetap bekerja. Hapus `--cobalt`, `--cobalt-deep`, `--cobalt-light`; tambahkan satu primitive `--acid-yellow: #EFFF2A`.
2. Ganti token light/dark sesuai tabel 2.2. Jangan ubah nilai status semantik atau dimensi kontrol.
3. Ganti semua pemakaian langsung `var(--cobalt*)` dalam `app.css` menurut perannya: active/focus/header action → `var(--acid-yellow)` pada app bar gelap; teks/fokus pada permukaan yang bisa terang → `var(--brand)` atau `var(--accent)`; warna status → token `--ok`, `--warn`, atau `--danger` yang sudah ada. Jangan melakukan global replace buta.
4. `.appbar`, `.statusline`, dan `.toast` tetap memakai permukaan gelap. Pastikan foreground mengikuti Soft Ivory/Muted Grey baru. Pertahankan border kontrol app bar yang terlihat.
5. `:focus-visible` global harus tetap terlihat di dua tema: light memakai `#4F5900`, dark memakai Acid Yellow. Fokus input yang memakai border dan ring juga harus memakai token tersebut.

**Gate:** tidak ada referensi `--cobalt*` atau hex cobalt di `src/app/app.css`; light dan dark masing-masing memiliki mapping tabel 2.2 lengkap.

### Langkah 2 — Tombol, navigasi, form, dan badge global

1. `button.primary`: isi `--brand-fill`, teks `--action-text`. Pada tema terang border tombol harus memakai `--brand` (`#4F5900`), **bukan** `--brand-fill`, karena yellow hampir menyatu dengan Soft Ivory. Pada hover, isi `--action-hover`, border tetap `--brand`. Pada tema gelap border `--brand` akan menjadi yellow. Pertahankan disabled state yang sudah ada dan pastikan labelnya tetap terbaca.
2. Tab aktif: label `--text`, underline `--accent`. Ini menghasilkan underline olive di light dan yellow di dark. Tab hover/focus harus tetap berbeda dari tab aktif.
3. Input, select, textarea: pertahankan surface dan tinggi yang ada; fokus gunakan `--brand`/`--accent` yang kontras. Error tetap merah, bukan yellow. Jangan hapus label atau hint.
4. Checkbox/radio native: ubah `accent-color` dari `--brand-fill` ke `--accent`, agar tanda yang dipilih terlihat pada tema terang. Pastikan teks label dan target sentuh tidak berubah.
5. `.badge`, `.card.active`, tombol ikon hover, dan selected/hover rows: gunakan pasangan `--brand-pale` + `--brand` bila ada teks. Selected state harus bisa dikenali lewat border, underline, atau label juga, bukan hanya tint latar.
6. Jangan mengubah tombol `.danger`, `.warn`, pesan status, atau banner menjadi yellow. Satu area tugas tetap memiliki satu aksi utama visual; aksi sekunder tidak perlu yellow.

**Gate:** periksa light/dark untuk primary, ghost, disabled, fokus keyboard, input error, tab aktif, dan badge. Tidak ada teks yellow di atas surface terang.

### Langkah 3 — Status dot yang memang semantik

1. Di `src/app/AppHeader.vue`, saat `bridgeEnabled` dan `serverState === 'ok'`, gunakan class dot `ok` yang sudah tersedia, bukan `on`. Saat error tetap `fail`; saat off/loading tetap `idle`. Teks `BRIDGE ON`, `BRIDGE FAILED`, dll. tetap seperti sekarang.
2. Di `src/features/presets/PresetsTab.vue`, status `Shared` gunakan class dot `ok`; `Private` tetap `idle`. Teks Shared/Private dan checkbox tetap seperti sekarang. Perubahan ini hanya class tampilan, bukan perilaku sharing.
3. Setelah dua perubahan itu, periksa apakah `.dot.on` masih dipakai. Jika tidak, hapus selector `.dot.on`; jangan biarkan warna yellow mewakili status sukses. `.dot.ok` tetap memakai `--ok` dan `.dot.fail` tetap `--danger`.

**Gate:** bridge ON dan preset Shared tampil hijau **dengan teks**; bridge FAILED merah **dengan teks**; off/private netral. Perilaku toggle tidak berubah.

### Langkah 4 — Editor dan komponen lokal

1. `src/features/design-editor/design-editor.css`: `.stage` adalah kertas putih pada kedua tema. Definisikan token lokal `--canvas-select-ink: #4F5900` dan `--canvas-select-fill: rgb(239 255 42 / 0.16)` pada `.design-editor .stage`. Gunakan ink lokal untuk border `.box.on`, `.handle`, `.band-line`, `.band-label`, dan outline fokus `.stage`. Gunakan fill lokal untuk `.box.on`. Tujuannya: selection tetap terlihat di atas kertas putih saat OS dark. Jangan mewarnai hasil cetak atau mengubah canvas rendering, posisi handle, ukuran ruler, atau interaksi drag.
2. Segmented control aktif di `src/features/design-editor/design-editor.css` dan `src/features/printers/PrinterDeviceForm.vue` memakai isi `--brand-fill`, teks `--action-text`, dan `box-shadow: inset 0 0 0 1px var(--brand)` agar batasnya terlihat di tema terang. Pertahankan tinggi dan jumlah opsi.
3. Tinjau pemakaian `--brand`, `--brand-fill`, `--brand-pale`, dan `--accent` yang sudah ada pada file Vue lain **hanya jika** tampilannya tidak memenuhi aturan 2.3. Perbaiki CSS lokal sekecil mungkin; jangan mengubah data atau alur.
4. Warna `#fff` untuk permukaan kertas, thumbnail, dan checkerboard transparansi tetap. Itu representasi media cetak, bukan primitive brand yang harus diganti.

**Gate:** pemilihan objek, handle, band line, dan label editor jelas di atas kertas putih dalam light/dark. Segmented control aktif jelas dengan/ tanpa persepsi warna.

### Langkah 5 — Halaman API `/docs`

1. Edit hanya CSS di `src-tauri/src/interfaces/http/assets/docs.html`. Jangan mengubah HTML form, script, teks endpoint, route, atau keamanan HTTP.
2. Samakan primitive dark, ivory, grey, divider, yellow dengan tabel 2.1. Hapus token cobalt di file ini.
3. Light: `--brand` dan `--brand-ink` `#4F5900`; `--brand-fill` Acid Yellow; `--brand-soft`/`--brand-pale` `#F2F4D6`; `--action-text` Obsidian; `--action-hover` `#DDEB1D`; `--border-control` `#737B87`. Dark: `--brand`/`--brand-ink` Acid Yellow; `--brand-fill` Acid Yellow; `--brand-soft`/`--brand-pale` `#303511`; action text/hover sama; `--border-control` `#777F8C`. `--bg`, `--surface`, `--text`, `--muted`, `--line` ikuti tabel 2.2. Status hijau/amber/merah dan code block tetap seperti sekarang.
4. Tombol utama halaman docs gunakan `--brand-fill` + `--action-text`, dengan border `--brand`; hover gunakan `--action-hover`. Tombol `ghost` dan `copy` gunakan surface + text + `--border-control`. Input dan select gunakan `--border-control`, bukan divider `--line`. Tab tetap aksi sekunder. Fokus input, tombol, dan link harus terlihat di dua tema.
5. **Jangan ubah deklarasi font `/docs` atau tambah route/font asset baru.** Ini keputusan scope agar pekerjaan visual tidak mengubah server. Font desktop utama tetap Instrument Sans/IBM Plex Mono; stack font docs yang sudah ada dipertahankan.

**Gate:** `/docs` masih menampilkan endpoint dan form interaktif yang sama, tanpa cobalt di CSS, dengan teks dan kontrol terbaca pada light/dark.

### Langkah 6 — Verifikasi terarah

1. Jalankan `npm run build` dari root repo. Jika gagal, bedakan error baru dari error baseline/pekerjaan lain; perbaiki hanya error yang ditimbulkan perubahan ini.
2. Karena perubahan utama visual, lakukan QA manual atau screenshot pada light dan dark untuk lebar sekitar **375px** dan **560px**: app bar/statusline, tab, printer, preset sharing, form, tombol primary/ghost, error/empty/loading, editor selection, serta `/docs`. Jangan menyatakan QA visual lulus bila layar itu belum benar-benar dibuka.
3. Periksa keyboard focus pada tab, tombol, input, pilihan editor, dan kontrol docs; pastikan fokus terlihat. Periksa disabled, hover, selected, success, warning, error, loading. Pada pointer kasar, target utama tetap minimal 44px.
4. Ukur atau hitung pasangan kontras yang digunakan: teks normal ≥4,5:1, teks besar ≥3:1, kontrol/fokus penting ≥3:1. Minimal cek `#4F5900`/Soft Ivory, Acid Yellow/Obsidian, Acid Yellow/Graphite, Muted Grey/Obsidian, dan button text/filled button. Divider halus tidak perlu 3:1 karena bukan satu-satunya batas kontrol.
5. Cari sisa `cobalt` dan hex lama di `src/app/app.css`, `src/features/`, `src/shared/ui/`, dan `src-tauri/src/interfaces/http/assets/docs.html`. Jangan mengubah kemunculan di aset ikon yang dibekukan. Setiap sisa pada file scope harus dijelaskan atau diperbaiki.
6. Jalankan `git diff --check` untuk file tracked dan periksa diff target. Jika file target sudah memiliki perubahan pengguna sebelum pekerjaan, pastikan patch milikmu tidak menimpa bagian tersebut. Jangan menjalankan seluruh Rust test suite untuk perubahan CSS/markup ini.

**Gate:** build lulus; pemeriksaan kontras dan visual dilaporkan dengan bukti singkat. Jika `/docs` tidak bisa dijalankan di lingkungan ini, tulis `belum diverifikasi secara visual`, bukan `lulus`.

### Langkah 7 — Laporan akhir

Berikan laporan ringkas dengan format:

1. **Diubah:** file dan keputusan UI yang benar-benar diterapkan.
2. **Diverifikasi:** perintah, hasil, tema/viewport yang dibuka, dan pasangan kontras yang dihitung.
3. **Belum diverifikasi / kendala:** item spesifik beserta alasan.
4. **Sisa sengaja:** warna cobalt dalam aset ikon yang dibekukan, bila masih ada. Jangan menyebutnya bug yang perlu diganti.

Jangan mengklaim desain sudah identik dengan empat mockup: ix.print adalah aplikasi operasional yang lebih padat. Ukur keberhasilan dari konsistensi brand **dan** keterbacaan, pencegahan kesalahan, serta kelancaran tugas cetak.
