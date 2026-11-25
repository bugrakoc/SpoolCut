$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = Join-Path $env:RUNNER_TEMP "SpoolCut portable ü $([guid]::NewGuid())"
$oldPath = $env:PATH
try {
    Expand-Archive -LiteralPath dist-windows/SpoolCut-Windows-x86_64.zip -DestinationPath $root
    $bin = Join-Path $root 'SpoolCut/bin'
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:GTK_A11Y = 'none'
    $gui = Start-Process -FilePath (Join-Path $bin 'SpoolCut.exe') -ArgumentList '--help' -PassThru
    try {
        if (!$gui.WaitForExit(15000)) { throw 'SpoolCut.exe --help hung' }
        if ($gui.ExitCode -ne 0) { throw "SpoolCut.exe exited with $($gui.ExitCode)" }
    } finally {
        if (!$gui.HasExited) { $gui.Kill() }
        $gui.Dispose()
    }

    $testExe = (Get-Content -LiteralPath windows-desktop-test.txt -Raw).Trim()
    Copy-Item -LiteralPath $testExe -Destination (Join-Path $bin 'desktop-smoke.exe')
    Push-Location $root
    try {
        & (Join-Path $bin 'ffmpeg.exe') -nostdin -v error -f lavfi -i 'testsrc2=size=160x96:rate=25:duration=2' -f lavfi -i 'sine=sample_rate=48000:duration=2' -c:v libx264 -tag:v avc1 -x264-params 'keyint=16:min-keyint=16:scenecut=0:open-gop=0' -c:a aac 'test ç video.mp4'
        $cli = Join-Path $bin 'spoolcut-cli.exe'
        $media = (& $cli inspect 'test ç video.mp4' | Out-String) | ConvertFrom-Json
        $project = @{
            sources = @{camera = 'test ç video.mp4'}
            clips = @(@{source = 'camera'; in_pts = $media.safe_in_pts[0]; out_pts = $media.safe_in_pts[1]})
        }
        $project | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8NoBOM 'project.json'
        & $cli export 'project.json' 'out ü.mkv'
        & (Join-Path $bin 'ffprobe.exe') -v error -show_entries 'stream=codec_name' 'out ü.mkv'
        $env:PATH = "$bin;$env:PATH"
        $env:SPOOLCUT_PREVIEW = 'software'
        $env:GSK_RENDERER = 'cairo'
        & (Join-Path $bin 'desktop-smoke.exe') --ignored --nocapture
    } finally {
        Pop-Location
    }
} finally {
    $env:PATH = $oldPath
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
