param([Parameter(Mandatory=$true)][string]$ImagePath, [string]$Exe)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (-not $Exe) { $Exe = Join-Path $root 'dist\SC-MiningAssist.exe' }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$output = Join-Path $root 'artifacts'
New-Item -ItemType Directory -Force -Path $output | Out-Null
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile((Resolve-Path -LiteralPath $ImagePath))
try {
    $cases = @(
        @{ Name='original'; Width=5120; Height=2880; X=0; Y=0 },
        @{ Name='head-left'; Width=5120; Height=2880; X=-850; Y=0 },
        @{ Name='head-right-down'; Width=5120; Height=2880; X=850; Y=450 },
        @{ Name='1440p'; Width=2560; Height=1440; X=0; Y=0 },
        @{ Name='1080p'; Width=1920; Height=1080; X=0; Y=0 }
    )
    $reports = foreach ($case in $cases) {
        $bitmap = [System.Drawing.Bitmap]::new($case.Width, $case.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear([System.Drawing.Color]::FromArgb(5,38,35))
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($source, $case.X, $case.Y, $case.Width, $case.Height)
            $fixture = Join-Path $output ($case.Name + '.png')
            $bitmap.Save($fixture, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
        $report = Join-Path $output ($case.Name + '.json')
        $arguments = '--ocr-file "' + $fixture + '" --output "' + $report + '"'
        $process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -Wait
        $data = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
        if ($process.ExitCode -ne 0 -or $data.candidates.Count -ne 1 -or $data.candidates[0].value -ne 10800 -or $data.tracking_signature -ne 10800) {
            throw ($case.Name + ': expected one signature 10800. ' + ($data | ConvertTo-Json -Compress -Depth 4))
        }
        [pscustomobject]@{ Case=$case.Name; Signature=$data.candidates[0].value; SearchMs=$data.elapsed_ms; TrackingMs=$data.tracking_ms }
    }
    $reports | Format-Table
    foreach ($case in @(
        @{Name='1080p-crisp'; Text='10,800'; Expected=10800},
        @{Name='1080p-savril'; Text='6,400'; Expected=6400},
        @{Name='salvage-one'; Text='2,000'; Expected=2000; Panels=1},
        @{Name='salvage-four'; Text='8,000'; Expected=8000; Panels=4},
        @{Name='salvage-thirteen'; Text='26,000'; Expected=26000; Panels=13},
        @{Name='salvage-collision'; Text='18,000'; Expected=18000; Panels=9; Collision=$true},
        @{Name='salvage-only'; Text='18,000'; Expected=18000; Panels=9; Mode='salvage'},
        @{Name='mining-only'; Text='8,000'; Expected=0; Mode='mining'},
        @{Name='salvage-distance'; Text='8,000 km'; Expected=0},
        @{Name='distance'; Text='10,800 km'; Expected=0},
        @{Name='rounded'; Text='10.8K'; Expected=0},
        @{Name='empty'; Text=''; Expected=0}
    )) {
        $bitmap = [System.Drawing.Bitmap]::new(1920,1080)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $font = [System.Drawing.Font]::new('Segoe UI',24,[System.Drawing.FontStyle]::Regular,[System.Drawing.GraphicsUnit]::Pixel)
        $brush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255,228,135))
        try {
            $graphics.Clear([System.Drawing.Color]::FromArgb(5,38,35))
            $graphics.DrawString($case.Text,$font,$brush,940,350)
            $fixture = Join-Path $output ($case.Name+'.png')
            $bitmap.Save($fixture,[System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $font.Dispose(); $brush.Dispose(); $graphics.Dispose(); $bitmap.Dispose() }
        $report = Join-Path $output ($case.Name+'.json')
        $mode = if ($case.Mode) { $case.Mode } else { 'auto' }
        $process = Start-Process -FilePath $exe -ArgumentList ('--ocr-file "'+$fixture+'" --output "'+$report+'" --mode '+$mode) -WindowStyle Hidden -Wait -PassThru
        $data = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
        if ($process.ExitCode -ne 0) { throw 'OCR failed' }
        if ($case.Expected -eq 0) {
            if ($data.candidates.Count -ne 0) { throw ($case.Name+': false positive') }
        } elseif ($data.candidates.Count -ne 1 -or $data.candidates[0].value -ne $case.Expected) { throw ($case.Name+': wrong detection '+($data | ConvertTo-Json -Depth 4 -Compress)) }
        if ($case.Panels) {
            $panels = @($data.matches | Where-Object { $_.kind -eq 'Panel' -and $_.name -eq 'Salvage' })
            if ($data.tracking_signature -ne $case.Expected -or $panels.Count -ne 1 -or $panels[0].count -ne $case.Panels) { throw ($case.Name+': salvage classification or tracking failed') }
            if ($case.Collision -and -not ($data.matches | Where-Object { $_.name -eq 'Bexalite' -and $_.count -eq 5 })) { throw 'Mining/salvage ambiguity was lost' }
            if ($case.Mode -eq 'salvage' -and $data.matches.Count -ne 1) { throw 'Salvage filter failed' }
        }
        Write-Host ($case.Name+': OK ('+$data.elapsed_ms+' ms)')
    }
} finally { $source.Dispose() }
