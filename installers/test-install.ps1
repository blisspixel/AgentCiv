# Pester tests use function mocks; the production installer has no URL override.
$repository = Split-Path $PSScriptRoot -Parent
. (Join-Path $repository 'install.ps1')
$script:downloadRemainingImplementation = (Get-Command Get-AgentCivDownloadRemaining).ScriptBlock

Describe 'Native PowerShell installer' {
    BeforeEach {
        $script:fixtureRoot = Join-Path $TestDrive ([Guid]::NewGuid().ToString('N'))
        [void][IO.Directory]::CreateDirectory($script:fixtureRoot)
        $script:installPrefix = Join-Path $script:fixtureRoot 'installation with spaces'
        $script:requestedUrls = @()
        $script:brokenAsset = $false
        $script:duplicateChecksum = $false
        $script:wrongVersion = $false
        $script:userPath = 'C:\existing tools;C:\other'
        Mock Get-AgentCivUserPath { return $script:userPath }
        Mock Set-AgentCivUserPath { param($Value) $script:userPath = $Value }
        Mock Get-AgentCivTarget { 'x86_64-pc-windows-msvc' }
        Mock Receive-AgentCivAsset {
            param($Url, $Path, $MaxBytes)
            $script:requestedUrls += $Url
            if ($Url -cnotmatch '^https://github\.com/blisspixel/AgentCiv/releases/') { throw 'unexpected origin' }
            if ($Url.EndsWith('/VERSION')) { [IO.File]::WriteAllText($Path, "v0.1.0`n"); return }
            if ($Url.EndsWith('/SHA256SUMS')) {
                $release = ($Url -split '/')[-2]
                $lines = @()
                foreach ($component in 'agentciv-archive', 'agentciv-reader', 'agentciv-host', 'agentciv-conformance') {
                    $body = "$component $($release.Substring(1))"
                    $bytes = [Text.Encoding]::UTF8.GetBytes($body)
                    $algorithm = [Security.Cryptography.SHA256]::Create()
                    try { $digest = ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() } finally { $algorithm.Dispose() }
                    $lines += "$digest  $component-$release-x86_64-pc-windows-msvc.exe"
                }
                foreach ($resource in 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json') {
                    $bytes = [Text.Encoding]::UTF8.GetBytes("$resource fixture")
                    $algorithm = [Security.Cryptography.SHA256]::Create()
                    try { $digest = ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() } finally { $algorithm.Dispose() }
                    $lines += "$digest  $resource"
                }
                if ($script:duplicateChecksum) { $lines += $lines[0] }
                [IO.File]::WriteAllLines($Path, $lines)
            } else {
                if ([IO.Path]::GetFileName($Path) -in @('LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json')) {
                    [IO.File]::WriteAllText($Path, ([IO.Path]::GetFileName($Path) + ' fixture'), [Text.UTF8Encoding]::new($false))
                    return
                }
                $name = [IO.Path]::GetFileName($Path).Replace('.exe', '')
                $release = ($Url -split '/')[-2]
                $body = "$name $($release.Substring(1))"
                if ($script:brokenAsset) { $body += ' changed' }
                [IO.File]::WriteAllText($Path, $body, [Text.UTF8Encoding]::new($false))
            }
        }
        Mock Get-AgentCivBinaryVersion {
            param($Path)
            if ($script:wrongVersion) { return 'wrong 9.9.9' }
            return [IO.File]::ReadAllText($Path)
        }
    }
    It 'installs versioned native utilities without requiring PATH changes' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.1.0'
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'versions/v0.1.0/agentciv-reader.exe') | Should Be $true
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'bin/agentciv-host.cmd') | Should Be $false
        $script:requestedUrls.Count | Should Be 7
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion 0.1.0 -SkipPath
    }
    It 'updates, activates all selected binaries together, and rolls back' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.1.0 -SkipPath
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -IncludeHostTools -SkipPath
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'versions/v0.1.0/agentciv-archive.exe') | Should Be $true
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -SkipPath
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.1.0'
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -SkipPath
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.2.0'
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'versions/v0.2.0/agentciv-host.exe') | Should Be $true
    }
    It 'rejects an unowned release-compatible generation without adopting or changing files' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.1.0 -SkipPath
        $donor = Join-Path $script:fixtureRoot 'release-compatible donor'
        Invoke-AgentCivInstall -InstallPrefix $donor -RequestedVersion v0.2.0 -SkipPath
        $generation = Join-Path $script:installPrefix 'versions/v0.2.0'
        Copy-Item -LiteralPath (Join-Path $donor 'versions/v0.2.0') -Destination $generation -Recurse
        [IO.File]::WriteAllText((Join-Path $generation 'sentinel.txt'), 'unowned user data')
        $unchanged = [ordered]@{}
        foreach ($file in Get-ChildItem -LiteralPath $generation -File) { $unchanged[$file.FullName] = Get-AgentCivHash $file.FullName }
        foreach ($name in 'owned.sha256', 'current', 'previous', 'state') {
            $path = Join-Path $script:installPrefix $name
            $unchanged[$path] = Get-AgentCivHash $path
        }
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath } | Should Throw 'existing version contains unowned files'
        foreach ($path in $unchanged.Keys) { Get-AgentCivHash $path | Should Be $unchanged[$path] }
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.1.0'
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'owned.new') | Should Be $false
        $previousPath = Join-Path $script:installPrefix 'previous'
        $originalPrevious = [IO.File]::ReadAllText($previousPath)
        [IO.File]::WriteAllText($previousPath, "v0.2.0`n")
        $rollbackPreviousHash = Get-AgentCivHash $previousPath
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -SkipPath } | Should Throw 'existing version contains unowned files'
        Get-AgentCivHash $previousPath | Should Be $rollbackPreviousHash
        foreach ($path in $unchanged.Keys) { if ($path -ne $previousPath) { Get-AgentCivHash $path | Should Be $unchanged[$path] } }
        [IO.File]::WriteAllText($previousPath, $originalPrevious)
        # An existing generation must remain wholly owned, including each notice.
        $manifestPath = Join-Path $script:installPrefix 'owned.sha256'
        $originalManifest = [IO.File]::ReadAllText($manifestPath)
        $generationRows = @([IO.File]::ReadAllLines((Join-Path $donor 'owned.sha256')) | Where-Object { $_.Contains(' versions/v0.2.0/') })
        foreach ($missing in 'components', 'agentciv-archive.exe', 'NOTICE_INVENTORY.json') {
            $partialRows = @($generationRows | Where-Object { -not $_.EndsWith(" versions/v0.2.0/$missing") })
            [IO.File]::WriteAllText($manifestPath, $originalManifest + ($partialRows -join "`n") + "`n")
            $partialHash = Get-AgentCivHash $manifestPath
            { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath } | Should Throw 'existing version contains unowned files'
            Get-AgentCivHash $manifestPath | Should Be $partialHash
            foreach ($path in $unchanged.Keys) { if ($path -ne $manifestPath) { Get-AgentCivHash $path | Should Be $unchanged[$path] } }
            [IO.File]::WriteAllText($previousPath, "v0.2.0`n")
            { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -SkipPath } | Should Throw 'existing version contains unowned files'
            Get-AgentCivHash $manifestPath | Should Be $partialHash
            Get-AgentCivHash $previousPath | Should Be $rollbackPreviousHash
            foreach ($path in $unchanged.Keys) {
                if ($path -ne $manifestPath -and $path -ne $previousPath) { Get-AgentCivHash $path | Should Be $unchanged[$path] }
            }
            [IO.File]::WriteAllText($previousPath, $originalPrevious)
        }
        [IO.File]::WriteAllText($manifestPath, $originalManifest)
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath
        foreach ($path in $unchanged.Keys) {
            if ($path.StartsWith($generation + [IO.Path]::DirectorySeparatorChar)) { Get-AgentCivHash $path | Should Be $unchanged[$path] }
        }
    }
    It 'preserves a working installation when download verification fails' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.1.0 -SkipPath
        $script:brokenAsset = $true
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath } | Should Throw
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.1.0'
        $script:brokenAsset = $false; $script:duplicateChecksum = $true
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath } | Should Throw
        $script:duplicateChecksum = $false; $script:wrongVersion = $true
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath } | Should Throw
    }
    It 'refuses unsafe versions, foreign prefixes and unowned command collisions' {
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion "v0.1.0`n../../other" -SkipPath } | Should Throw
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion "v0.1.0`n" -SkipPath } | Should Throw
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion '../../other' -SkipPath } | Should Throw
        [void][IO.Directory]::CreateDirectory($script:installPrefix)
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath } | Should Throw
        { Invoke-AgentCivInstall -InstallPrefix relative -SkipPath } | Should Throw
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -Remove -SkipPath } | Should Throw
    }
    It 'checks every owned hash before uninstall and preserves unknown files' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath
        $binary = Join-Path $script:installPrefix 'versions/v0.1.0/agentciv-reader.exe'
        $original = [IO.File]::ReadAllText($binary)
        [IO.File]::AppendAllText($binary, ' user modification')
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath } | Should Throw
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'bin/agentciv-archive.cmd') | Should Be $true
        [IO.File]::WriteAllText($binary, $original)
        $keep = Join-Path $script:installPrefix 'versions/v0.1.0/private.txt'
        [void][IO.Directory]::CreateDirectory((Join-Path $script:installPrefix 'versions/unowned-empty'))
        [IO.File]::WriteAllText($keep, 'user data')
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath
        Test-Path -LiteralPath $keep | Should Be $true
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'versions/unowned-empty') | Should Be $true
        Test-Path -LiteralPath (Join-Path $script:installPrefix 'bin/agentciv-archive.cmd') | Should Be $false
    }
    It 'rejects forged ownership, missing files and corrupt activation state' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath
        $manifest = Join-Path $script:installPrefix 'owned.sha256'
        $original = [IO.File]::ReadAllText($manifest)
        [IO.File]::AppendAllText($manifest, ('a' * 64) + " ../private.txt`n")
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath } | Should Throw
        [IO.File]::WriteAllText($manifest, $original + $original)
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath } | Should Throw
        [IO.File]::WriteAllText($manifest, $original)
        [IO.File]::WriteAllText((Join-Path $script:installPrefix 'current'), "../../other`n")
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -SkipPath } | Should Throw
    }
    It 'rejects a missing rollback and uninstall without touching the parent' {
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Restore -SkipPath } | Should Throw
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath } | Should Throw
    }
    It 'adds only User PATH and removes only the entry it introduced' {
        $originalPath = $script:userPath
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix
        $script:userPath | Should Be ($originalPath + ';' + (Join-Path $script:installPrefix 'bin'))
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix
        $script:userPath | Should Be ($originalPath + ';' + (Join-Path $script:installPrefix 'bin'))
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove
        $script:userPath | Should Be $originalPath
        $script:userPath += ';' + (Join-Path $script:installPrefix 'bin')
        $preexistingPath = $script:userPath
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove
        $script:userPath | Should Be $preexistingPath
    }
    It 'does not overwrite an unowned shim or traverse a junction' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath
        $foreign = Join-Path $script:installPrefix 'bin/agentciv-host.cmd'
        [IO.File]::WriteAllText($foreign, 'unowned user command')
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -IncludeHostTools -SkipPath } | Should Throw
        [IO.File]::ReadAllText($foreign) | Should Be 'unowned user command'
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.1.0'
        $junction = Join-Path $script:fixtureRoot 'junction'
        New-Item -ItemType Junction -Path $junction -Value $script:installPrefix | Out-Null
        { Invoke-AgentCivInstall -InstallPrefix $junction -SkipPath } | Should Throw
    }
    It 'keeps running version files intact during update and rejects locked uninstall before deletion' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath
        $old = Join-Path $script:installPrefix 'versions/v0.1.0/agentciv-archive.exe'
        $handle = [IO.File]::Open($old, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        try {
            Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath
            { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath } | Should Throw
            Test-Path -LiteralPath (Join-Path $script:installPrefix 'bin/agentciv-archive.cmd') | Should Be $true
            [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.2.0'
        } finally { $handle.Dispose() }
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -Remove -SkipPath
    }
    It 'rejects a concurrent or stale lock and preserves all working files' {
        Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -SkipPath
        $lock = Join-Path $script:installPrefix '.install-lock'
        [IO.File]::WriteAllText($lock, 'other installer')
        { Invoke-AgentCivInstall -InstallPrefix $script:installPrefix -RequestedVersion v0.2.0 -SkipPath } | Should Throw
        [IO.File]::ReadAllText($lock) | Should Be 'other installer'
        [IO.File]::ReadAllText((Join-Path $script:installPrefix 'current')).Trim() | Should Be 'v0.1.0'
        Remove-Item -LiteralPath $lock
    }
}

Describe 'Bounded HTTPS asset downloader' {
    BeforeEach {
        $script:downloadPath = Join-Path $TestDrive ('asset-' + [Guid]::NewGuid().ToString('N'))
        $script:downloadRequests = @()
        $script:downloadResponses = @()
        Mock Get-AgentCivDownloadRemaining {
            param($Timer)
            & $script:downloadRemainingImplementation $Timer
        }
        function New-FixtureDownloadResponse([int]$Status = 200, [string]$Body = 'fixture bytes', [long]$Length = -1, [string]$Location = '') {
            $response = [PSCustomObject]@{
                StatusCode = $Status
                ContentLength = $Length
                Headers = @{ Location = $Location }
                Stream = [IO.MemoryStream]::new([Text.Encoding]::UTF8.GetBytes($Body))
                Disposed = $false
            }
            $response | Add-Member -MemberType ScriptMethod -Name GetResponseStream -Value { return $this.Stream }
            $response | Add-Member -MemberType ScriptMethod -Name Dispose -Value { $this.Disposed = $true; $this.Stream.Dispose() }
            return $response
        }
        Mock Get-AgentCivDownloadRequest {
            param($Uri, $TimeoutMilliseconds)
            $script:downloadRequests += [PSCustomObject]@{ Uri = $Uri; Timeout = $TimeoutMilliseconds }
            $request = [PSCustomObject]@{ Response = $script:downloadResponses[$script:downloadRequests.Count - 1] }
            $request | Add-Member -MemberType ScriptMethod -Name GetResponse -Value { return $this.Response }
            return $request
        }
    }
    It 'copies bounded bytes and releases its response and stream' {
        $response = New-FixtureDownloadResponse
        $script:downloadResponses = @($response)
        Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath 64
        [IO.File]::ReadAllText($script:downloadPath) | Should Be 'fixture bytes'
        $response.Disposed | Should Be $true
        $response.Stream.CanRead | Should Be $false
        $script:downloadRequests.Count | Should Be 1
        $script:downloadRequests[0].Timeout | Should BeGreaterThan 0
    }
    It 'rejects oversized declared and streamed bodies without leaving open files' {
        $declared = New-FixtureDownloadResponse -Length 65
        $script:downloadResponses = @($declared)
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath 64 } | Should Throw 'download too large'
        Test-Path -LiteralPath $script:downloadPath | Should Be $false
        $declared.Disposed | Should Be $true
        $script:downloadRequests = @()
        $streamed = New-FixtureDownloadResponse -Body ('a' * 70000)
        $script:downloadResponses = @($streamed)
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath 65536 } | Should Throw 'download too large'
        (Get-Item -LiteralPath $script:downloadPath).Length | Should Be 65536
        $streamed.Disposed | Should Be $true
        Remove-Item -LiteralPath $script:downloadPath
    }
    It 'rejects insecure URLs and limits before creating any request or output' {
        { Receive-AgentCivAsset 'http://github.com/example/asset' $script:downloadPath } | Should Throw 'insecure download URL'
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath 0 } | Should Throw 'invalid download byte limit'
        $script:downloadRequests.Count | Should Be 0
        Test-Path -LiteralPath $script:downloadPath | Should Be $false
    }
    It 'rejects insecure redirects and disposes the source response' {
        $response = New-FixtureDownloadResponse -Status 302 -Location 'http://example.com/asset'
        $script:downloadResponses = @($response)
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath } | Should Throw 'insecure download redirect'
        $script:downloadRequests.Count | Should Be 1
        $response.Disposed | Should Be $true
        Test-Path -LiteralPath $script:downloadPath | Should Be $false
    }
    It 'follows HTTPS redirects with one decreasing time budget' {
        $redirect = New-FixtureDownloadResponse -Status 302 -Location 'https://release-assets.githubusercontent.com/asset'
        $response = New-FixtureDownloadResponse
        $script:downloadResponses = @($redirect, $response)
        $script:remainingCalls = 0
        Mock Get-AgentCivDownloadRemaining { $script:remainingCalls++; return (180000 - $script:remainingCalls) }
        Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath
        $script:downloadRequests.Count | Should Be 2
        $script:downloadRequests[1].Uri.Host | Should Be 'release-assets.githubusercontent.com'
        $script:downloadRequests[1].Timeout | Should Be 179998
        $redirect.Disposed | Should Be $true
        $response.Disposed | Should Be $true
    }
    It 'caps redirect chains before creating a seventh request' {
        $script:downloadResponses = @(1..6 | ForEach-Object { New-FixtureDownloadResponse -Status 302 -Location '/another' })
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath } | Should Throw 'too many download redirects'
        $script:downloadRequests.Count | Should Be 6
        @($script:downloadResponses | Where-Object { -not $_.Disposed }).Count | Should Be 0
        Test-Path -LiteralPath $script:downloadPath | Should Be $false
    }
    It 'stops response reads after the shared deadline and closes partial output' {
        $response = New-FixtureDownloadResponse
        $script:downloadResponses = @($response)
        $script:remainingCalls = 0
        Mock Get-AgentCivDownloadRemaining {
            $script:remainingCalls++
            if ($script:remainingCalls -gt 1) { throw 'download timed out' }
            return 1
        }
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath } | Should Throw 'download timed out'
        $script:downloadRequests.Count | Should Be 1
        $response.Disposed | Should Be $true
        (Get-Item -LiteralPath $script:downloadPath).Length | Should Be 0
        Remove-Item -LiteralPath $script:downloadPath
    }
    It 'disposes a failed status or response stream before installation can continue' {
        $failed = New-FixtureDownloadResponse -Status 503
        $script:downloadResponses = @($failed)
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath } | Should Throw 'download failed'
        $failed.Disposed | Should Be $true
        $script:downloadRequests = @()
        $broken = New-FixtureDownloadResponse
        $broken | Add-Member -MemberType ScriptMethod -Name GetResponseStream -Force -Value { throw 'response stream failed' }
        $script:downloadResponses = @($broken)
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath } | Should Throw
        $broken.Disposed | Should Be $true
        Test-Path -LiteralPath $script:downloadPath | Should Be $false
        $source = Join-Path $TestDrive ('response-' + [Guid]::NewGuid().ToString('N'))
        [IO.File]::WriteAllText($source, 'local protocol-error response')
        $errorResponse = [Net.WebRequest]::Create([Uri]$source).GetResponse()
        $errorStream = $errorResponse.GetResponseStream()
        $script:protocolFailure = [Net.WebException]::new('fixture protocol failure', $null, [Net.WebExceptionStatus]::ProtocolError, $errorResponse)
        Mock Get-AgentCivDownloadRequest {
            $request = [PSCustomObject]@{ Failure = $script:protocolFailure }
            $request | Add-Member -MemberType ScriptMethod -Name GetResponse -Value { throw $this.Failure }
            return $request
        }
        { Receive-AgentCivAsset 'https://github.com/example/asset' $script:downloadPath } | Should Throw
        $errorStream.CanRead | Should Be $false
        Test-Path -LiteralPath $script:downloadPath | Should Be $false
        Remove-Item -LiteralPath $source
    }
}

Describe 'Native HTTP request timeout configuration' {
    It 'bounds both header acquisition and response stream reads' {
        $request = Get-AgentCivDownloadRequest ([Uri]'https://github.com/example/asset') 1234
        $request.Timeout | Should Be 1234
        $request.ReadWriteTimeout | Should Be 1234
        $request.AllowAutoRedirect | Should Be $false
        $request.Abort()
    }
}
