param([switch]$Test, [switch]$Check, [switch]$Lint, [switch]$Format)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    $localCargo = Join-Path $PSScriptRoot '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:CARGO_HOME = Join-Path $PSScriptRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.tools\rustup'
        $cargo = $localCargo
    } else { $cargo = (Get-Command cargo -ErrorAction Stop).Source }
    # Recover the existing Build Tools even when VS Installer marked them incomplete.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
        $installations = & $vswhere -products '*' -all -property installationPath
        $vc = $installations | ForEach-Object {
            $toolRoot = Join-Path $_ 'VC\Tools\MSVC'
            if (Test-Path -LiteralPath $toolRoot) { Get-ChildItem -LiteralPath $toolRoot -Directory }
        } | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'bin\Hostx64\x64\link.exe') } | Sort-Object Name -Descending | Select-Object -First 1
        $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib'
        $sdk = if (Test-Path -LiteralPath $sdkRoot) {
            Get-ChildItem -LiteralPath $sdkRoot -Directory | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'um\x64\kernel32.lib') } | Sort-Object Name -Descending | Select-Object -First 1
        }
        if ($vc -and $sdk) {
            $env:PATH = (Join-Path $vc.FullName 'bin\Hostx64\x64') + ';' + $env:PATH
            $env:LIB = (Join-Path $vc.FullName 'lib\x64') + ';' + (Join-Path $sdk.FullName 'um\x64') + ';' + (Join-Path $sdk.FullName 'ucrt\x64')
        }
    }
    if ($Format) { & $cargo fmt --all -- --check } elseif ($Lint) { & $cargo clippy --locked --all-targets -- -D warnings } elseif ($Check) { & $cargo check --locked } elseif ($Test) { & $cargo test --locked } else {
        & $cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
        New-Item -ItemType Directory -Force -Path dist | Out-Null
        Copy-Item -LiteralPath target\release\sc-mining-assist.exe -Destination dist\SC-MiningAssist.exe
        Copy-Item -LiteralPath data\signatures.json -Destination dist
        Copy-Item -LiteralPath LICENSE -Destination dist
        if (Test-Path THIRD_PARTY_NOTICES.txt) { Copy-Item -LiteralPath THIRD_PARTY_NOTICES.txt -Destination dist }
        Copy-Item -LiteralPath docs\PORTABLE.md -Destination dist\README.md
        Copy-Item -LiteralPath CHANGELOG.md -Destination dist
        Write-Host ('Ready: ' + (Join-Path $PSScriptRoot 'dist\SC-MiningAssist.exe'))
    }
    if ($LASTEXITCODE -ne 0) { throw 'Check failed.' }
} finally { Pop-Location }
