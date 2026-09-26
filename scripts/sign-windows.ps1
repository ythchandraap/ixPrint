param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$File
)

if ([string]::IsNullOrWhiteSpace($env:WINDOWS_PFX_PATH) -or [string]::IsNullOrWhiteSpace($env:WINDOWS_PFX_PASSWORD)) {
  Write-Host "Skipping Windows code signing outside a configured release workflow."
  exit 0
}

$signtool = Get-Command signtool.exe -ErrorAction Stop
& $signtool.Source sign /fd SHA256 /tr "http://timestamp.digicert.com" /td SHA256 /f $env:WINDOWS_PFX_PATH /p $env:WINDOWS_PFX_PASSWORD $File
if ($LASTEXITCODE -ne 0) {
  throw "signtool failed to sign $File"
}
