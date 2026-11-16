# Collect license files verbatim from the dependencies resolved by Cargo.lock.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $localCargo = Join-Path $root '.tools/cargo/bin/cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:CARGO_HOME = Join-Path $root '.tools/cargo'
        $env:RUSTUP_HOME = Join-Path $root '.tools/rustup'
        $cargo = $localCargo
    } else { $cargo = (Get-Command cargo -ErrorAction Stop).Source }
    $raw = & $cargo metadata --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $metadata = $raw | ConvertFrom-Json
    $output = [System.Text.StringBuilder]::new()
    [void]$output.AppendLine('SC-MiningAssist third-party dependencies. Runtime and build dependencies are included below.')
    foreach ($package in ($metadata.packages | Where-Object { $_.name -ne 'sc-mining-assist' } | Sort-Object name,version)) {
        [void]$output.AppendLine()
        [void]$output.AppendLine("$($package.name) $($package.version) ($($package.license))")
        $packageRoot = Split-Path $package.manifest_path -Parent
        $licenses = @(Get-ChildItem -LiteralPath $packageRoot -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)([._-]|$)' } | Sort-Object Name)
        if ($package.license_file) {
            $extra = Get-Item -LiteralPath (Join-Path $packageRoot $package.license_file)
            $licenses = @($licenses + $extra | Sort-Object FullName -Unique)
        }
        if (-not $licenses.Count) { throw "No license text found for $($package.name); review manually before distribution." }
        foreach ($license in $licenses) {
            [void]$output.AppendLine($license.Name)
            [void]$output.AppendLine([System.IO.File]::ReadAllText($license.FullName).TrimEnd())
        }
    }
    [System.IO.File]::WriteAllText((Join-Path $root 'THIRD_PARTY_NOTICES.txt'),$output.ToString().Replace("`r`n","`n"),[System.Text.UTF8Encoding]::new($false))
    Write-Host 'Updated THIRD_PARTY_NOTICES.txt from Cargo.lock dependencies.'
} finally { Pop-Location }
