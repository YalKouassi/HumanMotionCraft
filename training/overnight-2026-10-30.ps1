# Overnight run prepared on 2026-10-30: roadmap 153000, the natural-only base model. Not run yet: the
# owner starts it, after editing training/review/natural-takes.json from the candidate videos
# (target/shots/2026-10-30/natural-candidates/<type>.mp4). Run from the repo root:
#   pwsh -NoProfile -File training/overnight-2026-10-30.ps1
# Everything logs under training/data/models/. Nothing here exports or ships a model: the owner
# judges the cards and side-by-side videos afterwards (export with
# uv run python -m ac_train.export parts-natural --checkpoint best --as parts, only if it wins).
# The 140000 (--context-after 4) and 142000 (--multi-key) runs that this script held before were
# moved out (owner's decision, 2026-10-30: 153000 trains next) and go in a later night.
$ErrorActionPreference = 'Stop'
Set-Location "$PSScriptRoot"
$log = "data/models/overnight-2026-10-30.log"
function Step($name, $cmd) {
  "=== $(Get-Date -Format s) $name" | Tee-Object -FilePath $log -Append
  $parts = $cmd -split ' '
  & uv run python -m "ac_train.$($parts[0])" @($parts | Select-Object -Skip 1) 2>&1 | Tee-Object -FilePath $log -Append
  if ($LASTEXITCODE -ne 0) { "!!! $name failed ($LASTEXITCODE)" | Tee-Object -FilePath $log -Append; exit $LASTEXITCODE }
}
# The take list is the owner's word: a non-empty include lists exactly the takes read (--natural is
# then only documentation of where the list came from); exclude takes out whatever else says.
$takes = "review/natural-takes.json"
# 1. The shipped model's card has no scenario rows (134750 found this): fill them, no training.
Step "evaluate shipped" "evaluate --tag parts-lowweights --checkpoint best"
# 2. 153000: the natural-only model, the shipped recipe (134500's weights), nothing else changed (3 h).
#    Training prints the minutes that survive by type and set before its first step.
Step "train parts-natural" "train --minutes 180 --tag parts-natural --natural --takes $takes --loss-weights vel=0.1,jerk=0.0005,skate=0.1"
# 3. The natural model on every held-out window (sit, ground, combat included: the cost), then on the
#    natural held-out windows alone, and the baseline on those same windows, under their own card key.
Step "evaluate parts-natural" "evaluate --tag parts-natural --checkpoint best"
Step "evaluate parts-natural on natural" "evaluate --tag parts-natural --checkpoint best --natural --takes $takes --subset natural"
Step "evaluate baseline on natural" "evaluate --tag parts-lowweights --checkpoint best --natural --takes $takes --subset natural"
# 4. The tables 153000 asks for: both models by gap bin and type, on the natural windows and on all.
Step "compare on natural" "compare --card parts-natural:best:natural --card parts-lowweights:best:natural"
Step "compare on all" "compare --card parts-natural:best --card parts-lowweights:best"
"=== $(Get-Date -Format s) done" | Tee-Object -FilePath $log -Append
