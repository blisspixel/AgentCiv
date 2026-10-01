[CmdletBinding()]
param([Parameter(Mandatory)][string]$ReleaseDirectory, [Parameter(Mandatory)][string]$ReleaseVersion)
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
. (Join-Path $repository 'install.ps1')
$releasePath = [IO.Path]::GetFullPath($ReleaseDirectory)
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('agentciv-native-smoke-' + [Guid]::NewGuid().ToString('N'))
$prefixPath = Join-Path $scratch 'installed with spaces'
[void][IO.Directory]::CreateDirectory($scratch)

# Test fixture only. The actual native version checker and shims are exercised.
function Receive-AgentCivAsset([string]$Url, [string]$Path, [long]$MaxBytes = 134217728) {
    if (-not $Url.StartsWith('https://github.com/blisspixel/AgentCiv/releases/download/' + $ReleaseVersion + '/')) { throw 'unexpected fixture origin' }
    $name = [IO.Path]::GetFileName(([Uri]$Url).AbsolutePath)
    if ($name -eq 'SHA256SUMS') {
        $lines = [IO.File]::ReadAllText((Join-Path $releasePath ('SHA256SUMS-' + (Get-AgentCivTarget))))
        foreach ($resource in 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json') {
            $sourcePath = if ($resource -eq 'LICENSE') { Join-Path $repository $resource } else { Join-Path $repository "release/$resource" }
            $lines += "$(Get-AgentCivHash $sourcePath)  $resource`n"
        }
        [IO.File]::WriteAllText($Path, $lines, [Text.UTF8Encoding]::new($false))
        return
    }
    if ($name -in @('LICENSE', 'THIRD_PARTY_NOTICES.txt', 'NOTICE_INVENTORY.json')) {
        $sourcePath = if ($name -eq 'LICENSE') { Join-Path $repository $name } else { Join-Path $repository "release/$name" }
        Copy-Item -LiteralPath $sourcePath -Destination $Path
        return
    }
    $sourcePath = Join-Path $releasePath $name
    if ((Get-Item -LiteralPath $sourcePath).Length -gt $MaxBytes) { throw 'fixture download too large' }
    Copy-Item -LiteralPath $sourcePath -Destination $Path
}
try {
    Invoke-AgentCivInstall -InstallPrefix $prefixPath -RequestedVersion $ReleaseVersion -IncludeHostTools -SkipPath
    $demo = & (Join-Path $prefixPath 'bin/agentciv-archive.cmd') demo
    if ($LASTEXITCODE -ne 0 -or ($demo -join "`n") -notmatch 'source_authenticity') { throw 'installed demo failed' }
    foreach ($name in 'archive', 'reader', 'host', 'conformance') {
        $actual = & (Join-Path $prefixPath "bin/agentciv-$name.cmd") --version
        if ($LASTEXITCODE -ne 0 -or $actual -cne "agentciv-$name $($ReleaseVersion.Substring(1))") { throw 'installed native version mismatch' }
    }
    Invoke-AgentCivInstall -InstallPrefix $prefixPath -RequestedVersion $ReleaseVersion -SkipPath
    Invoke-AgentCivInstall -InstallPrefix $prefixPath -Remove -SkipPath
    if (Test-Path -LiteralPath $prefixPath) { throw 'owned installation not fully removed' }
    Write-Output 'Native Windows installer fixture smoke passed.'
} finally {
    if (@(Get-ChildItem -LiteralPath $scratch -Force).Count -eq 0) { Remove-Item -LiteralPath $scratch }
}
