<#
.SYNOPSIS
The release zip holds every path tools/release_manifest.txt requires, and nothing it forbids.

.DESCRIPTION
Needs no build and no machine of its own: it reads the zip's entry list (or a release folder
already unzipped) and compares it with the manifest. The workflow (.github/workflows/release.yml)
runs it right after making the zip; tools/release_check.ps1 runs it as its first check.

    pwsh -NoProfile -File tools/release_contents.ps1 -Path dist/humanmotioncraft-0.2.0-windows-x64-portable.zip
    pwsh -NoProfile -File tools/release_contents.ps1 -Path target/package/humanmotioncraft-0.2.0-windows-x64-portable

Prints one PASS or FAIL line per rule with the evidence, and exits 1 on any FAIL. Runs on Windows
PowerShell 5.1 and PowerShell 7 with no module beyond what Windows ships.

.PARAMETER Path
The zip, or the unzipped release folder (the top folder itself, or the folder holding it).

.PARAMETER Manifest
The list of required paths (default: release_manifest.txt beside this script).
#>
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$Manifest = ''
)

$ErrorActionPreference = 'Stop'
$script:failed = 0
if ($Manifest -eq '') {
    # not a parameter default: Windows PowerShell 5.1 has no $PSScriptRoot there yet
    $Manifest = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) 'release_manifest.txt'
}

function Say([string]$status, [string]$rule, [string[]]$evidence) {
    Write-Output ("{0} {1}" -f $status, $rule)
    foreach ($line in $evidence) { if ($line) { Write-Output ("    " + $line) } }
    if ($status -eq 'FAIL') { $script:failed++ }
}

# --- the manifest ----------------------------------------------------------------------------
if (-not (Test-Path -LiteralPath $Manifest -PathType Leaf)) {
    Say 'FAIL' 'manifest' @("not found: $Manifest")
    exit 1
}
$required = @()   # files
$folders = @()    # folders that must hold at least one file
$forbidden = @()  # wildcard patterns no entry may match
foreach ($raw in Get-Content -LiteralPath $Manifest) {
    $line = $raw.Trim()
    if ($line -eq '' -or $line.StartsWith('#')) { continue }
    if ($line.StartsWith('!')) { $forbidden += $line.Substring(1).Trim(); continue }
    if ($line.EndsWith('/')) { $folders += $line } else { $required += $line }
}
if ($required.Count -eq 0) {
    Say 'FAIL' 'manifest' @("$Manifest names no required file")
    exit 1
}

# --- the entries -----------------------------------------------------------------------------
# $entries: every file in the release as a '/'-separated path relative to the top folder.
$entries = New-Object System.Collections.Generic.List[string]
$top = $null
$problems = @()
$what = $null

if (Test-Path -LiteralPath $Path -PathType Leaf) {
    if ([System.IO.Path]::GetExtension($Path).ToLowerInvariant() -ne '.zip') {
        Say 'FAIL' 'release' @("$Path is neither a .zip nor a folder")
        exit 1
    }
    $what = "zip $Path"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Path).ProviderPath)
    try {
        $tops = @{}
        foreach ($e in $zip.Entries) {
            $name = $e.FullName
            if ($name.Contains('\')) {
                # the launcher refuses an archive written with Windows separators
                $problems += "entry written with a backslash: $name"
                $name = $name.Replace('\', '/')
            }
            if ($name.EndsWith('/')) { continue }   # a folder entry
            $slash = $name.IndexOf('/')
            if ($slash -lt 0) {
                $problems += "file outside the top folder: $name"
                continue
            }
            $tops[$name.Substring(0, $slash)] = $true
            $entries.Add($name.Substring($slash + 1))
        }
        if ($tops.Count -ne 1) {
            $problems += "the zip must hold exactly one top folder; it holds: " + (($tops.Keys | Sort-Object) -join ', ')
        } else {
            $top = @($tops.Keys)[0]
        }
    } finally {
        $zip.Dispose()
    }
} elseif (Test-Path -LiteralPath $Path -PathType Container) {
    $dir = (Resolve-Path -LiteralPath $Path).ProviderPath
    if (-not (Test-Path -LiteralPath (Join-Path $dir 'humanmotioncraft.exe') -PathType Leaf)) {
        # the folder holding the top folder
        $inner = @(Get-ChildItem -LiteralPath $dir -Directory | Where-Object {
            Test-Path -LiteralPath (Join-Path $_.FullName 'humanmotioncraft.exe') -PathType Leaf })
        if ($inner.Count -ne 1) {
            Say 'FAIL' 'release' @("$dir holds no release folder (one folder with humanmotioncraft.exe in it)")
            exit 1
        }
        $dir = $inner[0].FullName
    }
    $what = "folder $dir"
    $top = Split-Path -Leaf $dir
    $prefix = $dir.TrimEnd('\') + '\'
    foreach ($f in Get-ChildItem -LiteralPath $dir -File -Recurse) {
        $entries.Add($f.FullName.Substring($prefix.Length).Replace('\', '/'))
    }
} else {
    Say 'FAIL' 'release' @("not found: $Path")
    exit 1
}

if ($problems.Count -gt 0) {
    Say 'FAIL' 'release layout' $problems
} else {
    Say 'PASS' 'release layout' @("$what", "one top folder: $top, $($entries.Count) files")
}

# --- required files --------------------------------------------------------------------------
$have = @{}
foreach ($e in $entries) { $have[$e] = $true }
$missing = @($required | Where-Object { -not $have.ContainsKey($_) })
if ($missing.Count -eq 0) {
    Say 'PASS' 'required files' @("all $($required.Count) in the release")
} else {
    Say 'FAIL' 'required files' (@("$($missing.Count) of $($required.Count) missing:") + ($missing | ForEach-Object { "  $_" }))
}

# --- required folders ------------------------------------------------------------------------
$empty = @()
$counts = @()
foreach ($folder in $folders) {
    $n = @($entries | Where-Object { $_.StartsWith($folder) }).Count
    if ($n -eq 0) { $empty += $folder } else { $counts += "$folder $n files" }
}
if ($folders.Count -eq 0) {
    # nothing to check
} elseif ($empty.Count -eq 0) {
    Say 'PASS' 'required folders' $counts
} else {
    Say 'FAIL' 'required folders' (@("empty or missing:") + ($empty | ForEach-Object { "  $_" }))
}

# --- forbidden -------------------------------------------------------------------------------
if ($forbidden.Count -gt 0) {
    $hits = @()
    foreach ($pattern in $forbidden) {
        $hits += @($entries | Where-Object { $_ -like $pattern } | ForEach-Object { "$_ (matches $pattern)" })
    }
    if ($hits.Count -eq 0) {
        Say 'PASS' 'nothing forbidden' @("no entry matches: " + ($forbidden -join ', '))
    } else {
        Say 'FAIL' 'nothing forbidden' $hits
    }
}

if ($script:failed -gt 0) { exit 1 } else { exit 0 }
