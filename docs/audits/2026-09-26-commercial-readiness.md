# Audit kesiapan jual ix.print

Tanggal: 26 September 2026. Target: **rilis umum tanpa pendampingan, untuk pengembang aplikasi yang membutuhkan integrasi cetak**.

## Keputusan

**NO-GO untuk rilis umum saat ini.** Fungsi inti dan fondasi engineering sudah cukup kuat untuk dilanjutkan menjadi produk komersial. Namun ada risiko hasil cetak yang salah dilaporkan, pengulangan cetak remote, pemulihan job yang tidak jelas, serta kekurangan bukti distribusi dan kompatibilitas pengguna akhir.

Kelayakan teknis dan kemauan pelanggan membayar merupakan dua pertanyaan berbeda. Audit ini menemukan bukti teknis; belum tersedia bukti permintaan pasar, konversi, atau biaya dukungan.

## Batas dan baseline audit

- Checkout: `52e0b4d28214351efd1248ac27d465368dd832b1`, branch `refactor/clean-architecture`, ditambah perubahan lokal pada delapan file dan direktori aset font yang belum di-commit.
- Meninjau kode API, keamanan, job, remote, History, PDF, printing Windows, konfigurasi distribusi, onboarding, dan editor. Membaca sumber resmi untuk browser, signing, EndDoc, dan lisensi PDFium.
- Menguji frontend dan backend pada mesin macOS ini. Beberapa tes PDF benar-benar menggunakan PDFium; banyak tes print memakai adapter pengujian.
- Tidak mencetak ke printer fisik, tidak menginstal artifact pada mesin bersih, dan tidak menjalankan runtime Windows/Linux atau matriks browser.
- Tidak memverifikasi artifact release, sertifikat, status GitHub Actions terbaru, maupun dokumen komersial di luar repository.
- Audit source dan tests ini tidak membuktikan ketiadaan semua kerentanan. Audit dependency Rust/native belum lengkap.

## Hasil verifikasi yang dilakukan

| Pemeriksaan | Hasil |
|---|---|
| `bun run test` | 17 file, 66 tes lulus |
| `bun run build` | Typecheck dan build produksi lulus |
| `cargo test` | 110 library + 3 integration tests lulus; 4 tes ignored |
| `cargo fmt --all -- --check` | Lulus |
| `cargo clippy --all-targets -- -D warnings` | Gagal: 3 lint |
| `bun audit --json` | `{}`; tidak ada advisory yang dilaporkan saat pemeriksaan |
| Dependency Rust/native advisory scan | Belum dilakukan; cargo-audit tidak tersedia |
| Printer fisik, install/upgrade, browser HTTPS | Belum diverifikasi |

Tes backend pertama terhalang sandbox saat membuka listener lokal. Setelah dijalankan dengan izin yang sesuai, seluruh tes aktif lulus. Kegagalan awal tersebut merupakan kendala lingkungan pengujian.

## Fondasi yang sudah baik

- Auth token wajib secara default; token API masuk disimpan sebagai hash, dengan revocation dan pembatasan percobaan token salah.
- Origin browser dapat dibatasi; instalasi baru tidak mengizinkan semua origin secara otomatis.
- Pembatasan request bersamaan per client dan ukuran input sudah ada.
- Idempotency-Key lokal, preflight, callback HMAC, retry callback, dan pemulihan callback pending sudah tersedia.
- Published preset dipisahkan dari draft, dengan satu versi sebelumnya untuk rollback.
- History menyimpan input dan output untuk investigasi, dengan pembatasan path dan permissions owner-only pada Unix.
- Penggunaan PDFium dilindungi lock sepanjang lifetime dokumen; ada tes layout dan concurrent layout.
- Arsitektur modular dalam satu crate mempunyai tes dependency boundary.
- Onboarding, contoh integrasi, dan workspace editor baru sudah ada. Screenshot lama tidak diperlakukan sebagai bukti bahwa layout lama masih berlaku pada checkout ini.

## Temuan prioritas tinggi

### R1 — Hasil EndDoc Windows diabaikan

**Fakta kode:** setelah rendering dianggap berhasil, `EndDoc(dc.0)` dipanggil tanpa memeriksa hasil. Fungsi kemudian mengembalikan job ID sebagai sukses.

Bukti: [printing.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/infrastructure/printing/windows/printing.rs:220).

Microsoft mendefinisikan nilai `EndDoc <= 0` sebagai gagal. Dengan kode sekarang, kegagalan tahap akhir ini dapat luput dari response dan History. Ini risiko false success, bukan bukti bahwa job fisik tertentu telah gagal dalam audit ini. [Dokumentasi EndDoc](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-enddoc).

**Syarat tutup:** periksa hasil EndDoc; propagasikan kegagalan/ketidakpastian sesuai tahap; jangan menyamakan spool submission dengan kertas sudah tercetak. Tambahkan tes failure path dan uji pada Windows dengan printer/driver yang didukung.

### R2 — Idempotency belum sampai ke penerima remote pada alur /print

**Fakta kode:** `print_pdf()` melakukan POST remote dengan Authorization dan hop header, tanpa Idempotency-Key atau identitas operasi stabil. Interface print_pdf juga belum menerima logical job ID.

Bukti: [remote_client.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/infrastructure/net/remote_client.rs:249), [remote_service.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/application/remote_service.rs:266).

**Skenario risiko:** penerima menerima dan mencetak, tetapi response terputus. Pengirim melaporkan gagal. Retry memakai key baru dapat mengirim cetakan kedua. Idempotency lokal membantu replay request yang hasilnya tercatat, tetapi belum mengatasi ketidakpastian di penerima remote. Ini berbeda dari alur print-ready yang membawa source_job_id.

**Syarat tutup:** logical operation ID stabil diteruskan ke receiver, dengan snapshot payload/settings yang konsisten; retry operasi yang sama tidak membuat job penerima baru. Uji putus response setelah acceptance, timeout, retry, dan restart. Hindari janji exactly-once physical printing yang tidak bisa dijamin oleh spooler.

### R3 — Reservation tanpa History dapat tetap pending sampai key kedaluwarsa

**Fakta kode:** idempotency file menyimpan fingerprint dan job_id. Jika reservation sudah ada tetapi History tidak ditemukan, handler mengembalikan Pending. History baru di-append setelah operasi print; kegagalan append hanya ditulis ke stderr.

Bukti: [idempotency_store.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/infrastructure/persistence/idempotency_store.rs:9), [jobs.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/interfaces/http/handlers/jobs.rs:67), [job_service.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/application/job_service.rs:233).

**Inferensi:** crash setelah reserve atau setelah spool submission, atau kegagalan menulis History, dapat meninggalkan request pending tanpa penyelesaian yang dapat diketahui client. Mengganti key secara buta berisiko mencetak ulang jika submission pertama sebenarnya sudah diterima.

**Syarat tutup:** simpan state operasi yang dapat dipulihkan; bedakan belum dikirim, sedang submit, queued, dan hasil tidak diketahui. Lakukan reconciliation dengan spooler bila tersedia. Uji restart dan disk-write failure. Pemulihan tidak boleh otomatis menganggap pekerjaan yang ambigu gagal lalu mencetaknya ulang.

### R4 — Retention History tidak dibersihkan secara periodik ketika aplikasi terus berjalan

**Fakta kode:** prune ditemukan pada startup dan perubahan setting retention; pembacaan/append History tidak menjalankan cleanup periodik.

Bukti: [lib.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/lib.rs:73), [history_service.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/application/history_service.rs:35).

**Dampak:** aplikasi yang terus berjalan dapat menyimpan PDF/JSON melampaui periode yang dipilih hingga restart atau perubahan setting. Ini mempengaruhi ekspektasi privasi dan konsumsi disk.

**Syarat tutup:** cleanup periodik menggunakan setting aktif, mencakup metadata/content/incoming documents, serta melindungi job aktif. Uji pergantian tanggal dengan clock terkontrol dan tanpa restart. Pertahankan keputusan bahwa content retention mengikuti setting History.

### R5 — Packaging lisensi PDFium belum lengkap

**Fakta kode:** fetch script menyalin library dan LICENSE utama, lalu menghapus direktori ekstraksi. build.rs hanya menyalin kedua file tersebut. LICENSE lokal menyebut direktori `licenses/`, yang tidak dipertahankan oleh alur ini.

Bukti: [fetch-pdfium.mjs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/scripts/fetch-pdfium.mjs:63), [build.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/build.rs:34).

**Syarat tutup:** pertahankan lisensi/notice native dan third-party dari paket versi yang benar-benar dibundel; sertakan notice dependency lain dan verifikasi isi installer akhir. Lisensi PDFium mengatur penyertaan notice pada distribusi binary. Temuan ini adalah gap packaging yang perlu ditutup, bukan kesimpulan hukum lengkap tentang semua dependency. [Lisensi PDFium](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE).

### R6 — Rilis tanpa pendampingan belum mempunyai bukti distribusi yang cukup

**Fakta repo:** installer workflow membangun dan mengunggah artifact; tidak terlihat langkah signing/notarization yang dikonfigurasi, publikasi download pelanggan, atau pengujian install/upgrade. Workflow installer tidak menjalankan seluruh gate pengujian sebelum bundling.

Bukti: [build-installers.yml](/Users/datin/Documents/Kerjaan/Personal/fxPrint/.github/workflows/build-installers.yml:70), [tauri.conf.json](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/tauri.conf.json:49).

Ini belum membuktikan semua artifact di luar repo unsigned; sertifikat atau proses manual di luar repo belum diaudit. Untuk macOS, signing/notarization diperlukan bagi distribusi yang dapat dipercaya tanpa langkah bypass pengguna. [Panduan Tauri](https://v2.tauri.app/distribute/sign/macos/).

**Syarat tutup:** verifikasi signature artifact sebenarnya, notarization macOS, installer Windows, install/upgrade/uninstall pada mesin bersih, preservation config saat upgrade, distribusi versi/checksum, serta mekanisme update keamanan yang jelas. Auto-update tidak wajib jika proses update manual terdokumentasi dan teruji.

### R7 — Integrasi HTTPS browser dan topologi server belum dibuktikan

**Fakta:** bridge menggunakan HTTP; setup menghasilkan alamat LAN dan contoh fetch. Belum ditemukan matriks tes browser atau panduan Local Network Access dalam repo yang diperiksa.

Bukti: [IntegrationSetup.vue](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src/features/settings/IntegrationSetup.vue:23), [HTTP server](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/interfaces/http/mod.rs:89).

Chrome menambahkan permission Local Network Access mulai Chrome 142. CORS allowlist tidak menggantikan permission browser; pengecualian mixed content bergantung pada bentuk alamat dan kondisi request. [Dokumentasi Chrome](https://developer.chrome.com/blog/local-network-access).

**Masalah topologi:** saran server-side tidak otomatis menyambungkan server SaaS di cloud ke PC pelanggan dengan private IP. Produk harus membedakan browser pada PC/LAN, server dalam LAN, dan server cloud yang memerlukan jalur jaringan tambahan. Koneksi HTTP remote juga mengirim token dan dokumen tanpa enkripsi transport; README sudah mengakuinya.

**Syarat tutup:** uji dari origin HTTPS sebenarnya pada browser yang dinyatakan didukung, termasuk permission denied, origin salah, bridge mati, firewall, port salah, dan remote offline. Publikasikan topologi yang didukung dan batas trusted-LAN/TLS. Jangan meminta pelanggan mematikan proteksi browser sebagai alur normal.

## Gate engineering dan verifikasi tambahan

### R8 — Gate Clippy yang diwajibkan CI gagal

Tiga lint pada toolchain audit:

- `too_many_arguments`: [ports.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/application/ports.rs:183).
- `derivable_impls`: [history.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/domain/history.rs:20).
- `unnecessary_map_or`: [element.rs](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/domain/template/element.rs:246).

Ketiganya tidak membuktikan crash runtime. Namun checkout ini belum lolos gate yang dinyatakan sendiri dalam README/CI. Tutup lint dan buat artifact dari commit bersih yang seluruh gate-nya hijau.

### R9 — Dukungan hardware dan beban belum terbukti

- Empat tes ignored mencakup live printer/admin, paper limits, dan visual layout. Audit ini tidak menjalankannya.
- README mengklaim output identik lintas OS; layout PDF bersama membantu konsistensi, tetapi Windows raster/GDI dan CUPS vector masih melewati driver/media yang berbeda.
- Belum ada hasil throughput, p95 latency, memory, volume History, disk growth, atau soak test yang ditemukan.
- Bitmap Windows memakai ukuran sheet dan DPI tanpa explicit pixel budget di draw_sheet. PDFium lock mengurangi concurrency native, tetapi satu job berat dapat menahan pekerjaan lain. Ini risiko yang perlu diukur, bukan OOM yang sudah direproduksi.

Bukti: [draw_sheet](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/infrastructure/printing/windows/printing.rs:243), [History.find](/Users/datin/Documents/Kerjaan/Personal/fxPrint/src-tauri/src/infrastructure/persistence/history_store.rs:121).

Tentukan OS/browser/printer/media yang didukung dan uji kombinasi tersebut. Batasi klaim platform sesuai artifact serta hasil pengujian. Tambahkan resource budget berdasarkan pengukuran; jangan langsung menambahkan queue/microservices/database tanpa kebutuhan yang terbukti.

## Kesiapan pengalaman pengembang dan bisnis

Setup checklist, preflight, sample request, docs interaktif, dan callback sudah merupakan nilai produk. Agar pembeli dapat memakai sendiri, masih perlu bukti berikut:

1. Panduan dari install hingga first print untuk pengguna baru; mencakup token, origin, printer, published preset, dan diagnosis koneksi.
2. Contoh runnable JavaScript dan server dalam topologi yang didukung; contoh retry mempertahankan key dan menangani 202/409/429 serta state hasil tidak diketahui.
3. Kebijakan versi/kompatibilitas API dan migration/update. Error code stabil yang dapat diproses aplikasi lebih baik daripada bergantung pada teks message.
4. Dokumentasi arti success: accepted/queued, spooler completed, dan physical output tidak boleh disamakan.
5. Penjelasan penyimpanan body/PDF, retention, backup/restore, remote credentials, dan akses perangkat. Permissions lokal sudah membantu; encryption-at-rest bukan kewajiban universal untuk semua pelanggan, tetapi kebutuhan customer sensitif harus ditentukan.
6. Kanal dukungan, prosedur laporan bug dengan log yang tidak membocorkan token/body, kebijakan refund, dan hak penggunaan produk.
7. Keputusan harga/unit lisensi dan bukti willingness-to-pay. Tidak ditemukan dalam repo tidak berarti belum ada di luar repo. License activation/DRM bukan syarat teknis wajib untuk mulai menjual.

Virtual printer browser, Finder/Explorer menu, Office conversion, dan animasi tambahan bukan prasyarat untuk menjual print bridge API yang scope-nya jelas. Adanya incoming flow atau source enum belum membuktikan integrasi OS end-to-end siap dijual.

## Urutan menuju rilis umum

1. Bekukan scope dan commit kandidat rilis. Jangan menguji satu checkout lalu mendistribusikan checkout lain.
2. Tutup R1–R4, terutama remote retry dan crash recovery. Jika remote belum aman, jangan memasukkannya dalam fitur yang dijamin pada rilis pertama.
3. Tutup R5–R6 serta R8: lisensi, signing/distribusi, gate hijau.
4. Buktikan R7–R9 dengan matriks browser/OS/printer, failure injection, dan beban representatif.
5. Uji instalasi dan first-print tanpa bantuan dengan pengembang baru.
6. Lengkapi paket komersial dan proses dukungan sebelum membuka pembelian umum.

## Kriteria GO yang diusulkan

Angka berikut adalah target penerimaan yang diusulkan, belum merupakan hasil ix.print:

- Semua gate CI hijau pada commit dan artifact yang sama; tidak ada temuan prioritas tinggi terbuka untuk fitur yang dijual.
- Nol duplicate spool submission pada skenario logical retry yang diuji; hasil ambigu diberi status eksplisit dan tidak otomatis dicetak ulang.
- Nol false success pada failure path driver/spooler yang disimulasikan dan diuji.
- Retention dijalankan pada aplikasi yang terus hidup, tanpa restart.
- Install/upgrade/uninstall lulus pada setiap OS yang dijanjikan; signature dan notice dalam artifact diverifikasi.
- Alur HTTPS browser lulus pada browser yang dijanjikan, dengan panduan penanganan permission/network error.
- Lima pengembang baru; minimal empat menyelesaikan install → first print dalam 15 menit hanya memakai dokumentasi. Semua blocker yang ditemukan ditutup sebelum launch.
- Soak test minimal 72 jam dan beban job representatif; laporkan latency, memory, growth disk, dan error rate. Durasi ini mendeteksi masalah operasional awal, tidak membuktikan reliability jangka panjang sendirian.
- Minimal tiga calon pembeli memvalidasi kebutuhan, paket, dan harga melalui komitmen yang konkret.

## Kesimpulan

ix.print sudah memiliki fondasi produk yang nyata. Keputusan rilis umum sekarang tetap **NO-GO** karena reliability, recovery, distribusi, dan integrasi pengguna akhir belum memenuhi bukti yang diperlukan. Prioritas berikutnya adalah menutup risiko yang ditemukan dan memverifikasi jalur pelanggan dari download hingga cetak; penambahan fitur baru tidak menyelesaikan gap tersebut.
