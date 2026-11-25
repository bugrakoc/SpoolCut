$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = Join-Path $env:RUNNER_TEMP "SpoolCut Qt portable ü $([guid]::NewGuid())"
$oldPath = $env:PATH
try {
    Expand-Archive -LiteralPath dist-qt-windows/SpoolCut-Qt-Windows-x86_64.zip -DestinationPath $root
    $bin = Join-Path $root 'SpoolCut Qt Prototype/bin'
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:SPOOLCUT_QT_TEST_AUDIO_NULL = '1'
    Remove-Item Env:SPOOLCUT_QT_PREVIEW -ErrorAction SilentlyContinue
    $ffmpeg = Join-Path $bin 'ffmpeg.exe'
    $avc = Join-Path $root 'test ç AVC.mp4'
    $hevc = Join-Path $root 'test ü HEVC Main 10.mp4'
    & $ffmpeg -nostdin -v error -f lavfi -i 'testsrc2=size=320x180:rate=25:duration=2' -f lavfi -i 'sine=sample_rate=48000:duration=2' -c:v libx264 -c:a aac $avc
    & $ffmpeg -nostdin -v error -f lavfi -i 'testsrc2=size=3840x2160:rate=25:duration=1' -f lavfi -i 'sine=sample_rate=48000:duration=1' -c:v libx265 -preset ultrafast -x265-params 'pools=2:frame-threads=2:log-level=error' -pix_fmt yuv420p10le -tag:v hvc1 -c:a aac $hevc
    foreach ($video in @($avc, $hevc)) {
        $process = Start-Process -FilePath (Join-Path $bin 'SpoolCutQt.exe') -ArgumentList @('--smoke', ('"' + $video + '"')) -WorkingDirectory $root -PassThru
        try {
            if (!$process.WaitForExit(22000)) { throw "SpoolCutQt hung opening $video" }
            if ($process.ExitCode -ne 0) { throw "SpoolCutQt exited with $($process.ExitCode) opening $video" }
            Write-Host "Opened $video with the packaged Windows default renderer"
        } finally {
            if (!$process.HasExited) { $process.Kill() }
            $process.Dispose()
        }
    }
} finally {
    $env:PATH = $oldPath
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
