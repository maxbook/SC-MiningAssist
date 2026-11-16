param([string]$Exe = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\release\miningmod.exe'))
$ErrorActionPreference='Stop'
$Exe=(Resolve-Path -LiteralPath $Exe).Path
$output=Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts'
New-Item -ItemType Directory -Path $output -Force | Out-Null
Add-Type -AssemblyName System.Drawing
function Fixture([string]$Name,[string]$Text,[int]$X=940,[int]$Y=350,[int]$Brightness=255,[int]$FontSize=24,[string]$Second='') {
    $bitmap=[System.Drawing.Bitmap]::new(1920,1080)
    $g=[System.Drawing.Graphics]::FromImage($bitmap)
    $font=[System.Drawing.Font]::new('Segoe UI',$FontSize,[System.Drawing.FontStyle]::Regular,[System.Drawing.GraphicsUnit]::Pixel)
    $brush=[System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb($Brightness,[int]($Brightness*0.89),[int]($Brightness*0.53)))
    try {
        $g.Clear([System.Drawing.Color]::FromArgb(5,38,35))
        $g.DrawString($Text,$font,$brush,$X,$Y)
        if ($Second) { $g.DrawString($Second,$font,$brush,1150,400) }
        $path=Join-Path $output ('tracking-'+$Name+'.png')
        $bitmap.Save($path,[System.Drawing.Imaging.ImageFormat]::Png)
        return $path
    } finally {$brush.Dispose();$font.Dispose();$g.Dispose();$bitmap.Dispose()}
}
$empty=Fixture 'empty' ''
$first=Fixture 'first' '10,800'
$move=Fixture 'move' '10,800' 990 365
$move2=Fixture 'move2' '10,800' 1030 380
$changed=Fixture 'changed' '6,400' 1030 380
$dim=Fixture 'dim' '6,400' 1030 380 115
$coldDim=Fixture 'cold-dim' '10,800' 940 350 115
$tiny=Fixture 'tiny' '10,800' 940 350 215 12
$distance=Fixture 'distance' '10,800 km'
$ambiguous=Fixture 'ambiguous' '10,800' 940 350 255 24 '6,400'
function Run-Sequence([string]$Name,$Steps) {
    $manifest=Join-Path $output ($Name+'.sequence.json')
    $report=Join-Path $output ($Name+'.tracking.json')
    $Steps | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifest
    $p=Start-Process -FilePath $Exe -ArgumentList ('--ocr-sequence "'+$manifest+'" --output "'+$report+'"') -WindowStyle Hidden -Wait -PassThru
    $data=Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if ($p.ExitCode -ne 0 -or $data.error) {throw ($data | ConvertTo-Json -Compress)}
    return $data.frames
}
$frames=Run-Sequence 'motion' @(
    @{image=$empty;at_ms=0}, @{image=$empty;at_ms=100},
    @{image=$first;at_ms=1000}, @{image=$move;at_ms=1250},
    @{image=$empty;at_ms=1500}, @{image=$move2;at_ms=1750},
    @{image=$changed;at_ms=2000}, @{image=$changed;at_ms=2250},
    @{image=$dim;at_ms=2500}, @{image=$empty;at_ms=3000},
    @{image=$empty;at_ms=8000}, @{image=$empty;at_ms=8100},
    @{image=$distance;at_ms=9000}, @{image=$ambiguous;at_ms=10000}
)
if ($frames[1].stage -ne 'skipped' -or $frames[1].captures -ne 0) {throw 'Scheduled idle tick captured the screen or emitted an OCR failure.'}
foreach ($index in @(3,4,5)) { if ($frames[$index].stable -ne 10800) {throw ('Tracking lost at '+$frames[$index].at_ms+' ms')} }
if ($frames[3].stage -notlike 'local*' -or $frames[5].stage -notlike 'local*') {throw 'Local tracking waited for the global search.'}
if ($null -ne $frames[6].stable -or $frames[7].stable -ne 6400 -or $frames[8].stable -ne 6400) {throw 'Target change or dim reading failed.'}
if ($null -ne $frames[10].stable -or $frames[11].stage -ne 'skipped') {throw 'Lost target did not expire.'}
if ($null -ne $frames[12].raw -or -not $frames[13].ambiguous) {throw 'Distance or multiple signals were handled incorrectly.'}
foreach ($case in @(@{name='cold-dim';image=$coldDim},@{name='tiny';image=$tiny})) {
    $result=Run-Sequence $case.name @(@{image=$case.image;at_ms=0},@{image=$case.image;at_ms=250})
    if ($result[0].raw -ne 10800 -or $result[1].stable -ne 10800) {throw ($case.name+': acquisition failed')}
    Write-Host ($case.name+': confirmed at 250 ms simulated time; initial search '+$result[0].elapsed_ms+' ms')
}
$frames | Select-Object at_ms,raw,stable,stage,elapsed_ms,captures | Format-Table
Write-Host 'OK: acquisition, motion, brief miss, target change, dim text, expiry, distances and ambiguity.'

# A signal appearing just after a wide scan must not wait for a one-second timer.
$appearance = Run-Sequence 'appearance' @(
    @{image=$empty;at_ms=0}, @{image=$first;at_ms=125},
    @{image=$first;at_ms=250}, @{image=$first;at_ms=375},
    @{image=$first;at_ms=500}, @{image=$first;at_ms=750},
    @{image=$first;at_ms=1000}, @{image=$first;at_ms=1125}
)
$confirmed = @($appearance | Where-Object { $_.stable -eq 10800 })
if ($confirmed.Count -eq 0 -or $confirmed[0].at_ms -gt 375) {throw 'New signal acquisition is too slow'}

# Keep context around an already tracked number as its width or suffix changes.
$jump = Fixture 'jump' '6,400' 1420 600
$jumped = Run-Sequence 'jump' @(
    @{image=$first;at_ms=0}, @{image=$first;at_ms=125},
    @{image=$jump;at_ms=250}, @{image=$jump;at_ms=375}
)
if($jumped[2].raw -ne 6400 -or $jumped[3].stable -ne 6400) {throw 'Large head movement did not reacquire within two reads'}

$distanceAfter = Fixture 'distance-after' '10,800 km' 940 350
$longer = Fixture 'longer' '108000' 940 350
$context = Run-Sequence 'context' @(
    @{image=$first;at_ms=0}, @{image=$first;at_ms=125},
    @{image=$distanceAfter;at_ms=250}, @{image=$distanceAfter;at_ms=375},
    @{image=$longer;at_ms=500}, @{image=$longer;at_ms=625}
)
foreach($index in @(2,3)) {
    if($null -ne $context[$index].raw -or $null -ne $context[$index].stable) {
        throw 'Tracked distance accepted or stale signature retained after explicit unit'
    }
}
foreach($index in @(4,5)) {if($context[$index].raw -eq 10800) {throw 'Longer number was truncated to the old signature'}}

# Replay real cockpit images at several resolutions and large head-look offsets.
$replayed = 0
foreach($name in @('original','head-left','head-right-down','1440p','1080p')) {
    $path=Join-Path $output ($name+'.png')
    if(Test-Path -LiteralPath $path) {
        $result=Run-Sequence ('real-'+$name) @(@{image=$path;at_ms=0},@{image=$path;at_ms=125},@{image=$path;at_ms=250})
        if($result[0].raw -ne 10800 -or $result[1].stable -ne 10800) {throw ($name+': live tracker failed on real cockpit')}
        Write-Host ($name+': acquisition '+$result[0].elapsed_ms+' ms; tracking '+$result[1].elapsed_ms+' ms')
        $replayed++
    }
}
Write-Host "OK: new-signal latency, tracked suffix/width changes; $replayed optional cockpit fixtures replayed."
