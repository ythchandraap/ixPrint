# Manual install and update procedure

ix.print has no auto-update channel. This procedure is the intended manual
process; install, upgrade, uninstall, and user-data preservation still require
the real-installer checks in the
[verification matrix](verification-matrix.md). No public release download is
available from this checkout. The build workflow currently creates private
workflow artifacts only.

## Before installing or updating

1. Obtain the installer for the matching OS and architecture from the
   designated release page. Download its `artifact-manifest.txt` from the same
   release.
2. Check that the manifest names the expected version, commit, and target, then
   compare the installer SHA-256 with its manifest entry.
   - Windows PowerShell: `Get-FileHash .\ix.print.msi -Algorithm SHA256`
   - macOS: `shasum -a 256 ix.print.dmg`
   - Linux: `sha256sum ix.print.deb` or `sha256sum ix.print.AppImage`
3. Confirm the installer signature using the OS signature UI/tooling. Do not
   bypass an invalid signature or macOS notarization warning.
4. Quit ix.print. Back up the application data directory before an update or
   uninstall.

The application data directory is Tauri `app_data_dir()/ixPrint` because the
bundle identifier is `ixPrint`. Default locations are:

- Windows: `%APPDATA%\ixPrint`
- macOS: `~/Library/Application Support/ixPrint`
- Linux: `$XDG_DATA_HOME/ixPrint`, or `~/.local/share/ixPrint` when
  `XDG_DATA_HOME` is unset

This directory contains `config.json` and backup, presets and tokens, History
metadata and retained PDF/JSON content, remote definitions, and logs. It can
contain sensitive print data. Keep backups access-controlled. To restore, quit
ix.print and restore the backed-up directory to the same location.

## Install or upgrade by platform

- **Windows:** run the signed MSI or NSIS installer for Windows x64. For an
  upgrade, use the same installer type as the existing installation where
  possible. Verify the installed version in Windows **Installed apps** or the
  executable's file properties.
- **macOS:** open the signed and notarized DMG, move ix.print into Applications
  (replace the older app when upgrading), and launch it. Verify version in
  Finder **Get Info** for the installed app.
- **Linux DEB:** install or upgrade the checksum-verified DEB using the distribution's
  package installer (for example, `sudo apt install ./ix.print.deb`). Verify
  version in the package manager.
- **Linux AppImage:** make the downloaded file executable and run it. For an
  update, keep the previous AppImage until the new one launches and passes the
  local first-print check. Compare the file's checksum to the matching
  manifest; do not infer version from a filename alone.

After updating, open ix.print, verify the expected presets and printer
configuration, and perform a deliberate test print. If app data appears
missing, quit the app and restore the backup before creating new configuration
or printing jobs.

## Uninstall

Use Windows **Installed apps**, remove ix.print from Applications on macOS, or
use the Linux package manager/remove the AppImage. Keep the `ixPrint` app data
directory if settings, presets, tokens, or retained History need to be
preserved. Delete that directory only when intentionally removing all local
ix.print data, after making any required backup.

The exact uninstaller's treatment of application data is **not verified** on
these OS targets. The uninstall checks must establish whether that directory
is retained before these instructions can serve as a general-release support
guarantee.
