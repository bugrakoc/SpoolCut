$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = Join-Path $env:RUNNER_TEMP "SpoolCut Qt portable ü $([guid]::NewGuid())"
$oldPath = $env:PATH
$version = (Get-Content -LiteralPath VERSION -Raw).Trim()
try {
    Expand-Archive -LiteralPath "dist-qt-windows/SpoolCut-${version}-Qt-Windows-x86_64.zip" -DestinationPath $root
    $bin = Join-Path $root 'SpoolCut Qt/bin'
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:SPOOLCUT_QT_TEST_AUDIO_NULL = '1'
    Remove-Item Env:SPOOLCUT_QT_PREVIEW -ErrorAction SilentlyContinue
    $cli = Join-Path $bin 'spoolcut.exe'
    $ffmpeg = Join-Path $bin 'ffmpeg.exe'
    $desktop = Join-Path $bin 'SpoolCutQt.exe'
    if ((& $cli --version) -ne "SpoolCut $version") { throw "Wrong bundled CLI version" }
    if ((Get-Item -LiteralPath $desktop).VersionInfo.ProductVersion -ne $version) {
        throw "Wrong desktop executable version"
    }
    $avc = Join-Path $root 'test ç AVC.mp4'
    $hevc = Join-Path $root 'test ü HEVC Main 10.mp4'
    & $ffmpeg -nostdin -v error -f lavfi -i 'testsrc2=size=320x180:rate=25:duration=2' -f lavfi -i 'sine=sample_rate=48000:duration=2' -c:v libx264 -c:a aac $avc
    & $ffmpeg -nostdin -v error -f lavfi -i 'testsrc2=size=3840x2160:rate=25:duration=1' -f lavfi -i 'sine=sample_rate=48000:duration=1' -c:v libx265 -preset ultrafast -x265-params 'pools=2:frame-threads=2:log-level=error:open-gop=0' -pix_fmt yuv420p10le -tag:v hvc1 -c:a aac $hevc
    foreach ($video in @($avc, $hevc)) {
        $info = ((& $cli inspect $video) -join [Environment]::NewLine) | ConvertFrom-Json
        $project = "${video}.spoolcut.json"
        $output = "${video}.mkv"
        @{
            sources = @{ A = Split-Path -Leaf $video }
            clips = @(@{ source = 'A'; in_pts = $info.safe_in_pts[0]; out_pts = $info.video_end_pts })
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $project -Encoding utf8NoBOM
        & $cli plan $project
        & $cli export $project $output
        & $ffmpeg -nostdin -v error -xerror -i $output -f null -
        Write-Host "Exported and decoded $output with the relocated CLI and tools"
        $process = Start-Process -FilePath $desktop -ArgumentList @('--smoke', ('"' + $video + '"')) -WorkingDirectory $root -PassThru
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
