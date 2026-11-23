# Runs the regression tests on Windows: red on the base with only the test added, green on the
# head, and red on a head mutant that ignores the cached year.
$ErrorActionPreference = "Stop"
$root = (Get-Location).Path
tzutil /s "Pacific Standard Time"

function Expect($dir, $filter, $shouldPass, $needle) {
    Push-Location (Join-Path $root $dir)
    $output = cargo test --lib $filter -- --nocapture 2>&1 | Out-String
    $code = $LASTEXITCODE
    Pop-Location
    Write-Output "===== $dir $filter (exit $code)"
    Write-Output $output
    if ($shouldPass -and $code -ne 0) { throw "$dir $filter failed" }
    if (-not $shouldPass) {
        if ($code -eq 0) { throw "$dir $filter passed, expected a failure" }
        if (-not $output.Contains($needle)) { throw "$dir $filter failed without '$needle'" }
    }
}

Copy-Item -Recurse (Join-Path $root "base") (Join-Path $root "basetest")
git -C (Join-Path $root "basetest") apply (Join-Path $root "proof-src\proof\windows-local\base-test.patch")
if ($LASTEXITCODE -ne 0) { throw "patch failed" }

Copy-Item -Recurse (Join-Path $root "head") (Join-Path $root "mutant")
$file = Join-Path $root "mutant\src\offset\local\windows.rs"
$text = Get-Content -Raw $file
if (-not $text.Contains("if *cached_year == year {")) { throw "mutation site not found" }
$text.Replace("if *cached_year == year {", "if true {") | Set-Content -NoNewline $file

Expect "basetest" "repeated_year_reuses_loaded_rules" $false "366 rule queries for 366 conversions"
Expect "head" "offset::local" $true ""
Expect "mutant" "cached_rules_match_windows_rules" $false "assertion"
Write-Output "regression tests behaved as expected"
exit 0
