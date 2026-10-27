#requires -Version 5.1
<#
.SYNOPSIS
Shows Windows-visible physical RAM using a fresh native snapshot on every tick.
.DESCRIPTION
Runs on Windows 11 in Windows PowerShell 5.1 or PowerShell 7 without elevation.
Use Ctrl+C to stop. An in-place dashboard shows a table, rolling physical RAM
history and total-RAM change events. Redirected output uses plain snapshots.
This observes Windows-visible memory, not the hypervisor's requested allocation.
.EXAMPLE
.\Watch-Ram.ps1
.EXAMPLE
.\Watch-Ram.ps1 -IntervalMilliseconds 250
.EXAMPLE
.\Watch-Ram.ps1 -Samples 5 -AsObject | ConvertTo-Json
.EXAMPLE
.\Watch-Ram.ps1 -AsObject | Export-Csv .\ram-history.csv -NoTypeInformation
#>
[CmdletBinding()]
param(
    [ValidateRange(100, 60000)]
    [int] $IntervalMilliseconds = 1000,
    # Zero runs until cancelled; a positive value provides a bounded observation.
    [ValidateRange(0, 2147483647)]
    [int] $Samples = 0,
    [switch] $AsObject,
    [ValidateRange(10, 3600)]
    [int] $HistorySamples = 60,
    # Useful in transcripts, redirected output, or terminals without cursor support.
    [switch] $NoClear
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') {
    throw 'This script requires Windows.'
}

if (-not ('VirtioMemTools.RamSnapshot' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace VirtioMemTools {
    public static class RamSnapshot {
        [StructLayout(LayoutKind.Sequential)]
        public struct PerformanceInformation {
            public uint cb;
            public UIntPtr CommitTotal, CommitLimit, CommitPeak;
            public UIntPtr PhysicalTotal, PhysicalAvailable, SystemCache;
            public UIntPtr KernelTotal, KernelPaged, KernelNonpaged, PageSize;
            public uint HandleCount, ProcessCount, ThreadCount;
        }
        [DllImport("psapi.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetPerformanceInfo(
            out PerformanceInformation information, uint size);
        public static PerformanceInformation Read() {
            PerformanceInformation value;
            if (!GetPerformanceInfo(out value,
                    (uint)Marshal.SizeOf(typeof(PerformanceInformation)))) {
                throw new Win32Exception(Marshal.GetLastWin32Error(),
                    "GetPerformanceInfo failed");
            }
            if (value.PageSize.ToUInt64() == 0 || value.PhysicalTotal.ToUInt64() == 0 ||
                    value.PhysicalAvailable.ToUInt64() > value.PhysicalTotal.ToUInt64() ||
                    value.CommitTotal.ToUInt64() > value.CommitLimit.ToUInt64()) {
                throw new InvalidOperationException("Invalid Windows memory snapshot");
            }
            return value;
        }
    }
}
'@
}

function Format-RamValue([decimal] $Bytes) {
    return ('{0:N3} GiB' -f ($Bytes / 1GB))
}

function New-RamBar([double] $Percent, [int] $Width = 30) {
    $filled = [int] [math]::Round([math]::Max(0, [math]::Min(100, $Percent)) * $Width / 100)
    return '[' + ('#' * $filled) + ('.' * ($Width - $filled)) + ']'
}

$history = New-Object 'System.Collections.Generic.List[object]'
$events = New-Object 'System.Collections.Generic.List[string]'
$previousTotal = $null
$baselineTotal = $null
$minimumTotal = $null
$maximumTotal = $null
$minimumAvailable = $null
$peakInUse = [uint64] 0
$addedBytes = [decimal] 0
$removedBytes = [decimal] 0
$sampleNumber = 0
$startedAt = [DateTimeOffset]::Now
$interactive = $false
$oldCursorVisible = $true
if (-not $AsObject -and -not $NoClear) {
    try {
        $interactive = -not [Console]::IsOutputRedirected -and
            [Console]::WindowWidth -ge 80 -and [Console]::WindowHeight -ge 30
        if ($interactive) {
            $oldCursorVisible = [Console]::CursorVisible
            [Console]::Clear()
            [Console]::CursorVisible = $false
        }
    } catch {
        # ISE, remoting and redirected hosts may not expose a console.
        $interactive = $false
    }
}

try {
    while ($Samples -eq 0 -or $sampleNumber -lt $Samples) {
        # Never cache physical total: hot-add/removal can change it between ticks.
        $native = [VirtioMemTools.RamSnapshot]::Read()
        $pageSize = [decimal] $native.PageSize.ToUInt64()
        $total = [uint64] ([decimal] $native.PhysicalTotal.ToUInt64() * $pageSize)
        $available = [uint64] ([decimal] $native.PhysicalAvailable.ToUInt64() * $pageSize)
        $commit = [uint64] ([decimal] $native.CommitTotal.ToUInt64() * $pageSize)
        $limit = [uint64] ([decimal] $native.CommitLimit.ToUInt64() * $pageSize)
        $inUse = $total - $available
        $now = [DateTimeOffset]::Now
        # Decimal subtraction preserves negative deltas without unsigned underflow.
        $delta = if ($null -eq $previousTotal) { [decimal] 0 } else {
            [decimal] $total - [decimal] $previousTotal
        }
        if ($null -eq $baselineTotal) {
            $baselineTotal = $total
            $minimumTotal = $total
            $maximumTotal = $total
            $minimumAvailable = $available
        }
        $minimumTotal = [math]::Min($minimumTotal, $total)
        $maximumTotal = [math]::Max($maximumTotal, $total)
        $minimumAvailable = [math]::Min($minimumAvailable, $available)
        $peakInUse = [math]::Max($peakInUse, $inUse)
        if ($delta -ne 0) {
            if ($delta -gt 0) { $addedBytes += $delta } else { $removedBytes -= $delta }
            $events.Insert(0, ('{0:HH:mm:ss}  {1,10:+0.00;-0.00} MiB  total {2:N3} GiB' -f
                $now, ($delta / 1MB), ($total / 1GB)))
            if ($events.Count -gt 3) { $events.RemoveAt(3) }
        }
        $row = [pscustomobject] [ordered] @{
            Timestamp = $now
            TotalBytes = $total
            AvailableBytes = $available
            InUseBytes = $inUse
            CommitBytes = $commit
            CommitLimitBytes = $limit
            CommitHeadroomBytes = $limit - $commit
            TotalDeltaBytes = $delta
            TotalGiB = [math]::Round($total / 1GB, 3)
            AvailableGiB = [math]::Round($available / 1GB, 3)
            SessionMinimumTotalBytes = $minimumTotal
            SessionMaximumTotalBytes = $maximumTotal
            SessionMinimumAvailableBytes = $minimumAvailable
            SessionPeakInUseBytes = $peakInUse
            SessionAddedBytes = $addedBytes
            SessionRemovedBytes = $removedBytes
        }
        if ($AsObject) {
            $row
        } else {
            $history.Add($row)
            if ($history.Count -gt $HistorySamples) { $history.RemoveAt(0) }
            $lines = New-Object 'System.Collections.Generic.List[string]'
            $lines.Add((' RAM WATCH | {0} | {1:HH:mm:ss} | refresh {2} ms' -f
                $env:COMPUTERNAME, $now, $IntervalMilliseconds))
            $lines.Add((' Session {0} | samples {1} | Ctrl+C to stop' -f
                ($now - $startedAt).ToString('hh\:mm\:ss'), ($sampleNumber + 1)))
            $lines.Add(' +---------------------+------------------+-------------------------------+')
            $lines.Add(' | PHYSICAL RAM        | NOW              | SESSION                       |')
            $lines.Add(' +---------------------+------------------+-------------------------------+')
            $lines.Add((' | Total               | {0,16} | min {1,8:N3} / max {2,8:N3} GiB |' -f
                (Format-RamValue $total), ($minimumTotal / 1GB), ($maximumTotal / 1GB)))
            $lines.Add((' | In use              | {0,16} | peak {1,20:N3} GiB |' -f
                (Format-RamValue $inUse), ($peakInUse / 1GB)))
            $lines.Add((' | Available           | {0,16} | low  {1,20:N3} GiB |' -f
                (Format-RamValue $available), ($minimumAvailable / 1GB)))
            $netText = '{0:+0.00;-0.00;0.00} MiB' -f (([decimal] $total - $baselineTotal) / 1MB)
            $changeText = '+{0:N1} / -{1:N1} MiB' -f ($addedBytes / 1MB), ($removedBytes / 1MB)
            $lines.Add((' | Net total change    | {0,16} | {1,-29} |' -f $netText, $changeText))
            $lines.Add(' +---------------------+------------------+-------------------------------+')
            $lines.Add((' In use {0} {1,5:N1}%' -f
                (New-RamBar (100.0 * $inUse / $total)), (100.0 * $inUse / $total)))
            $lines.Add((' Commit {0} {1,5:N1}%' -f
                (New-RamBar (100.0 * $commit / $limit)), (100.0 * $commit / $limit)))
            $lines.Add((' Commit: {0} / {1}; headroom {2}' -f
                (Format-RamValue $commit), (Format-RamValue $limit), (Format-RamValue ($limit - $commit))))
            $lines.Add('')
            # Absolute GiB, not percentages: total changes remain visible even
            # when Windows grows memory during a workload. Scale includes total.
            $scale = [math]::Ceiling((($history | Measure-Object TotalBytes -Maximum).Maximum / 1GB))
            $scale = [math]::Max(1, $scale)
            $plotWidth = 60
            $plotHeight = 5
            $points = $history.ToArray()
            if ($history.Count -gt $plotWidth) {
                # Compress the entire selected window into columns, preserving
                # peak in-use within each bin and its latest physical total.
                $points = @(for ($column = 0; $column -lt $plotWidth; $column++) {
                    $first = [int] [math]::Floor($column * $history.Count / $plotWidth)
                    $last = [int] [math]::Floor(($column + 1) * $history.Count / $plotWidth) - 1
                    $peak = [uint64] 0
                    for ($index = $first; $index -le $last; $index++) {
                        $peak = [math]::Max($peak, $history[$index].InUseBytes)
                    }
                    [pscustomobject] @{
                        Timestamp = $history[$last].Timestamp
                        TotalBytes = $history[$last].TotalBytes
                        InUseBytes = $peak
                    }
                })
            }
            $span = ($history[$history.Count - 1].Timestamp - $history[0].Timestamp).TotalSeconds
            $lines.Add((' RAM HISTORY | last {0:N1}s | T=total  #=peak used | scale 0..{1} GiB' -f $span, $scale))
            for ($y = $plotHeight; $y -ge 0; $y--) {
                $label = $scale * $y / $plotHeight
                $cells = ''
                foreach ($point in $points) {
                    $totalLevel = [int] [math]::Round(($point.TotalBytes / 1GB) / $scale * $plotHeight)
                    $usedLevel = [int] [math]::Round(($point.InUseBytes / 1GB) / $scale * $plotHeight)
                    if ($y -eq $totalLevel) { $cells += 'T' }
                    elseif ($y -le $usedLevel) { $cells += '#' }
                    else { $cells += ' ' }
                }
                $lines.Add((' {0,5:N1} |{1}' -f $label, $cells.PadRight($plotWidth)))
            }
            $lines.Add('       +' + ('-' * $plotWidth))
            $lines.Add(('        {0:HH:mm:ss} oldest                         newest {1:HH:mm:ss}' -f
                $points[0].Timestamp, $points[-1].Timestamp))
            $lines.Add(' TOTAL RAM CHANGES (newest first)')
            for ($eventIndex = 0; $eventIndex -lt 3; $eventIndex++) {
                if ($eventIndex -lt $events.Count) { $lines.Add(' ' + $events[$eventIndex]) }
                elseif ($eventIndex -eq 0) { $lines.Add(' No total-RAM changes observed this session.') }
                else { $lines.Add('') }
            }
            $lines.Add(' Available includes standby/free/zero pages; commit is not resident RAM.')
            $lines.Add(' Windows-visible RAM, not hypervisor requested bytes.')
            if ($interactive) {
                try {
                    $width = [Console]::WindowWidth - 1
                    if ($width -lt 40 -or [Console]::WindowHeight -le $lines.Count) {
                        throw 'Console resized below dashboard dimensions.'
                    }
                    [Console]::SetCursorPosition(0, 0)
                    $frame = @($lines | ForEach-Object {
                        if ($_.Length -gt $width) { $_.Substring(0, $width) } else { $_.PadRight($width) }
                    }) -join [Environment]::NewLine
                    [Console]::Write($frame)
                } catch {
                    # Continue collecting if the terminal is resized or disconnected.
                    [Console]::CursorVisible = $oldCursorVisible
                    $interactive = $false
                    Write-Host ($lines -join [Environment]::NewLine)
                }
            } else {
                Write-Host ($lines -join [Environment]::NewLine)
            }
        }
        $previousTotal = $total
        $sampleNumber++
        if ($Samples -eq 0 -or $sampleNumber -lt $Samples) {
            Start-Sleep -Milliseconds $IntervalMilliseconds
        }
    }
} finally {
    if ($interactive) {
        [Console]::CursorVisible = $oldCursorVisible
        [Console]::WriteLine()
    }
}
