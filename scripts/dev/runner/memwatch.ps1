$a=[int](Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory).AvailableMBytes
function S($n){ [int](((Get-Process $n -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Sum).Sum)/1MB) }
function M($exe,$re){ [int](((Get-CimInstance Win32_Process -Filter "Name='$exe'" | Where-Object { $_.CommandLine -match $re } | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).WorkingSet64 } | Measure-Object -Sum).Sum)/1MB) }
# wave 106: Playwright's Chromium (the column read 0 in wave 105: the old filter looked at chrome.exe only). Counted: the headless shell by name
# (headless_shell.exe, chrome-headless-shell.exe: only Playwright runs those), and chrome.exe only where its path or command line holds
# ms-playwright, so the owner's own browser is never counted.
function C(){ [int](((Get-CimInstance Win32_Process | Where-Object { $_.Name -in 'headless_shell.exe','chrome-headless-shell.exe' -or ($_.Name -eq 'chrome.exe' -and ("$($_.ExecutablePath) $($_.CommandLine)" -match 'ms-playwright')) } | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).WorkingSet64 } | Measure-Object -Sum).Sum)/1MB) }
"{0},{1},{2},{3},{4},{5},{6},{7}" -f (Get-Date -Format HH:mm:ss),$a,(S node),(S sr-server),(C),(S postgres),(S claude),(M 'node.exe' 'tsc|vitest|eslint')
