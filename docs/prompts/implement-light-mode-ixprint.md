# Prompt implementasi Light / Dark / System pada aplikasi ix.print

Salin seluruh isi dokumen ini ke AI coding agent yang bekerja di repo `fxPrint`. Kerjakan **Langkah 0 sampai 7 secara berurutan**. Targetnya adalah tema aplikasi desktop ix.print yang dapat dipilih di Settings: **System, Light, Dark**, dengan **System sebagai default**. Palet light dan dark serta ukuran dasar 14px sudah ada; implementasikan pemilihan tema dan perbaiki hanya masalah tema yang terbukti saat validasi.

## 0. Kontrak dan ruang lingkup

1. Jangan browsing, mencari contoh desain, meminta AI lain membuat keputusan, atau memilih ulang palet, font, arsitektur, lokasi kontrol, copy, storage, maupun perilaku tema. Semua keputusan itu ditetapkan di bawah. Pemeriksaan kode dibatasi ke file yang disebut dalam Langkah 0 dan file yang terbukti perlu diperbaiki oleh hasil QA.
2. Baca `AGENTS.md`, `docs/BRAND.md` (khususnya bagian 5, 6, 8, 16–18, 22), dan `docs/ARCHITECTURE.md`. Jalankan Pre-flight Checklist bagian 22 sebelum edit. Panduan brand dan spesifikasi dalam prompt ini adalah sumber keputusan; screenshot referensi bukan spesifikasi fitur aplikasi.
3. Pertahankan Instrument Sans untuk UI dan IBM Plex Mono untuk data teknis; ukuran dasar UI **14px/21px** dan hierarki **20/18/16/14/13/12px**. Pada input pointer kasar dan instruksi panjang di layar kecil gunakan sedikitnya 16px/24px. Pertahankan logo, ikon, SVG, favicon, font files, dan aset ix.print yang sekarang.
4. Jangan mengubah API, Rust, database, konfigurasi bridge, hasil cetak, data pengguna, alur printer, atau halaman API `/docs`. `/docs` dibuka di browser dan tetap mengikuti `prefers-color-scheme` browser/OS, terpisah dari preferensi desktop.
5. Working tree mungkin berisi perubahan pengguna. Catat `git status --short`; jangan reset, force checkout, commit, reformat seluruh file, atau menimpa perubahan yang tidak terkait. Gunakan patch kecil. Jangan menambah dependency, state manager, service backend, atau toggle di app bar.
6. Jika struktur file telah bergeser, pakai padanan terdekat yang sudah ada tanpa mengubah kontrak di dokumen ini. Jika ada konflik nyata yang membuat spesifikasi tidak dapat dipenuhi, laporkan file, baris, dan konflik spesifik; jangan mengarang keputusan produk baru. Selesaikan bagian yang tidak terhalang.

## 1. Perilaku final yang wajib

| Keadaan | Preferensi yang terlihat di Settings | Tema efektif aplikasi |
|---|---|---|
| Belum ada nilai tersimpan; OS terang | System | Light |
| Belum ada nilai tersimpan; OS gelap | System | Dark |
| Nilai tersimpan `system` | System | Mengikuti OS saat itu |
| Nilai tersimpan `light` | Light | Light, walau OS gelap |
| Nilai tersimpan `dark` | Dark | Dark, walau OS terang |
| Nilai tersimpan tidak dikenal / pembacaan storage gagal | System | Mengikuti OS saat itu |
| OS berganti saat System dipilih | System | Berubah langsung tanpa reload |
| OS berganti saat Light atau Dark dipilih | Tetap pilihan manual | Tidak berubah |
| `matchMedia` tidak tersedia / melempar error | Pilihan tetap dapat dipilih | System jatuh ke Light; Light/Dark manual tetap bekerja |
| Penyimpanan pilihan gagal | Pilihan baru tetap aktif selama sesi | Tampilkan notifikasi error sekali untuk aksi itu |

- Kunci `localStorage`: **`ixprint.appearance.v1`**. Satu-satunya nilai sah: **`system`**, **`light`**, **`dark`**. Default System tidak perlu ditulis saat pertama kali dibuka; tulis saat pengguna memilihnya. Jika nilai tersimpan tidak sah, perlakukan sebagai System tanpa mencoba migrasi/cleanup otomatis.
- Terapkan hasil resolusi pada elemen `<html>` sebagai `data-theme="light"` atau `data-theme="dark"`. Atribut itu selalu berisi **tema efektif**, bukan string `system`. Preferensi pengguna disimpan terpisah dalam state aplikasi.
- Terapkan preferensi **sebelum `app.mount('#app')`** agar konten Vue pertama menggunakan tema yang benar. Perubahan select harus berlaku langsung, tanpa tombol Save, restart, atau reload.
- `System` mengikuti perubahan OS melalui satu listener `matchMedia('(prefers-color-scheme: dark)')`. Daftarkan listener sekali selama umur aplikasi. Listener memperbarui tema efektif hanya bila preferensi saat itu `system`. Saat pengguna kembali memilih System, hitung ulang dari keadaan OS terbaru.
- Gunakan CSS `color-scheme: light`/`dark` sesuai tema efektif agar kontrol native, scrollbar, dan rendering browser tidak bercampur tema.
- App bar ix.print tetap Obsidian pada kedua tema; kanvas/kertas preview cetak tetap putih; warna status success/warning/error tetap semantik.

## 2. Token, tipografi, dan kontras yang dikunci

Nilai ini **sudah ada** di `src/app/app.css`; pertahankan saat mengubah mekanisme tema. Light berada di `:root`, dark pindah dari query OS ke selector `:root[data-theme="dark"]`. Jangan menciptakan token warna baru untuk tema.

| Peran | Light | Dark |
|---|---|---|
| `--bg` | `#F2F1ED` | `#111214` |
| `--surface` | `#FBFBF9` | `#1B1D20` |
| `--text` | `#111214` | `#F2F1ED` |
| `--muted` | `#5E646E` | `#9B9EA4` |
| `--line` | `#DAD8D2` | `#2B2E33` |
| `--border-control` | `#8A909A` | `#777F8C` |
| `--accent`, `--brand`, `--brand-ink` | `#4F5900` | `#EFFF2A` |
| `--brand-fill` | `#EFFF2A` | `#EFFF2A` |
| `--action-hover` | `#DDEB1D` | `#DDEB1D` |
| `--action-text` | `#111214` | `#111214` |
| `--brand-pale`, `--brand-soft` | `#F2F4D6` | `#303511` |
| `--ok` / `--ok-soft` | `#1F7A4D` / `#E3F1E9` | `#5CC28E` / `#142A1F` |
| `--warn` | `#9A5B00` | `#E0A84A` |
| `--danger` / `--danger-soft` | `#C2382B` / `#F9E6E3` | `#F07A6C` / `#2E1714` |

Token struktur lain (`--bar-control-border`, `--bar-warn`, tinggi kontrol, spacing, radius, easing, font) tetap seperti sekarang. `index.html` memiliki meta `theme-color` lama `#0B0C0E`; ubah menjadi `#111214` agar cocok dengan app bar tetap gelap pada kedua tema. Jangan membuat meta ini berubah dinamis.

Aturan penggunaan:

1. Light: workspace Soft Ivory, panel `#FBFBF9`, teks utama Obsidian. Teks aktif, link, dan fokus memakai olive `#4F5900`. Tombol primary Acid Yellow memakai teks Obsidian **dan batas gelap**; yellow tidak boleh menjadi teks/fokus/border tunggal di atas light background.
2. Dark: gunakan token dark yang sudah ada; primary tetap yellow dengan teks Obsidian. Jangan menggelapkan atau mencerahkan seluruh aset hanya karena tema berganti.
3. Teks normal minimal 4,5:1, teks besar minimal 3:1, batas/fokus kontrol penting minimal 3:1. Nilai acuan: Obsidian/Soft Ivory sekitar 16,6:1, muted/Soft Ivory 5,3:1, olive/Soft Ivory 6,7:1, yellow/Soft Ivory sekitar 1:1. Selected dan status penting memiliki teks/indikator selain warna isi.
4. Ukuran target sentuh utama minimal 44×44px; jangan mengecilkan tipografi atau kontrol untuk mengatasi overflow. Jangan mengubah skala 14px hanya karena tema menjadi selectable.

## 3. File dan kontrak perubahan

Edit hanya file berikut, ditambah satu file scoped style jika QA membuktikan masalah spesifik:

| File | Perubahan wajib |
|---|---|
| `src/app/appearance.ts` **baru** | State dan resolusi tema, akses localStorage, listener OS, penerapan `data-theme` |
| `src/main.ts` | Panggil `initializeAppearance()` sebelum membuat/mount Vue app |
| `src/app/useAppShell.ts` | Ekspos preferensi dan aksi perubahan; gunakan `notify` yang sudah ada untuk kegagalan simpan |
| `src/app/App.vue` | Oper preferensi ke Settings dan sambungkan event perubahan ke aksi shell |
| `src/features/settings/SettingsView.vue` | Tambahkan satu native select Appearance di Settings |
| `src/app/app.css` | Jadikan dark token selector `data-theme`, tambah `color-scheme`; pertahankan isi token |
| `index.html` | Perbarui meta `theme-color` menjadi `#111214` |
| `src/app/appearance.test.ts` **baru** | Unit test perilaku tema dan storage |

Jangan sentuh `src-tauri/src/interfaces/http/assets/docs.html`, `public/ix-print-logo.svg`, `public/ix-print-mark.svg`, atau ikon Tauri. Bila nilai visual dalam file scoped terbukti salah hanya pada salah satu tema, perbaiki selector lokal terkecil dan sebutkan buktinya di laporan.

## 4. Langkah eksekusi

### Langkah 0 — Baseline terarah

1. Baca sumber aturan pada bagian 0 dan tandai checklist brand. Jalankan `git status --short`; catat perubahan yang sudah ada pada file target.
2. Baca hanya `src/main.ts`, `src/app/useAppShell.ts`, `src/app/App.vue`, `src/features/settings/SettingsView.vue`, blok token/Settings di `src/app/app.css`, `index.html`, dan konfigurasi tes yang diperlukan untuk satu test file baru. Jangan memindai seluruh repo.
3. Konfirmasi fakta awal: `:root` sudah light dengan 14px base; dark token masih berada dalam `@media (prefers-color-scheme: dark)`; `SettingsView` belum punya Appearance; `useAppShell` sudah punya `notify` dan pola localStorage.

**Gate:** daftar file yang akan berubah cocok dengan tabel bagian 3; perubahan pengguna tidak akan tertimpa.

### Langkah 1 — State tema di `src/app/appearance.ts`

1. Ekspor tipe `Appearance = 'system' | 'light' | 'dark'`, ref preferensi bernama `appearance` dengan nilai awal `system`, fungsi `initializeAppearance(): void`, dan fungsi `setAppearance(next: Appearance): boolean` (`true` bila tersimpan, `false` bila penyimpanan gagal). Biarkan ref tersebut menjadi satu sumber state; jangan membuat state kedua di Settings.
2. Dalam `initializeAppearance`, lakukan idempotency guard sehingga pemanggilan berulang tidak mendaftarkan listener kedua. Baca kunci storage dengan `try/catch`; terima hanya tiga string sah. Nilai lain atau exception menjadi `system`. Jangan menulis storage saat inisialisasi.
3. Coba buat satu `MediaQueryList` dari `window.matchMedia('(prefers-color-scheme: dark)')`; lindungi ketiadaan fungsi atau exception. Untuk System, `matches === true` berarti dark, selain itu light. Simpan referensi MediaQueryList sehingga perubahan berikutnya dapat dibaca langsung. Pasang tepat satu `change` listener jika tersedia. Listener hanya memanggil penerapan tema bila `appearance.value === 'system'`.
4. Buat fungsi privat yang menyelesaikan preferensi menjadi `light`/`dark` dan menetapkan `document.documentElement.dataset.theme`. Pilihan manual menang atas OS. Panggil setelah membaca storage dan setiap kali preferensi/OS yang relevan berubah.
5. Dalam `setAppearance`, ubah ref dan terapkan tema **sebelum** mencoba `localStorage.setItem`. Tangkap exception dan kembalikan `false` tanpa mengembalikan ref/tema; pilihan tetap aktif untuk sesi ini. Nilai pilihan berasal dari select tiga opsi, sehingga tidak perlu mekanisme validasi/form baru.
6. Di `src/main.ts`, import fungsi tersebut dan panggil sekali sebelum `createApp(App)` serta sebelum `app.mount('#app')`. Jangan menaruh inline script di `index.html` karena CSP Tauri.

**Gate:** tanpa membuka Settings, reload memulihkan pilihan sah; System mengikuti OS, pilihan manual mengalahkan OS, dan `<html data-theme>` selalu menunjukkan tema efektif.

### Langkah 2 — Wiring shell dan kontrol Settings

1. Di `useAppShell.ts`, import `appearance` dan `setAppearance`. Ekspor ref `appearance` dalam objek return. Tambahkan `changeAppearance(next: Appearance)` yang memanggil `setAppearance(next)`; bila hasilnya `false`, panggil `notify('Appearance changed for this session, but could not be saved on this computer.', true)`. Jangan menulis storage lagi di shell.
2. Di `App.vue`, destructure `appearance` dan `changeAppearance`. Tambahkan `:appearance="appearance"` dan `@appearance-change="changeAppearance"` pada `SettingsView`. Jangan menambah kontrol tema di `AppHeader`.
3. Di `SettingsView.vue`, perluas props dengan `appearance: 'system' | 'light' | 'dark'` dan emits dengan event `appearance-change` yang membawa union yang sama. Komponen ini hanya mengirim pilihan; tidak membaca `localStorage`, `matchMedia`, atau memutuskan tema efektif.
4. Tambahkan `<section class="settings-group">` **pertama setelah slot diagnostics dan sebelum Integration setup**. Gunakan struktur heading yang sudah ada, ikon `Monitor` dari `lucide-vue-next`, judul **`Appearance`**, deskripsi **`Choose how ix.print looks on this computer.`**. Di dalamnya gunakan native `<select>` berlabel **`Theme`** dengan `id="appearance-theme"`, `class="appearance-select"`, `:value="appearance"`, dan tiga opsi tepat dalam urutan: **`System`** (`system`), **`Light`** (`light`), **`Dark`** (`dark`). Tambahkan hint **`System follows your device appearance.`** dengan `id="appearance-theme-hint"` dan `aria-describedby="appearance-theme-hint"` pada select. Pada event `change`, baca `HTMLSelectElement.value`, kirim `appearance-change` hanya jika nilainya `system`, `light`, atau `dark`; ini juga menjaga tipe event tanpa cast string yang tidak terverifikasi.
5. Pakai class/spacing form Settings yang sudah ada. Dalam scoped style komponen, set `.appearance-select { min-height: 44px; }` dan pada `@media (pointer: coarse)` set `.appearance-select { font-size: 16px; line-height: 24px; }`. Jangan membuat segmented control, radio group, kartu baru, tombol Save, atau modal konfirmasi. Copy Settings tetap berbahasa Inggris seperti UI sekarang.

**Gate:** Settings menampilkan pilihan yang benar setelah reload; perubahan terasa langsung; keyboard dapat memberi fokus dan memilih setiap opsi; kesalahan storage muncul sebagai toast tetapi tema sesi tetap berubah.

### Langkah 3 — CSS dan metadata

1. Di `src/app/app.css`, tambahkan `color-scheme: light` pada `:root`. Ganti pembungkus `@media (prefers-color-scheme: dark) { :root { … } }` dengan `:root[data-theme="dark"] { … }`, isi token dark **persis seperti semula**, serta `color-scheme: dark`. Dengan demikian OS tidak dapat menimpa pilihan Light/Dark manual.
2. Pertahankan semua token light di `:root`, base font 14px, font families, aturan `.appbar`, kelas status, dan stylesheet komponen. Jangan menyalin seluruh CSS menjadi dua file atau menambah `!important` global.
3. Di `index.html`, ganti hanya nilai meta `theme-color` menjadi `#111214`. Jangan mengubah favicon atau CSP.
4. Jalankan satu pencarian terarah `rg -n 'prefers-color-scheme' src/app src/features src/shared`. Baca hanya hasil yang ditemukan. Jika ada selector aplikasi yang membuat mode manual tidak konsisten, pindahkan kondisi itu ke `data-theme` dengan patch lokal. Jangan mengubah query di `/docs`.

**Gate:** setiap pilihan manual mengendalikan seluruh UI aplikasi; native input dan scrollbar mengikuti tema efektif; app bar tetap gelap pada Light dan Dark.

### Langkah 4 — Unit test terarah

Buat `src/app/appearance.test.ts` dengan Vitest dan jsdom. Untuk setiap kasus yang menguji inisialisasi singleton, gunakan reset module/mock yang bersih supaya ref dan guard tidak bocor antar kasus. Mock `window.matchMedia` dengan MediaQueryList yang bisa mengirim event `change`; pulihkan storage, DOM `data-theme`, dan mock setelah tes.

Kasus wajib:

1. Storage kosong + OS terang → preferensi `system`, `data-theme=light`; storage tetap kosong.
2. Storage kosong + OS gelap → `system`, `data-theme=dark`.
3. Storage berisi `light` saat OS gelap → `light`; storage berisi `dark` saat OS terang → `dark`.
4. Storage invalid dan `getItem` melempar → System; gunakan OS saat itu.
5. Event perubahan OS mengubah tema efektif saat System, tetapi tidak mengubahnya saat Light/Dark manual.
6. Setelah memilih System kembali, tema langsung mengambil nilai OS terbaru.
7. `setAppearance` menulis tepat `ixprint.appearance.v1`, mengubah ref/DOM langsung, dan mengembalikan `true` saat berhasil.
8. `setItem` melempar → mengembalikan `false`, ref/DOM tetap pada pilihan baru selama sesi.
9. `matchMedia` hilang atau melempar → System menjadi Light; pilihan Dark manual masih dark.
10. `initializeAppearance` dipanggil dua kali → satu listener, preferensi tidak dibaca/ditimpa ulang.

Jalankan `npm test -- src/app/appearance.test.ts` lalu `npm run build`. Jangan menjalankan seluruh Rust test suite untuk perubahan UI ini.

**Gate:** test fokus lulus dan TypeScript/Vite build lulus. Jika tes gagal, perbaiki penyebab di ruang lingkup ini, lalu ulangi hanya pemeriksaan yang gagal.

### Langkah 5 — QA visual dan aksesibilitas

Uji aplikasi desktop bila lingkungan memungkinkan; jika tidak, jelaskan bagian visual yang belum diverifikasi. Gunakan OS terang dan gelap untuk matriks berikut:

| OS | Pilihan Settings | Yang harus terlihat |
|---|---|---|
| Terang | System | Light |
| Gelap | System | Dark |
| Gelap | Light | Light |
| Terang | Dark | Dark |

Untuk setiap kombinasi, periksa halaman Settings, Printers, Presets, Print, History, dan editor bila dapat dibuka. Uji loading, empty, error, disabled, fokus keyboard, tab aktif, badge, selected state, toast, form, serta preview kertas. Periksa lebar **380px** (minimum desktop Tauri) dan **560px** (lebar konten maksimum), zoom **200%**, dan label/pesan panjang. Jangan memperbaiki overflow dengan mengecilkan teks penting atau target sentuh. Periksa bahwa `/docs` tetap mengikuti OS/browser terlepas dari pilihan desktop.

**Gate:** teks normal ≥4,5:1, teks besar ≥3:1, kontrol/fokus penting ≥3:1; status dan selected state punya penanda selain warna. Catat layar yang benar-benar diuji, jangan mengklaim QA yang tidak dilakukan.

### Langkah 6 — Review diff

1. Jalankan `git diff --check` untuk file tracked dan periksa whitespace pada file baru. Tinjau diff file target saja; pisahkan perubahan awal pengguna dari hasil pekerjaan ini.
2. Pastikan tidak ada perubahan Rust/API, route `/docs`, aset, font, base 14px, kontrak cetak, atau konfigurasi bridge. Pastikan dark token tidak berubah nilainya, hanya kondisi aktifnya.
3. Pastikan hanya satu kunci storage dan satu listener OS. Pastikan default System serta kegagalan baca/simpan berjalan menurut tabel bagian 1.

**Gate:** perubahan kecil, sesuai scope, tanpa regresi yang terdeteksi.

### Langkah 7 — Laporan akhir

Laporkan dalam bahasa Indonesia: file yang diubah dan alasannya; hasil `npm test -- src/app/appearance.test.ts`, `npm run build`, dan diff check; matriks tema yang diuji; hasil aksesibilitas yang benar-benar diverifikasi; serta kendala yang tersisa dengan lokasi spesifik. Jangan menyatakan tema sudah terpasang bila baru menulis prompt atau baru menjalankan audit.
