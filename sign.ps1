#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Generates a self-signed certificate and signs the Medusa driver.

.PARAMETER BuildProfile
    Build profile directory name (default: "debug").

.PARAMETER PfxPassword
    Password for the PFX certificate (default: "test1234").
#>

param(
    [string]$BuildProfile = "debug",
    [string]$PfxPassword  = "test1234"
)

$ErrorActionPreference = "Stop"

# Cert paths
$certFolder = Join-Path $PSScriptRoot "certs"
New-Item -ItemType Directory -Force -Path $certFolder | Out-Null

$securePassword = ConvertTo-SecureString -String $PfxPassword -Force -AsPlainText
$certSubject    = "CN=MedusaDriverCert"
$pfxPath        = Join-Path $certFolder "driver_cert.pfx"
$cerPath        = Join-Path $certFolder "driver_cert.cer"
$sysFile        = Join-Path $PSScriptRoot "target\$BuildProfile\medusa.sys"

# Find signtool.exe automatically
$signtoolPath = $null
$wdkRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
if (Test-Path $wdkRoot) {
    $signtoolPath = Get-ChildItem -Path $wdkRoot -Recurse -Filter "signtool.exe" |
        Where-Object { $_.FullName -match "x64" } |
        Sort-Object { $_.Directory.Name } -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}

if (-not $signtoolPath) {
    Write-Error "signtool.exe not found. Install the Windows SDK or WDK."
    exit 1
}

Write-Host "[+] Using signtool: $signtoolPath"

# Verify .sys file exists
if (-not (Test-Path $sysFile)) {
    Write-Error "Driver not found at $sysFile. Build first with 'cargo make'."
    exit 1
}

# Generate self-signed certificate
Write-Host "[*] Generating self-signed certificate..."
$cert = New-SelfSignedCertificate `
    -Subject $certSubject `
    -Type CodeSigning `
    -CertStoreLocation "Cert:\LocalMachine\My" `
    -KeyExportPolicy Exportable `
    -KeySpec Signature `
    -HashAlgorithm SHA256

# Export to PFX
Write-Host "[*] Exporting to PFX..."
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $securePassword

# Import to CurrentUser
Write-Host "[*] Importing to CurrentUser..."
$importedCert = Import-PfxCertificate -FilePath $pfxPath `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -Password $securePassword

# Export CER
Export-Certificate -Cert $importedCert -FilePath $cerPath

# Sign the .sys
$thumb = ($importedCert.Thumbprint).Replace(" ", "")
Write-Host "[+] Signing driver: $sysFile"
& "$signtoolPath" sign `
    /fd SHA256 `
    /td SHA256 `
    /tr http://timestamp.digicert.com `
    /sha1 $thumb `
    "$sysFile"

if ($LASTEXITCODE -ne 0) {
    Write-Error "Signing failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

# Verify
Write-Host "`n[+] Signature Verification:"
Get-AuthenticodeSignature "$sysFile"
Write-Host "[+] Done!"
