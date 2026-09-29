# Usage: cargo run -q -p magpie-core --example lunar_dump --release > dump.txt
#        pwsh scripts/lunar-xcheck.ps1 -Dump dump.txt   (Windows: needs .NET)
# Compare tyme4rs (lunar_dump.rs output) with .NET ChineseLunisolarCalendar,
# an independent implementation, day by day 1901-02-19 .. 2100-12-31.
param([string]$Dump)
$cal = [Globalization.ChineseLunisolarCalendar]::new()
$lines = [IO.File]::ReadAllLines($Dump)
$bad = [Collections.Generic.List[string]]::new()
foreach ($l in $lines) {
  $p = $l.Split(' ')
  $d = [datetime]::ParseExact($p[0], 'yyyy-MM-dd', $null)
  $y = $cal.GetYear($d); $m = $cal.GetMonth($d); $day = $cal.GetDayOfMonth($d)
  $lm = $cal.GetLeapMonth($y)
  $leap = 0
  if ($lm -gt 0 -and $m -eq $lm) { $leap = 1; $m = $m - 1 } elseif ($lm -gt 0 -and $m -gt $lm) { $m = $m - 1 }
  $mine = "$y $m $leap $day"
  $theirs = "$($p[1]) $($p[2]) $($p[3]) $($p[4])"
  if ($mine -ne $theirs) { $bad.Add("$($p[0]) tyme=[$theirs] dotnet=[$mine]") }
}
"days compared: $($lines.Count)"
"mismatches: $($bad.Count)"
$bad | Select-Object -First 40
