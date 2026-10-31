<#
.SYNOPSIS
The release zip works on a clean Windows machine: no Rust, Python, uv or ffmpeg (roadmap 124000).

.DESCRIPTION
Unzips the release into a scratch folder and runs the app and `ac` from it with a minimal
environment: PATH holds only Windows' own folders, AC_FFMPEG is unset, the home, data and temp
folders point into the scratch, and the helper installs are told to look in empty scratch folders.
Each check prints PASS or FAIL (or SKIP) with its evidence, a log goes to <scratch>\release_check.log,
and the script exits 1 on any FAIL.

The checks:
  1. contents     every path tools/release_manifest.txt requires is in the zip (release_contents.ps1)
  2. version      humanmotioncraft.exe's ProductVersion is the version in the zip's name
  3. clean path   the minimal PATH really has no ffmpeg, python, py, uv, cargo or rustc
  4. app          humanmotioncraft.exe starts, runs a --script that opens a built-in clip, writes a
                  --screenshot PNG and exits 0
  5. ac script    ac.exe runs a script against the shipped assets: session.info, clip.open, a frame,
                  a bone's position, clip.picture (PNG) and a clip.video GIF (no ffmpeg needed)
  6. no ffmpeg    clip.video to .mp4 is refused with a message naming ffmpeg: no crash, no file
  7. text.install  `check` finds the shipped source tree (requirement "Source tree" ok), downloads nothing
  8. posefill.install  the same for pose fill

Runs on Windows PowerShell 5.1 (what Windows Sandbox has) and PowerShell 7, with nothing installed.
For a truly clean machine run it in Windows Sandbox: tools/release_check.wsb, see roadmap/124000-packaging.md.

    pwsh -NoProfile -File tools/release_check.ps1 -Zip dist/humanmotioncraft-0.2.0-windows-x64-portable.zip -Scratch target/release-check

.PARAMETER Zip
The release zip; or a folder holding exactly one .zip; or an already unzipped release folder (the
top folder with humanmotioncraft.exe in it, or the folder holding it).

.PARAMETER Scratch
Where to unzip and where everything the checks write goes. Made if missing; the subfolders this
script owns (release, app-library, cli-library, home, tmp, textmotion, posefill) are emptied first.

.PARAMETER Report
Optional: a folder the log and the pictures are copied to at the end (Windows Sandbox: a mapped
folder, so they outlive the sandbox).

.PARAMETER TimeoutSeconds
How long one run of the app or ac may take (default 240).
#>
param(
    [Parameter(Mandatory = $true)][string]$Zip,
    [Parameter(Mandatory = $true)][string]$Scratch,
    [string]$Report = '',
    [int]$TimeoutSeconds = 240
)

$ErrorActionPreference = 'Stop'
$script:passed = 0
$script:failed = 0
$script:skipped = 0
$script:log = $null

function Out([string]$text) {
    Write-Host $text
    if ($script:log) { Add-Content -LiteralPath $script:log -Value $text -Encoding UTF8 }
}

function Result([string]$status, [string]$check, [string[]]$evidence) {
    Out ("{0} {1}" -f $status, $check)
    foreach ($line in $evidence) {
        if ($null -ne $line -and $line -ne '') {
            foreach ($l in ($line -split "`r?`n")) { if ($l -ne '') { Out ("    " + $l) } }
        }
    }
    switch ($status) {
        'PASS' { $script:passed++ }
        'FAIL' { $script:failed++ }
        'SKIP' { $script:skipped++ }
    }
}

function Tail([string]$text, [int]$lines = 6) {
    if (-not $text) { return '' }
    $all = $text -split "`r?`n" | Where-Object { $_ -ne '' }
    if ($all.Count -gt $lines) { $all = $all | Select-Object -Last $lines }
    return ($all -join "`n")
}

# One argument quoted the way Windows programs (Rust's std included) parse a command line.
function Quote([string]$a) {
    if ($a -ne '' -and $a -notmatch '[\s"]') { return $a }
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.Append('"')
    $backslashes = 0
    foreach ($c in $a.ToCharArray()) {
        if ($c -eq '\') { $backslashes++; continue }
        if ($c -eq '"') {
            [void]$sb.Append('\' * ($backslashes * 2 + 1)).Append('"')
            $backslashes = 0
            continue
        }
        [void]$sb.Append('\' * $backslashes).Append($c)
        $backslashes = 0
    }
    [void]$sb.Append('\' * ($backslashes * 2)).Append('"')
    return $sb.ToString()
}

function Write-Utf8([string]$path, [string]$text) {
    [System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding $false))
}

function Fresh([string]$dir) {
    if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
    [void](New-Item -ItemType Directory -Path $dir -Force)
    return $dir
}

# Run a program with the minimal environment; returns exit code, stdout, stderr, seconds.
function Invoke-Clean([string]$exe, [string[]]$arguments, [hashtable]$environment, [string]$cwd) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = (($arguments | ForEach-Object { Quote $_ }) -join ' ')
    $psi.WorkingDirectory = $cwd
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    $psi.CreateNoWindow = $true
    $psi.EnvironmentVariables.Clear()
    foreach ($k in $environment.Keys) { $psi.EnvironmentVariables[$k] = [string]$environment[$k] }
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Close()
    $outTask = $p.StandardOutput.ReadToEndAsync()
    $errTask = $p.StandardError.ReadToEndAsync()
    $timedOut = $false
    if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
        $timedOut = $true
        try { $p.Kill() } catch { }
        $p.WaitForExit()
    }
    $out = $outTask.Result
    $err = $errTask.Result
    $watch.Stop()
    return @{
        Code = $p.ExitCode
        Out = $out
        Err = $err
        TimedOut = $timedOut
        Seconds = [math]::Round($watch.Elapsed.TotalSeconds, 1)
        Command = (Split-Path -Leaf $exe) + ' ' + $psi.Arguments
    }
}

function Has-Magic([string]$path, [byte[]]$magic) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
    $bytes = New-Object byte[] $magic.Length
    $fs = [System.IO.File]::OpenRead($path)
    try { $n = $fs.Read($bytes, 0, $bytes.Length) } finally { $fs.Dispose() }
    if ($n -ne $magic.Length) { return $false }
    for ($i = 0; $i -lt $magic.Length; $i++) { if ($bytes[$i] -ne $magic[$i]) { return $false } }
    return $true
}
$pngMagic = [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
$gifMagic = [byte[]](0x47, 0x49, 0x46, 0x38)

# JSON lines (`ac --jsonl`): every non-empty stdout line parsed, or $null for a line that isn't JSON.
function Json-Lines([string]$text) {
    $parsed = @()
    foreach ($line in ($text -split "`r?`n")) {
        if ($line.Trim() -eq '') { continue }
        try { $parsed += , (ConvertFrom-Json -InputObject $line) } catch { $parsed += , $null }
    }
    return , $parsed
}

# ---------------------------------------------------------------------------------------------
# the scratch and the log
$Scratch = [System.IO.Path]::GetFullPath((Join-Path (Get-Location).ProviderPath $Scratch))
if (-not (Test-Path -LiteralPath $Scratch)) { [void](New-Item -ItemType Directory -Path $Scratch -Force) }
$script:log = Join-Path $Scratch 'release_check.log'
if (Test-Path -LiteralPath $script:log) { Remove-Item -LiteralPath $script:log -Force }
Out ("release check {0} on {1} ({2}), PowerShell {3}" -f (Get-Date -Format s), $env:COMPUTERNAME, [System.Environment]::OSVersion.VersionString, $PSVersionTable.PSVersion)
Out "scratch: $Scratch"

# the release: a zip, a folder with one zip, or an unzipped folder
$zipFile = $null
$topDir = $null
$Zip = [System.IO.Path]::GetFullPath((Join-Path (Get-Location).ProviderPath $Zip))
if (Test-Path -LiteralPath $Zip -PathType Leaf) {
    $zipFile = $Zip
} elseif (Test-Path -LiteralPath $Zip -PathType Container) {
    if (Test-Path -LiteralPath (Join-Path $Zip 'humanmotioncraft.exe') -PathType Leaf) {
        $topDir = $Zip
    } else {
        $zips = @(Get-ChildItem -LiteralPath $Zip -File -Filter '*.zip')
        $inner = @(Get-ChildItem -LiteralPath $Zip -Directory | Where-Object {
            Test-Path -LiteralPath (Join-Path $_.FullName 'humanmotioncraft.exe') -PathType Leaf })
        if ($zips.Count -eq 1) { $zipFile = $zips[0].FullName }
        elseif ($inner.Count -eq 1) { $topDir = $inner[0].FullName }
        else {
            Out "FAIL release: $Zip holds $($zips.Count) zip(s) and $($inner.Count) release folder(s); give one zip or one unzipped release"
            exit 1
        }
    }
} else {
    Out "FAIL release: not found: $Zip"
    exit 1
}

if ($zipFile) {
    Out "zip: $zipFile ($([math]::Round((Get-Item -LiteralPath $zipFile).Length / 1MB, 1)) MB)"
    $unzipped = Fresh (Join-Path $Scratch 'release')
    Expand-Archive -LiteralPath $zipFile -DestinationPath $unzipped -Force
    $tops = @(Get-ChildItem -LiteralPath $unzipped -Directory)
    if ($tops.Count -ne 1 -or -not (Test-Path -LiteralPath (Join-Path $tops[0].FullName 'humanmotioncraft.exe'))) {
        Result 'FAIL' 'release layout' @("the zip must hold one folder with humanmotioncraft.exe in it; found: " + (($tops | ForEach-Object { $_.Name }) -join ', '))
        Out "FAIL: the remaining checks need the app"
        exit 1
    }
    $topDir = $tops[0].FullName
} else {
    Out "release folder (already unzipped): $topDir"
}
$app = Join-Path $topDir 'humanmotioncraft.exe'
$ac = Join-Path $topDir 'ac.exe'
Out "release folder: $topDir"
Out ''

# ---------------------------------------------------------------------------------------------
# 1. contents
$contentsScript = Join-Path $PSScriptRoot 'release_contents.ps1'
$target = if ($zipFile) { $zipFile } else { $topDir }
$contentsOut = & $contentsScript -Path $target 2>&1 | ForEach-Object { [string]$_ }
$contentsCode = $LASTEXITCODE
if ($contentsCode -eq 0) {
    Result 'PASS' 'contents: every required path is in the release' $contentsOut
} else {
    Result 'FAIL' 'contents: the release is missing required paths (see tools/release_manifest.txt)' $contentsOut
}

# ---------------------------------------------------------------------------------------------
# 2. version
$nameForVersion = if ($zipFile) { [System.IO.Path]::GetFileNameWithoutExtension($zipFile) } else { Split-Path -Leaf $topDir }
$productVersion = (Get-Item -LiteralPath $app).VersionInfo.ProductVersion
if ($nameForVersion -match '^humanmotioncraft-(.+)-windows-x64-portable$') {
    $named = $Matches[1]
    if ($productVersion -eq $named) {
        Result 'PASS' "version: humanmotioncraft.exe is $productVersion, as the release's name says" @()
    } else {
        Result 'FAIL' "version: the release is named $named but humanmotioncraft.exe's ProductVersion is '$productVersion'" @()
    }
} else {
    Result 'SKIP' "version: '$nameForVersion' carries no version to compare with ProductVersion '$productVersion'" @()
}

# ---------------------------------------------------------------------------------------------
# the minimal environment
$systemRoot = $env:SystemRoot
if (-not $systemRoot) { $systemRoot = 'C:\Windows' }
$home_ = Fresh (Join-Path $Scratch 'home')
$tmp = Fresh (Join-Path $Scratch 'tmp')
$local = Join-Path $home_ 'AppData\Local'
$roaming = Join-Path $home_ 'AppData\Roaming'
[void](New-Item -ItemType Directory -Path $local, $roaming -Force)
$textDir = Fresh (Join-Path $Scratch 'textmotion')
$poseDir = Fresh (Join-Path $Scratch 'posefill')
$cleanPath = @(
    (Join-Path $systemRoot 'System32'),
    $systemRoot,
    (Join-Path $systemRoot 'System32\Wbem'),
    (Join-Path $systemRoot 'System32\WindowsPowerShell\v1.0')
) -join ';'
$cleanEnv = @{
    'SystemRoot' = $systemRoot
    'windir' = $systemRoot
    'SystemDrive' = $systemRoot.Substring(0, 2)
    'ComSpec' = (Join-Path $systemRoot 'System32\cmd.exe')
    'PATH' = $cleanPath
    'PATHEXT' = '.COM;.EXE;.BAT;.CMD'
    'NUMBER_OF_PROCESSORS' = [string][Environment]::ProcessorCount
    'PROCESSOR_ARCHITECTURE' = 'AMD64'
    'OS' = 'Windows_NT'
    'USERNAME' = 'release-check'
    'COMPUTERNAME' = [string]$env:COMPUTERNAME
    'USERPROFILE' = $home_
    'HOMEDRIVE' = $home_.Substring(0, 2)
    'HOMEPATH' = $home_.Substring(2)
    'APPDATA' = $roaming
    'LOCALAPPDATA' = $local
    'TEMP' = $tmp
    'TMP' = $tmp
    'ProgramData' = [string]$env:ProgramData
    'ProgramFiles' = [string]$env:ProgramFiles
    'ProgramFiles(x86)' = [string]${env:ProgramFiles(x86)}
    'AC_TEXTMOTION_DIR' = $textDir
    'AC_POSEFILL_DIR' = $poseDir
}
# AC_FFMPEG, AC_LOG, RUST_BACKTRACE, CARGO_HOME, PYTHONPATH and everything else: unset.
Out ("environment: PATH=" + $cleanPath)
Out "environment: HOME, APPDATA, LOCALAPPDATA, TEMP under $Scratch; AC_FFMPEG unset; AC_TEXTMOTION_DIR and AC_POSEFILL_DIR empty scratch folders"
Out ''

# ---------------------------------------------------------------------------------------------
# 3. clean path
$where = Invoke-Clean (Join-Path $systemRoot 'System32\where.exe') @('ffmpeg', 'python', 'py', 'uv', 'cargo', 'rustc') $cleanEnv $Scratch
$found = @(($where.Out -split "`r?`n") | Where-Object { $_.Trim() -ne '' })
if ($found.Count -eq 0) {
    Result 'PASS' 'clean path: no ffmpeg, python, py, uv, cargo or rustc on the PATH the checks run with' @("where.exe exit $($where.Code): nothing found")
} else {
    Result 'FAIL' 'clean path: tools found on the minimal PATH, so the checks below prove nothing about a clean machine' $found
}

# ---------------------------------------------------------------------------------------------
# 4. the app
$appLibrary = Fresh (Join-Path $Scratch 'app-library')
$openScript = Join-Path $Scratch 'open-walk.json'
Write-Utf8 $openScript '[{"command": "clip.open", "params": {"id": "Locomotion/Walk"}}, {"command": "frame.set", "params": {"frame": 10}}]'
$shot = Join-Path $Scratch 'app-window.png'
if (Test-Path -LiteralPath $shot) { Remove-Item -LiteralPath $shot -Force }
$r = Invoke-Clean $app @('--library', $appLibrary, '--script', $openScript, '--screenshot', $shot, '--size', '1280x800') $cleanEnv $topDir
$shotOk = Has-Magic $shot $pngMagic
$shotSize = if (Test-Path -LiteralPath $shot) { (Get-Item -LiteralPath $shot).Length } else { 0 }
$evidence = @("$($r.Command)", "exit $($r.Code) in $($r.Seconds) s" + $(if ($r.TimedOut) { " (killed after $TimeoutSeconds s)" } else { '' }), "screenshot: $shot ($shotSize bytes, PNG header: $shotOk)", (Tail $r.Err))
if ($r.Code -eq 0 -and -not $r.TimedOut -and $shotOk -and $shotSize -gt 10000) {
    Result 'PASS' 'app: humanmotioncraft.exe started from the release, opened Locomotion/Walk by --script and wrote a screenshot' $evidence
} else {
    Result 'FAIL' 'app: humanmotioncraft.exe did not start, or wrote no screenshot' $evidence
}

# ---------------------------------------------------------------------------------------------
# 5. ac script against the shipped assets
$cliLibrary = Fresh (Join-Path $Scratch 'cli-library')
$picture = Join-Path $Scratch 'walk-front.png'
$gif = Join-Path $Scratch 'walk.gif'
foreach ($f in @($picture, $gif)) { if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f -Force } }
$steps = @(
    '{"command": "session.info", "params": {}}',
    '{"command": "clip.open", "params": {"id": "Locomotion/Walk"}}',
    '{"command": "frame.set", "params": {"frame": 10}}',
    '{"command": "bone.world", "params": {"bone": "wrist.L"}}',
    ('{"command": "clip.picture", "params": {"path": ' + (ConvertTo-Json $picture) + ', "view": "front", "width": 320, "height": 240}}'),
    ('{"command": "clip.video", "params": {"path": ' + (ConvertTo-Json $gif) + ', "to": 4, "width": 160, "height": 90}}')
)
$acScript = Join-Path $Scratch 'ac-script.json'
Write-Utf8 $acScript ('[' + ($steps -join ",`n") + ']')
$r = Invoke-Clean $ac @('--library', $cliLibrary, '--jsonl', 'script', $acScript) $cleanEnv $topDir
$lines = Json-Lines $r.Out
$parsedAll = ($lines.Count -eq $steps.Count) -and -not ($lines | Where-Object { $null -eq $_ })
$info = if ($lines.Count -ge 1 -and $lines[0]) { $lines[0].result } else { $null }
$opened = if ($lines.Count -ge 2 -and $lines[1]) { $lines[1].result } else { $null }
$wrist = if ($lines.Count -ge 4 -and $lines[3]) { $lines[3].result } else { $null }
$pictureOk = Has-Magic $picture $pngMagic
$gifOk = Has-Magic $gif $gifMagic
$evidence = @(
    "$($r.Command)",
    "exit $($r.Code) in $($r.Seconds) s; $($lines.Count) of $($steps.Count) results, all JSON: $parsedAll",
    $(if ($info) { "session.info: $($info.product) $($info.version), rig $($info.rig_hash), model $(ConvertTo-Json -Compress $info.model), $($info.commands) commands" } else { 'session.info: no result' }),
    $(if ($opened) { "clip.open: $($opened.id), $($opened.length) frames" } else { 'clip.open: no result' }),
    $(if ($wrist) { "bone.world wrist.L at frame 10: position $(ConvertTo-Json -Compress $wrist.position)" } else { 'bone.world: no result' }),
    "clip.picture: $picture (PNG header: $pictureOk)",
    "clip.video GIF: $gif (GIF header: $gifOk, $(if (Test-Path -LiteralPath $gif) { (Get-Item -LiteralPath $gif).Length } else { 0 }) bytes)",
    (Tail $r.Err)
)
if ($r.Code -eq 0 -and $parsedAll -and $opened -and $opened.length -gt 1 -and $info -and $pictureOk -and $gifOk) {
    Result 'PASS' 'ac script: ac.exe ran six commands against the shipped assets, wrote a picture and a GIF' $evidence
} else {
    Result 'FAIL' 'ac script: ac.exe could not run the script against the shipped assets' $evidence
}

# ---------------------------------------------------------------------------------------------
# 6. clip.video to MP4 without ffmpeg: a clear refusal, no crash, no file
$mp4 = Join-Path $Scratch 'walk.mp4'
if (Test-Path -LiteralPath $mp4) { Remove-Item -LiteralPath $mp4 -Force }
$mp4Script = Join-Path $Scratch 'ac-mp4.json'
Write-Utf8 $mp4Script ('[{"command": "clip.open", "params": {"id": "Locomotion/Walk"}}, {"command": "clip.video", "params": {"path": ' + (ConvertTo-Json $mp4) + ', "to": 4, "width": 160, "height": 90}}]')
$mp4Env = $cleanEnv.Clone()
$note = ''
# a development machine: the app also looks for ffmpeg at this fixed path (ac_render::anim::ffmpeg),
# so here the only way to run the no-ffmpeg case is to point AC_FFMPEG at a file that doesn't exist
$devFfmpeg = 'C:\tools\ffmpeg\bin\ffmpeg.exe'
if (Test-Path -LiteralPath $devFfmpeg -PathType Leaf) {
    $mp4Env['AC_FFMPEG'] = Join-Path $Scratch 'no-ffmpeg\ffmpeg.exe'
    $note = "this machine has $devFfmpeg, the app's fallback, so AC_FFMPEG was pointed at a missing file for this one check; Windows Sandbox proves the real case"
}
$r = Invoke-Clean $ac @('--library', $cliLibrary, '--jsonl', 'script', $mp4Script) $mp4Env $topDir
$text = $r.Err + "`n" + $r.Out
$mentions = $text -match '(?i)ffmpeg'
$crashed = $text -match '(?i)panicked|RUST_BACKTRACE|stack backtrace'
$mp4Exists = Test-Path -LiteralPath $mp4 -PathType Leaf
$evidence = @("$($r.Command)", "exit $($r.Code) in $($r.Seconds) s; the message names ffmpeg: $mentions; crash markers: $crashed; $mp4 written: $mp4Exists", (Tail $r.Err 4), $note)
if ($r.Code -ne 0 -and -not $r.TimedOut -and $mentions -and -not $crashed -and -not $mp4Exists) {
    Result 'PASS' 'no ffmpeg: clip.video to .mp4 is refused with a message that names ffmpeg, and writes nothing' $evidence
} elseif ($r.Code -eq 0 -and $mp4Exists) {
    Result 'FAIL' 'no ffmpeg: an MP4 was written, so an ffmpeg was found on this machine' $evidence
} else {
    Result 'FAIL' 'no ffmpeg: clip.video to .mp4 did not refuse cleanly' $evidence
}

# ---------------------------------------------------------------------------------------------
# 7 and 8: the helper installs find the shipped source tree (check downloads nothing)
function Check-Install([string]$command, [string]$what) {
    $r = Invoke-Clean $ac @('--library', $cliLibrary, '--jsonl', 'run', $command, '{"action": "check"}') $cleanEnv $topDir
    $lines = Json-Lines $r.Out
    $v = if ($lines.Count -ge 1) { $lines[0] } else { $null }
    $tree = $null
    $needs = @()
    if ($v -and $v.requirements) {
        foreach ($n in $v.requirements) {
            $needs += ("requirement {0}: ok={1} required={2}: {3}" -f $n.name, $n.ok, $n.required, $n.note)
            if ($n.name -eq 'Source tree') { $tree = $n }
        }
    }
    $evidence = @("$($r.Command)", "exit $($r.Code) in $($r.Seconds) s; state: $(if ($v) { $v.state } else { 'no JSON result' }); dir: $(if ($v) { $v.dir } else { '' })") + $needs + @((Tail $r.Err 4))
    if ($r.Code -eq 0 -and $v -and $v.state -eq 'not installed' -and $tree -and $tree.ok -eq $true) {
        Result 'PASS' "${command}: check finds the shipped source tree for $what, with nothing installed and nothing downloaded" $evidence
    } elseif ($r.Code -eq 0 -and $v -and $v.state -ne 'not installed') {
        Result 'FAIL' "${command}: the state is '$($v.state)', not 'not installed': the scratch install folder was not empty, so the check is not clean" $evidence
    } elseif ($r.Code -eq 0 -and $v -and $tree) {
        Result 'FAIL' "${command}: the shipped source tree was not found ($($tree.note))" $evidence
    } else {
        Result 'FAIL' "${command}: check did not answer as expected" $evidence
    }
}
Check-Install 'text.install' 'motion from text'
Check-Install 'posefill.install' 'pose fill'

# ---------------------------------------------------------------------------------------------
Out ''
Out ("{0} passed, {1} failed, {2} skipped; log: {3}" -f $script:passed, $script:failed, $script:skipped, $script:log)
if ($Report) {
    [void](New-Item -ItemType Directory -Path $Report -Force)
    foreach ($f in @($script:log, $shot, $picture, $gif)) {
        if (Test-Path -LiteralPath $f) { Copy-Item -LiteralPath $f -Destination $Report -Force }
    }
    Out "copied the log and pictures to $Report"
}
if ($script:failed -gt 0) { exit 1 } else { exit 0 }
