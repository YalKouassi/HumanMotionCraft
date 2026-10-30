# Overnight retrain plan prepared on 2026-10-30 (roadmap 134500, 140000, 142000). Not run yet:
# the owner starts it. Run from the repo root:  pwsh -NoProfile -File training/overnight-2026-10-30.ps1
# Everything logs under training/data/models/. Nothing here exports or ships a model: the owner
# picks by eye afterwards (export with  uv run python -m ac_train.export <tag> --checkpoint best --as parts).
$ErrorActionPreference = 'Stop'
Set-Location "$PSScriptRoot"
$log = "data/models/overnight-2026-10-30.log"
function Step($name, $cmd) {
  "=== $(Get-Date -Format s) $name" | Tee-Object -FilePath $log -Append
  $parts = $cmd -split ' '
  & uv run python -m "ac_train.$($parts[0])" @($parts | Select-Object -Skip 1) 2>&1 | Tee-Object -FilePath $log -Append
  if ($LASTEXITCODE -ne 0) { "!!! $name failed ($LASTEXITCODE)" | Tee-Object -FilePath $log -Append; exit $LASTEXITCODE }
}
# 1. The shipped model's card has no scenario rows (134750 found this): fill them, no training.
Step "evaluate shipped" "evaluate --tag parts-lowweights --checkpoint best"
# 2. 140000: context after key B, the shipped recipe plus the option (3 h).
Step "train parts-after4" "train --minutes 180 --tag parts-after4 --context-after 4 --loss-weights vel=0.1,jerk=0.0005,skate=0.1"
Step "evaluate parts-after4" "evaluate --tag parts-after4 --checkpoint best"
# 3. 142000: several keys in one window, on top of 140000's option (3 h).
Step "train parts-multikey" "train --minutes 180 --tag parts-multikey --context-after 4 --multi-key --loss-weights vel=0.1,jerk=0.0005,skate=0.1"
Step "evaluate parts-multikey" "evaluate --tag parts-multikey --checkpoint best"
"=== $(Get-Date -Format s) done" | Tee-Object -FilePath $log -Append
