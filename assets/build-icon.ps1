# The original vector artwork is the single source for every Windows icon size.
# System.Drawing is used only during asset generation, never by the application.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
[xml]$source = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'sc-mining-assist.svg') -Raw
$images = @()
foreach ($size in @(16,24,32,48,64,128,256)) {
    $canvas = [System.Drawing.Bitmap]::new($size * 4,$size * 4)
    $g = [System.Drawing.Graphics]::FromImage($canvas)
    $bitmap = [System.Drawing.Bitmap]::new($size,$size)
    $scaled = [System.Drawing.Graphics]::FromImage($bitmap)
    $stream = [System.IO.MemoryStream]::new()
    try {
        $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $g.ScaleTransform(($size * 4 / 256),($size * 4 / 256))
        foreach ($polygon in $source.svg.polygon) {
            $points = [System.Drawing.PointF[]]@(foreach ($pair in ($polygon.points -split '\s+')) {
                $xy = $pair -split ','
                [System.Drawing.PointF]::new([single]::Parse($xy[0],[cultureinfo]::InvariantCulture),[single]::Parse($xy[1],[cultureinfo]::InvariantCulture))
            })
            $brush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml($polygon.fill))
            try { $g.FillPolygon($brush,$points) } finally { $brush.Dispose() }
        }
        $scaled.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $scaled.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $scaled.DrawImage($canvas,0,0,$size,$size)
        $bitmap.Save($stream,[System.Drawing.Imaging.ImageFormat]::Png)
        $images += @{Size=$size;Bytes=$stream.ToArray()}
        if ($size -eq 256) { [System.IO.File]::WriteAllBytes((Join-Path $PSScriptRoot 'sc-mining-assist.png'),$stream.ToArray()) }
    } finally { $stream.Dispose(); $scaled.Dispose(); $bitmap.Dispose(); $g.Dispose(); $canvas.Dispose() }
}
$file = [System.IO.File]::Create((Join-Path $PSScriptRoot 'sc-mining-assist.ico'))
$writer = [System.IO.BinaryWriter]::new($file)
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$images.Count)
    $offset = 6 + 16 * $images.Count
    foreach ($entry in $images) {
        $encoded = if ($entry.Size -eq 256) { 0 } else { $entry.Size }
        $writer.Write([byte]$encoded); $writer.Write([byte]$encoded)
        $writer.Write([byte]0); $writer.Write([byte]0)
        $writer.Write([uint16]1); $writer.Write([uint16]32)
        $writer.Write([uint32]$entry.Bytes.Length); $writer.Write([uint32]$offset)
        $offset += $entry.Bytes.Length
    }
    foreach ($entry in $images) { $writer.Write([byte[]]$entry.Bytes) }
} finally { $writer.Dispose() }
Write-Host 'Generated sc-mining-assist.png and sc-mining-assist.ico (16–256 px) from sc-mining-assist.svg.'
