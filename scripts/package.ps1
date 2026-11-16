param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    if (-not $NoBuild) { & (Join-Path $root 'build.ps1') }
    $manifest = Get-Content -LiteralPath Cargo.toml -Raw
    $version = [regex]::Match($manifest,'(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?)"').Groups[1].Value
    if (-not $version) { throw 'Cannot determine the release version from Cargo.toml.' }
    $names = @('MiningMod.exe','signatures.json','README.md','CHANGELOG.md','LICENSE','THIRD_PARTY_NOTICES.txt')
    $files = @($names | ForEach-Object {
        $path = Join-Path $root ('dist/' + $_)
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing release file: $_" }
        $path
    })
    $archiveName = "MiningMod-v$version-win64.zip"
    $archivePath = Join-Path $root ('dist/' + $archiveName)
    # Explicit allowlist: no old ZIPs, debug output or local screenshots enter the package.
    Compress-Archive -LiteralPath $files -DestinationPath $archivePath -CompressionLevel Optimal -Force
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        if ($zip.Entries.Count -ne $names.Count) { throw 'Unexpected number of archive entries.' }
        foreach ($name in $names) {
            $entry = $zip.GetEntry($name)
            if ($null -eq $entry) { throw "Release archive is missing $name" }
            $stream = $entry.Open()
            $sha = [System.Security.Cryptography.SHA256]::Create()
            try {
                $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','')
                $expected = (Get-FileHash -LiteralPath (Join-Path $root ('dist/' + $name)) -Algorithm SHA256).Hash
                if ($actual -ne $expected) { throw "Archive integrity check failed for $name" }
            } finally { $sha.Dispose(); $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
    $hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText((Join-Path $root 'dist/SHA256SUMS.txt'),"$hash  $archiveName`n",[System.Text.UTF8Encoding]::new($false))
    Write-Host "Packaged and verified: $archivePath"
    Write-Host "SHA-256: $hash"
} finally { Pop-Location }
