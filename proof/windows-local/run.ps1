# Builds the harness against each checkout in $Labels and runs the proof in alternating order.
param(
    [string[]]$Labels,
    [int]$Rounds = 6,
    [string]$BenchZone = "Pacific Standard Time"
)
$ErrorActionPreference = "Stop"
$root = (Get-Location).Path
$out = Join-Path $root "results.jsonl"
Remove-Item -ErrorAction SilentlyContinue $out

function Bin($label) { Join-Path $root "harness-$label\target\release\windows-local-proof.exe" }

foreach ($label in $Labels) {
    $dir = Join-Path $root "harness-$label"
    Copy-Item -Recurse (Join-Path $root "proof-src\proof\windows-local") $dir
    $chrono = (Join-Path $root $label) -replace '\\', '/'
    (Get-Content "$dir\Cargo.toml") -replace 'CHRONO_PATH', $chrono | Set-Content "$dir\Cargo.toml"
    cargo build --release --manifest-path "$dir\Cargo.toml"
    if ($LASTEXITCODE -ne 0) { throw "build $label failed" }
}

function Run($label, $mode) {
    & (Bin $label) $mode $label | Tee-Object -Append -FilePath $out
    if ($LASTEXITCODE -ne 0) { throw "$mode $label failed" }
}

tzutil /s $BenchZone
& (Bin $Labels[0]) profile | Tee-Object -Append -FilePath $out
if ($LASTEXITCODE -ne 0) { throw "profile failed" }

for ($r = 0; $r -lt $Rounds; $r++) {
    $order = @($Labels)
    if ($r % 2 -eq 1) { [array]::Reverse($order) }
    foreach ($label in $order) { Run $label "bench" }
}

foreach ($zone in @("Pacific Standard Time", "W. Europe Standard Time", "AUS Eastern Standard Time", "Morocco Standard Time", "Iran Standard Time", "Pacific Standard Time_dstoff", "UTC")) {
    tzutil /s $zone
    "{`"kind`":`"zone`",`"zone`":`"$zone`"}" | Tee-Object -Append -FilePath $out
    foreach ($label in $Labels) { Run $label "checksum" }
}

foreach ($label in $Labels) { Run $label "reconfig" }
tzutil /s $BenchZone
