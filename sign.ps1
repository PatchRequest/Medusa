#cargo make

# Cert paths
$certFolder = Join-Path $PSScriptRoot "certs"
New-Item -ItemType Directory -Force -Path $certFolder | Out-Null

$pfxPassword = ConvertTo-SecureString -String "test1234" -Force -AsPlainText
$certSubject = "CN=RustDriverTestCert"
$pfxPath = Join-Path $certFolder "driver_cert.pfx"
$cerPath = Join-Path $certFolder "driver_cert.cer"
$sysFile = Join-Path $PSScriptRoot "target\debug\medusa.sys"
$signtoolPath = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe"

# Generate certificate
Write-Host "Generating self-signed certificate..."
$cert = New-SelfSignedCertificate `
    -Subject $certSubject `
    -Type CodeSigning `
    -CertStoreLocation "Cert:\LocalMachine\My" `
    -KeyExportPolicy Exportable `
    -KeySpec Signature `
    -HashAlgorithm SHA256

# Export to PFX
Write-Host "Exporting to PFX..."
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $pfxPassword

# Import to CurrentUser
Write-Host "Importing to CurrentUser..."
$importedCert = Import-PfxCertificate -FilePath $pfxPath `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -Password $pfxPassword

# Export CER and trust it
Export-Certificate -Cert $importedCert -FilePath $cerPath
#Import-Certificate -FilePath $cerPath -CertStoreLocation "Cert:\CurrentUser\Root"
#Import-Certificate -FilePath $cerPath -CertStoreLocation "Cert:\CurrentUser\TrustedPublisher"

# Sign the .sys
$thumb = ($importedCert.Thumbprint).Replace(" ", "")
Write-Host "Signing driver: $sysFile"
& "$signtoolPath" sign `
    /fd SHA256 `
    /td SHA256 `
    /tr http://timestamp.digicert.com `
    /sha1 $thumb `
    "$sysFile"

# Verify
Write-Host "`nSignature Verification:"
Get-AuthenticodeSignature "$sysFile"
