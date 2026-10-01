# Per-user native utilities. No administrator access or execution-policy changes.
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA 'AgentCiv'),
    [switch]$WithHostTools,
    [switch]$NoModifyPath,
    [switch]$Rollback,
    [switch]$Uninstall
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-AgentCivVersion([string]$Value) { return $Value -cmatch '\Av[0-9]+\.[0-9]+\.[0-9]+\z' }
function Get-AgentCivHash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Test-AgentCivPath([string]$Path) {
    $itemPath = [IO.Path]::GetFullPath($Path)
    while ($itemPath) {
        if (Test-Path -LiteralPath $itemPath) {
            if ((Get-Item -LiteralPath $itemPath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw 'installation path contains a reparse point'
            }
        }
        $parent = [IO.Directory]::GetParent($itemPath)
        if ($null -eq $parent) { break }
        $itemPath = $parent.FullName
    }
}
function Set-AgentCivFile {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$Path, [string]$Content)
    if (-not $PSCmdlet.ShouldProcess($Path, 'Atomically write installer-owned file')) { return }
    $temporary = "$Path.new"
    if (Test-Path -LiteralPath $temporary) { throw 'staging file already exists; preserve it and resolve manually' }
    [IO.File]::WriteAllText($temporary, $Content, [Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $Path) { [IO.File]::Replace($temporary, $Path, [NullString]::Value) }
    else { [IO.File]::Move($temporary, $Path) }
}
function Get-AgentCivDownloadRequest([Uri]$Uri, [int]$TimeoutMilliseconds) {
    $request = [Net.HttpWebRequest]::Create($Uri)
    $request.AllowAutoRedirect = $false
    $request.Timeout = $TimeoutMilliseconds
    $request.ReadWriteTimeout = $TimeoutMilliseconds
    return $request
}
function Get-AgentCivDownloadRemaining([Diagnostics.Stopwatch]$Timer) {
    $remaining = 180000L - $Timer.ElapsedMilliseconds
    if ($remaining -le 0) { throw 'download timed out' }
    return [int]$remaining
}
function Receive-AgentCivAsset([string]$Url, [string]$Path, [long]$MaxBytes = 134217728) {
    # Origin is fixed by the caller. GitHub release storage redirects use HTTPS.
    $uri = [Uri]$Url
    if (-not $uri.IsAbsoluteUri -or $uri.Scheme -ne 'https') { throw 'insecure download URL' }
    if ($MaxBytes -le 0) { throw 'invalid download byte limit' }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $response = $null
    $inputStream = $null
    $outputStream = $null
    try {
        for ($redirect = 0; $redirect -le 5; $redirect++) {
            $request = Get-AgentCivDownloadRequest $uri (Get-AgentCivDownloadRemaining $timer)
            try { $response = $request.GetResponse() }
            catch {
                $failure = $_.Exception
                while ($failure -isnot [Net.WebException] -and $null -ne $failure.InnerException) { $failure = $failure.InnerException }
                if ($failure -is [Net.WebException]) { $response = $failure.Response }
                throw
            }
            if ([int]$response.StatusCode -ge 300 -and [int]$response.StatusCode -lt 400) {
                if ($redirect -eq 5) { throw 'too many download redirects' }
                $next = [Uri]::new($uri, $response.Headers['Location'])
                if ($next.Scheme -ne 'https') { throw 'insecure download redirect' }
                $response.Dispose()
                $response = $null
                $uri = $next
            } else { break }
        }
        if ($null -eq $response -or [int]$response.StatusCode -ne 200) { throw 'download failed' }
        if ($response.ContentLength -gt $MaxBytes) { throw 'download too large' }
        $inputStream = $response.GetResponseStream()
        $outputStream = [IO.File]::Create($Path)
        $buffer = [byte[]]::new(65536)
        $total = 0L
        while ($true) {
            $remaining = Get-AgentCivDownloadRemaining $timer
            if ($inputStream.CanTimeout) { $inputStream.ReadTimeout = $remaining }
            $count = $inputStream.Read($buffer, 0, $buffer.Length)
            if ($count -eq 0) { break }
            $total += $count
            if ($total -gt $MaxBytes) { throw 'download too large' }
            $outputStream.Write($buffer, 0, $count)
        }
        Get-AgentCivDownloadRemaining $timer | Out-Null
    } finally {
        if ($null -ne $outputStream) { $outputStream.Dispose() }
        if ($null -ne $inputStream) { $inputStream.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
        $timer.Stop()
    }
}
function Get-AgentCivTarget {
    $architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($architecture) {
        'X64' { return 'x86_64-pc-windows-msvc' }
        'Arm64' { return 'aarch64-pc-windows-msvc' }
        default { throw 'unsupported Windows architecture' }
    }
}
function Get-AgentCivBinaryVersion([string]$Path) {
    $result = & $Path --version
    if ($LASTEXITCODE -ne 0) { throw 'binary version command failed' }
    return $result
}
function Get-AgentCivUserPath { return [string][Environment]::GetEnvironmentVariable('Path', 'User') }
function Set-AgentCivUserPath {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$Value)
    if ($PSCmdlet.ShouldProcess('User PATH', 'Write scoped installer PATH entry')) { [Environment]::SetEnvironmentVariable('Path', $Value, 'User') }
}
function Invoke-AgentCivInstall {
    [CmdletBinding()]
    param([string]$RequestedVersion = 'latest', [Parameter(Mandatory)][string]$InstallPrefix,
        [switch]$IncludeHostTools, [switch]$SkipPath, [switch]$Restore, [switch]$Remove)
    if ($Restore -and $Remove) { throw 'conflicting operations' }
    if (-not ($Restore -or $Remove) -and $RequestedVersion -ne 'latest') {
        $candidate = $RequestedVersion
        if (-not $candidate.StartsWith('v')) { $candidate = 'v' + $candidate }
        if (-not (Test-AgentCivVersion $candidate)) { throw 'version must be vX.Y.Z' }
    }
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'use install.sh on Linux or macOS' }
    $root = [IO.Path]::GetFullPath($InstallPrefix).TrimEnd('\')
    if (-not [IO.Path]::IsPathRooted($InstallPrefix) -or $root -eq [IO.Path]::GetPathRoot($root).TrimEnd('\') -or $root -eq $env:USERPROFILE) {
        throw 'prefix must be an absolute dedicated directory'
    }
    Test-AgentCivPath $root
    $statePath = Join-Path $root 'state'
    $ownershipPath = Join-Path $root 'owned.sha256'
    $current = 'none'
    $previous = 'none'
    $owned = [ordered]@{}
    $installLock = $null
    $temporary = $null
    try {
    if (Test-Path -LiteralPath $root) {
        if (-not (Test-Path -LiteralPath $statePath) -or [IO.File]::ReadAllText($statePath).Trim() -ne 'agentciv-install/1') {
            throw 'prefix exists without installer ownership'
        }
        Test-AgentCivPath $statePath
        Test-AgentCivPath $ownershipPath
        foreach ($line in [IO.File]::ReadAllLines($ownershipPath)) {
            if ($line -cnotmatch '\A([0-9a-f]{64}) (bin/agentciv-(?:archive|reader|host|conformance)\.cmd|versions/v[0-9]+\.[0-9]+\.[0-9]+/(?:agentciv-(?:archive|reader|host|conformance)\.exe|components|LICENSE|THIRD_PARTY_NOTICES\.txt|NOTICE_INVENTORY\.json))\z') {
                throw 'invalid ownership manifest'
            }
            $digest = $Matches[1]; $relative = $Matches[2]
            if ($owned.Contains($relative)) { throw 'duplicate ownership path' }
            $path = Join-Path $root $relative
            Test-AgentCivPath $path
            if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-AgentCivHash $path) -ne $digest) {
                throw 'owned file changed or missing; preserve it and resolve manually'
            }
            $owned[$relative] = $digest
        }
        foreach ($name in 'current', 'previous', 'path-owned') { Test-AgentCivPath (Join-Path $root $name) }
        if (Test-Path -LiteralPath (Join-Path $root 'current')) { $current = [IO.File]::ReadAllText((Join-Path $root 'current')).Trim() }
        if (Test-Path -LiteralPath (Join-Path $root 'previous')) { $previous = [IO.File]::ReadAllText((Join-Path $root 'previous')).Trim() }
        if (($current -ne 'none' -and -not (Test-AgentCivVersion $current)) -or ($previous -ne 'none' -and -not (Test-AgentCivVersion $previous))) {
            throw 'invalid install state'
        }
    }
    $bin = Join-Path $root 'bin'
    if (($Remove -or $Restore) -and $current -eq 'none') { throw 'nothing installed at prefix' }
    $newPrefix = -not (Test-Path -LiteralPath $root)
    if ($newPrefix) { [void][IO.Directory]::CreateDirectory($root) }
    $lockPath = Join-Path $root '.install-lock'
    Test-AgentCivPath $lockPath
    $installLock = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    if ($newPrefix) {
        [void][IO.Directory]::CreateDirectory($bin)
        [void][IO.Directory]::CreateDirectory((Join-Path $root 'versions'))
        Set-AgentCivFile $statePath "agentciv-install/1`n"
        Set-AgentCivFile $ownershipPath ''
    }
    # If another installation completed after the preflight, fail safely and
    # require a retry with a fresh ownership snapshot.
    $lockedCurrent = 'none'
    if (Test-Path -LiteralPath (Join-Path $root 'current')) { $lockedCurrent = [IO.File]::ReadAllText((Join-Path $root 'current')).Trim() }
    if ($lockedCurrent -cne $current) { throw 'installation changed concurrently; retry' }
    if ($Remove) {
        if ($current -eq 'none') { throw 'nothing installed at prefix' }
        # Refuse locked executables before deleting any owned file.
        foreach ($relative in $owned.Keys) {
            $probe = [IO.File]::Open((Join-Path $root $relative), [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            $probe.Dispose()
        }
        foreach ($relative in $owned.Keys) { Remove-Item -LiteralPath (Join-Path $root $relative) }
        if (Test-Path -LiteralPath (Join-Path $root 'path-owned')) {
            $entries = @((Get-AgentCivUserPath) -split ';' | Where-Object { $_ -and $_ -ine $bin })
            Set-AgentCivUserPath ($entries -join ';')
        }
        foreach ($name in 'state', 'owned.sha256', 'current', 'previous', 'path-owned') {
            $path = Join-Path $root $name
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        }
        # No recursive deletion. Preserve unknown files in any directory.
        $ownedVersions = @($owned.Keys | Where-Object { $_.StartsWith('versions/') } | ForEach-Object { ($_ -split '/')[1] } | Select-Object -Unique)
        $directories = @($ownedVersions | ForEach-Object { Get-Item -LiteralPath (Join-Path $root "versions/$_") })
        foreach ($directory in $directories + @(Get-Item -LiteralPath $bin) + @(Get-Item -LiteralPath (Join-Path $root 'versions')) + @(Get-Item -LiteralPath $root)) {
            if (@(Get-ChildItem -LiteralPath $directory.FullName -Force).Count -eq 0) { Remove-Item -LiteralPath $directory.FullName }
        }
        Write-Output 'AgentCiv removed; unowned files and user data retained. Restart your terminal to refresh PATH.'
        return
    }
    $components = @('agentciv-archive', 'agentciv-reader')
    if ($IncludeHostTools) { $components += 'agentciv-host', 'agentciv-conformance' }
    if ($current -ne 'none' -and [IO.File]::ReadAllText((Join-Path $root "versions/$current/components")).Contains('agentciv-host')) {
        $components = @('agentciv-archive', 'agentciv-reader', 'agentciv-host', 'agentciv-conformance')
    }
        if ($Restore) {
            if ($previous -eq 'none') { throw 'no previous version' }
            $resolvedVersion = $previous
            $components = [IO.File]::ReadAllText((Join-Path $root "versions/$resolvedVersion/components")).Trim() -split ' '
        } else {
            $target = Get-AgentCivTarget
            $temporary = Join-Path ([IO.Path]::GetTempPath()) ('agentciv-install-' + [Guid]::NewGuid().ToString('N'))
            [void][IO.Directory]::CreateDirectory($temporary)
            $base = 'https://github.com/blisspixel/AgentCiv/releases'
            $resolvedVersion = $RequestedVersion
            if ($resolvedVersion -eq 'latest') {
                Receive-AgentCivAsset "$base/latest/download/VERSION" (Join-Path $temporary 'VERSION') 64
                if ((Get-Item -LiteralPath (Join-Path $temporary 'VERSION')).Length -gt 64) { throw 'invalid release version' }
                $resolvedVersion = [IO.File]::ReadAllText((Join-Path $temporary 'VERSION')).Trim()
            }
            if (-not $resolvedVersion.StartsWith('v')) { $resolvedVersion = 'v' + $resolvedVersion }
            if (-not (Test-AgentCivVersion $resolvedVersion)) { throw 'version must be vX.Y.Z' }
            $checksumPath = Join-Path $temporary 'SHA256SUMS'
            Receive-AgentCivAsset "$base/download/$resolvedVersion/SHA256SUMS" $checksumPath 16384
            if ((Get-Item -LiteralPath $checksumPath).Length -gt 16384) { throw 'checksum manifest too large' }
            foreach ($resource in 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json') {
                $lines = @([IO.File]::ReadAllLines($checksumPath) | Where-Object { $_ -cmatch ('\A[0-9a-f]{64}  ' + [Regex]::Escape($resource) + '\z') })
                if ($lines.Count -ne 1) { throw 'missing or duplicate notice checksum' }
                $resourcePath = Join-Path $temporary $resource
                Receive-AgentCivAsset "$base/download/$resolvedVersion/$resource" $resourcePath
                if ((Get-AgentCivHash $resourcePath) -ne $lines[0].Substring(0, 64)) { throw 'license notice checksum mismatch' }
            }
            foreach ($component in $components) {
                $asset = "$component-$resolvedVersion-$target.exe"
                $lines = @([IO.File]::ReadAllLines($checksumPath) | Where-Object { $_ -cmatch ('^[0-9a-f]{64}  ' + [Regex]::Escape($asset) + '$') })
                if ($lines.Count -ne 1) { throw 'missing or duplicate checksum' }
                $expected = $lines[0].Substring(0, 64)
                $downloadPath = Join-Path $temporary "$component.exe"
                Receive-AgentCivAsset "$base/download/$resolvedVersion/$asset" $downloadPath
                if ((Get-AgentCivHash $downloadPath) -ne $expected) { throw 'binary checksum mismatch' }
                $actual = Get-AgentCivBinaryVersion $downloadPath
                if ($actual -cne "$component $($resolvedVersion.Substring(1))") { throw 'binary version or platform mismatch' }
            }
            $destination = Join-Path $root "versions/$resolvedVersion"
            Test-AgentCivPath $destination
            if (Test-Path -LiteralPath $destination) {
                if ([IO.File]::ReadAllText((Join-Path $destination 'components')).Trim() -ne ($components -join ' ')) { throw 'version installed with different components' }
                foreach ($component in $components) {
                    if ((Get-AgentCivHash (Join-Path $destination "$component.exe")) -ne (Get-AgentCivHash (Join-Path $temporary "$component.exe"))) { throw 'existing version differs from release' }
                }
                foreach ($resource in 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json') {
                    if ((Get-AgentCivHash (Join-Path $destination $resource)) -ne (Get-AgentCivHash (Join-Path $temporary $resource))) { throw 'existing notices differ from release' }
                }
            } else {
                $stage = Join-Path $root ('versions/.stage-' + [Guid]::NewGuid().ToString('N'))
                [void][IO.Directory]::CreateDirectory($stage)
                foreach ($component in $components) { Copy-Item -LiteralPath (Join-Path $temporary "$component.exe") -Destination $stage }
                foreach ($resource in 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json') { Copy-Item -LiteralPath (Join-Path $temporary $resource) -Destination $stage }
                Set-AgentCivFile (Join-Path $stage 'components') (($components -join ' ') + "`n")
                [IO.Directory]::Move($stage, $destination)
            }
        }
        foreach ($component in $components) {
            if ($component -cnotmatch '^agentciv-(archive|reader|host|conformance)$') { throw 'invalid components' }
            $relative = "bin/$component.cmd"
            $shimPath = Join-Path $root $relative
            Test-AgentCivPath $shimPath
            if ((Test-Path -LiteralPath $shimPath) -and -not $owned.Contains($relative)) { throw 'unowned command collision' }
            # Disable inherited delayed expansion; no installation path is embedded.
            $shim = "@echo off`r`nsetlocal DisableDelayedExpansion`r`nset /p agentciv_version=<`"%~dp0..\current`"`r`nif not exist `"%~dp0..\versions\%agentciv_version%\$component.exe`" (echo AgentCiv: component unavailable in selected version 1>&2 & exit /b 1)`r`n`"%~dp0..\versions\%agentciv_version%\$component.exe`" %*`r`n"
            Set-AgentCivFile $shimPath $shim
            $owned[$relative] = Get-AgentCivHash $shimPath
            $relative = "versions/$resolvedVersion/$component.exe"
            $owned[$relative] = Get-AgentCivHash (Join-Path $root $relative)
        }
        $relative = "versions/$resolvedVersion/components"
        $owned[$relative] = Get-AgentCivHash (Join-Path $root $relative)
        foreach ($resource in 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json') {
            $relative = "versions/$resolvedVersion/$resource"
            $owned[$relative] = Get-AgentCivHash (Join-Path $root $relative)
        }
        Set-AgentCivFile $ownershipPath (($owned.Keys | ForEach-Object { "$($owned[$_]) $_`n" }) -join '')
        if ($resolvedVersion -ne $current) { $previous = $current }
        Set-AgentCivFile (Join-Path $root 'previous') "$previous`n"
        # Atomic activation of the entire verified generation. Old executables
        # stay in their version directory, including when Windows locks them.
        Set-AgentCivFile (Join-Path $root 'current') "$resolvedVersion`n"
        if (-not $SkipPath) {
            $userPath = Get-AgentCivUserPath
            if (@($userPath -split ';' | Where-Object { $_ -ieq $bin }).Count -eq 0) {
                Set-AgentCivUserPath (($userPath.TrimEnd(';') + ';' + $bin).TrimStart(';'))
                Set-AgentCivFile (Join-Path $root 'path-owned') "user-path/1`n"
            }
        }
        Write-Output "Installed AgentCiv $resolvedVersion in $root"
        Write-Output "Restart your terminal, or add to this session: `$env:Path = '$($bin.Replace("'", "''"));' + `$env:Path"
        Write-Output "First run (offline): & '$($bin.Replace("'", "''"))\agentciv-archive.cmd' demo"
    } finally {
        if ($null -ne $temporary -and (Test-Path -LiteralPath $temporary)) {
            # The generated temporary directory is fixed for this invocation.
            foreach ($file in Get-ChildItem -LiteralPath $temporary -File) { Remove-Item -LiteralPath $file.FullName }
            Remove-Item -LiteralPath $temporary
        }
        if ($null -ne $installLock) {
            $installLock.Dispose()
            Remove-Item -LiteralPath $lockPath
            if ($Remove -and (Test-Path -LiteralPath $root) -and @(Get-ChildItem -LiteralPath $root -Force).Count -eq 0) { Remove-Item -LiteralPath $root }
        }
    }
}
if ($MyInvocation.InvocationName -ne '.') {
    Invoke-AgentCivInstall -RequestedVersion $Version -InstallPrefix $Prefix -IncludeHostTools:$WithHostTools -SkipPath:$NoModifyPath -Restore:$Rollback -Remove:$Uninstall
}
