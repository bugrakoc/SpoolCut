$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = Join-Path $env:RUNNER_TEMP "SpoolCut Qt portable ü $([guid]::NewGuid())"
$oldPath = $env:PATH
try {
    Expand-Archive -LiteralPath dist-qt-windows/SpoolCut-Qt-Windows-x86_64.zip -DestinationPath $root
    $bin = Join-Path $root 'SpoolCut Qt Prototype/bin'
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:SPOOLCUT_QT_TEST_AUDIO_NULL = '1'
    $video = Join-Path $root 'test ç video.mp4'
    & (Join-Path $bin 'ffmpeg.exe') -nostdin -v error -f lavfi -i 'testsrc2=size=320x180:rate=25:duration=2' -f lavfi -i 'sine=sample_rate=48000:duration=2' -c:v libx264 -c:a aac $video
    foreach ($renderer in @('software', 'auto')) {
        $env:SPOOLCUT_QT_PREVIEW = $renderer
        $process = Start-Process -FilePath (Join-Path $bin 'SpoolCutQt.exe') -ArgumentList @('--smoke', ('"' + $video + '"')) -WorkingDirectory $root -PassThru
        try {
            if (!$process.WaitForExit(22000)) { throw "SpoolCutQt $renderer hung" }
            if ($process.ExitCode -ne 0) { throw "SpoolCutQt $renderer exited with $($process.ExitCode)" }
        } finally {
            if (!$process.HasExited) { $process.Kill() }
            $process.Dispose()
        }
    }
} finally {
    $env:PATH = $oldPath
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
