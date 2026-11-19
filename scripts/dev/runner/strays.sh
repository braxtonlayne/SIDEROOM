#!/usr/bin/env bash
# strays.sh (wave 104, D-989): what still runs from a worktree or folder, and what still listens. READ ONLY: it never
# stops anything (stop what is yours by its Windows PID: taskkill //PID <pid> //T //F).
#
#   bash strays.sh <worktree or folder name>... [--ports <n>...]
#
# For each name: the processes whose command line names it (PID, name, the first 120 characters of the command line),
# one PowerShell Get-CimInstance Win32_Process call for all the names. The listing leaves out its own PowerShell and
# every ancestor of it and every command line that names strays.sh (the shell that runs this script names the folder). A name is matched
# as given, with / turned into \ and with \ turned into /; a /c/... name also as C:/... . Case does not matter.
# With --ports: the listeners on those ports (netstat -ano), with the owning process name.
# Exit 0 when nothing is found, 1 when something is, 2 on a usage error.
names=(); ports=(); mode=names
for a in "$@"; do
  case $a in
    --ports) mode=ports ;;
    -h|--help) sed -n 2,14p "$0"; exit 0 ;;
    *) if [ $mode = ports ]; then ports+=("$a"); else names+=("$a"); fi ;;
  esac
done
[ ${#names[@]} -gt 0 ] || [ ${#ports[@]} -gt 0 ] || { echo "usage: strays.sh <name>... [--ports <n>...]" >&2; exit 2; }
found=0

if [ ${#names[@]} -gt 0 ]; then
  # names go in through the environment, one per line, so no quoting problem reaches PowerShell
  STRAYS_NAMES=$(printf '%s\n' "${names[@]}")
  export STRAYS_NAMES
  out=$(powershell -NoProfile -ExecutionPolicy Bypass -Command '
    $pats = @()
    foreach ($n in ($env:STRAYS_NAMES -split "`n")) {
      $n = $n.Trim(); if ($n -eq "") { continue }
      $pats += $n; $pats += $n.Replace("/","\"); $pats += $n.Replace("\","/")
      if ($n -match "^/([a-zA-Z])/(.*)$") { $pats += ($Matches[1].ToUpper() + ":/" + $Matches[2]); $pats += ($Matches[1].ToUpper() + ":\" + $Matches[2].Replace("/","\")) }
    }
    $pats = $pats | Select-Object -Unique
    $all = Get-CimInstance Win32_Process
    $skip = @{}; $p = $PID
    while ($p -and -not $skip.ContainsKey([int]$p)) { $skip[[int]$p] = 1; $row = $all | Where-Object { $_.ProcessId -eq $p } | Select-Object -First 1; if (-not $row) { break }; $p = $row.ParentProcessId }
    foreach ($r in $all) {
      if ($skip.ContainsKey([int]$r.ProcessId) -or -not $r.CommandLine -or $r.CommandLine -like "*strays.sh*") { continue }
      foreach ($pt in $pats) {
        if ($r.CommandLine.IndexOf($pt, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
          $c = ($r.CommandLine -replace "\s+"," "); if ($c.Length -gt 120) { $c = $c.Substring(0,120) }
          "{0} {1} {2}" -f $r.ProcessId, $r.Name, $c; break
        }
      }
    }' 2>/dev/null | tr -d '\r')
  if [ -n "$out" ]; then found=1; echo "processes naming ${names[*]}:"; echo "$out" | sed 's/^/  /'; fi
fi

if [ ${#ports[@]} -gt 0 ]; then
  re=$(IFS='|'; echo "${ports[*]}")
  lines=$(netstat -ano 2>/dev/null | tr -d '\r' | grep LISTENING | grep -E ":($re) ")
  if [ -n "$lines" ]; then
    found=1; echo "listeners on ${ports[*]}:"
    echo "$lines" | while read -r _ local _ _ pid; do
      pn=$(tasklist //FI "PID eq $pid" //FO CSV //NH 2>/dev/null | tr -d '\r' | head -1 | cut -d, -f1 | tr -d '"')
      echo "  $local pid $pid ${pn:-?}"
    done
  fi
fi

[ $found -eq 0 ] && echo "strays: none found"
exit $found
