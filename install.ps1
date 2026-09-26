# Run from an extracted repository. No Git checkout or administrator shell needed.
[CmdletBinding()]
param(
    [switch]$CheckOnly,
    [switch]$SkipPrerequisites
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Find-Program([string]$Name, [string]$Fallback = '') {
    if ($Fallback -and (Test-Path -LiteralPath $Fallback -PathType Leaf)) {
        return $Fallback
    }
    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    return $null
}

function Has-CppTools {
    $programFiles = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
    if (-not $programFiles) { return $false }
    $sdkRoot = Join-Path $programFiles 'Windows Kits\10'
    $installedRoots = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
    if ($installedRoots -and $installedRoots.PSObject.Properties['KitsRoot10']) {
        $sdkRoot = $installedRoots.KitsRoot10
    }
    $sdkLibraries = @(Get-ChildItem -Path (Join-Path $sdkRoot 'Lib\*\um\*\kernel32.lib') -ErrorAction SilentlyContinue)
    $sdkHeaders = @(Get-ChildItem -Path (Join-Path $sdkRoot 'Include\*\um\Windows.h') -ErrorAction SilentlyContinue)
    if ($sdkLibraries.Count -eq 0 -or $sdkHeaders.Count -eq 0) { return $false }
    if (Find-Program 'cl.exe') { return $true }
    $vswhere = Join-Path $programFiles 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return $false }
    $tools = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace(($tools -join '')))
}

function Has-StableRust([string]$Rustup) {
    if (-not $Rustup) { return $false }
    try {
        $null = & $Rustup run stable cargo --version 2>$null
        return $LASTEXITCODE -eq 0
    }
    catch { return $false }
}

function Confirm-Step([string]$Message) {
    Write-Host $Message
    $answer = Read-Host 'Continue? [y/N]'
    if ($answer -notmatch '^(?i:y|yes)$') { throw 'Cancelled. No further installation steps were run.' }
}

function Run-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    $code = $LASTEXITCODE
    if ($code -in @(1641, 3010)) {
        throw 'A prerequisite requested a Windows restart. Restart, then run this installer again.'
    }
    if ($code -ne 0) { throw "Command failed (exit $code): $Program. Correct the reported error before retrying." }
}

function Refresh-Path([string]$CargoBin) {
    # Process-only update; never change machine/user policy or persistent PATH.
    $env:PATH = @(
        $CargoBin,
        $env:PATH,
        [Environment]::GetEnvironmentVariable('Path', 'Machine'),
        [Environment]::GetEnvironmentVariable('Path', 'User')
    ) -join ';'
}

function Ensure-PlainDirectory([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    if ($item) {
        if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Preserving existing non-directory or linked path; configuration setup cannot continue through it: $Path"
        }
        return $true
    }
    New-Item -ItemType Directory -Path $Path -ErrorAction Stop | Out-Null
    return $true
}

function Copy-MissingFile([string]$Source, [string]$Destination) {
    # Get-Item -Force also recognizes existing hidden files and link entries.
    if (Get-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue) {
        Write-Host "Preserved: $Destination"
        return
    }
    $inputFile = $null
    $outputFile = $null
    $temporary = Join-Path ([IO.Path]::GetDirectoryName($Destination)) ('.mud-client-install-' + [IO.Path]::GetRandomFileName())
    $ownsTemporary = $false
    try {
        $inputFile = [IO.File]::OpenRead($Source)
        $outputFile = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $ownsTemporary = $true
        $inputFile.CopyTo($outputFile)
        $outputFile.Dispose()
        $outputFile = $null
        # Same-directory publication is atomic and refuses to replace a file or
        # link that appeared after the initial existence check.
        [IO.File]::Move($temporary, $Destination)
        $ownsTemporary = $false
    }
    finally {
        if ($outputFile) { $outputFile.Dispose() }
        if ($inputFile) { $inputFile.Dispose() }
        if ($ownsTemporary) { [IO.File]::Delete($temporary) }
    }
    Write-Host "Created: $Destination"
}

try {
    $sourceDir = $PSScriptRoot
    $safeConfig = Join-Path $sourceDir 'install\default-config.toml'
    foreach ($required in @('Cargo.toml', 'Cargo.lock', 'LICENSE', 'src\main.rs', 'scripts\init.lua', 'install\default-config.toml')) {
        if (-not (Test-Path -LiteralPath (Join-Path $sourceDir $required) -PathType Leaf)) {
            throw "Missing $required. Extract the complete source archive before running this installer."
        }
    }
    if (-not $env:USERPROFILE -or -not $env:APPDATA) { throw 'USERPROFILE and APPDATA must identify a normal Windows user account.' }
    if ($env:CARGO_HOME -and $env:CARGO_HOME -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\/]+[\\/][^\\/]+)') {
        throw 'CARGO_HOME must be an absolute drive or UNC path, not a relative path.'
    }
    $cargoRoot = if ($env:CARGO_HOME) { [IO.Path]::GetFullPath($env:CARGO_HOME) } else { Join-Path $env:USERPROFILE '.cargo' }
    $cargoBin = Join-Path $cargoRoot 'bin'
    $rustup = Find-Program 'rustup.exe' (Join-Path $cargoBin 'rustup.exe')
    $cargo = Find-Program 'cargo.exe' (Join-Path $cargoBin 'cargo.exe')
    $hasCpp = Has-CppTools
    $hasStable = Has-StableRust $rustup
    $winget = Find-Program 'winget.exe'
    $configRoot = Join-Path $env:APPDATA 'mud-client\mud-client\config'
    $binary = Join-Path $cargoBin 'mud-client.exe'

    if ($CheckOnly) {
        Write-Host "Source: $sourceDir"
        Write-Host "Executable destination: $binary"
        Write-Host "Configuration destination: $configRoot"
        Write-Host "Rustup: $([bool]$rustup); Cargo: $([bool]$cargo); Stable toolchain: $hasStable; C++ tools: $hasCpp; WinGet: $([bool]$winget)"
        Write-Host 'Check only: no downloads, installs, directory creation, or configuration writes performed.'
        if (-not $rustup -or -not $cargo -or -not $hasStable -or -not $hasCpp) { exit 1 }
        exit 0
    }

    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run as your normal user, not in an administrator shell. Prerequisite installers request elevation themselves if needed.'
    }
    Write-Host 'mud-client source installer for Windows'
    Write-Host 'Licensed under MIT; see LICENSE for permissions, conditions and warranty disclaimer. Third-party installer terms remain separate.'

    $needsRust = -not $rustup -or -not $cargo
    if ($SkipPrerequisites -and -not $hasStable) { throw 'Stable Rust is not installed. Run rustup toolchain install stable first, or rerun without -SkipPrerequisites.' }
    if ($needsRust -or -not $hasCpp) {
        if ($SkipPrerequisites) { throw 'Missing Rustup/Cargo or MSVC C++ tools. Install them first, or rerun without -SkipPrerequisites.' }
        if (-not $winget) { throw 'WinGet is unavailable. Install/update Microsoft App Installer, or install Rustup and Visual Studio Build Tools manually; see docs/installation.md.' }
        if (-not $hasCpp) {
            Confirm-Step 'Download Visual Studio 2022 Build Tools with the C++ workload and recommended Windows SDK components? This may request administrator permission and requires substantial disk space.'
            Run-Checked $winget @('install', '--id', 'Microsoft.VisualStudio.2022.BuildTools', '--exact', '--source', 'winget', '--interactive', '--override', '--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --wait')
            if (-not (Has-CppTools)) {
                throw 'C++ tools were not detected. In Visual Studio Installer, add Desktop development with C++, an MSVC toolset and a Windows SDK, restart if requested, then retry.'
            }
        }
        if ($needsRust) {
            Confirm-Step 'Download and install Rustup for your user account? Review and respond to any installer terms yourself.'
            Run-Checked $winget @('install', '--id', 'Rustlang.Rustup', '--exact', '--source', 'winget', '--interactive')
        }
        Refresh-Path $cargoBin
        $rustup = Find-Program 'rustup.exe' (Join-Path $cargoBin 'rustup.exe')
        $cargo = Find-Program 'cargo.exe' (Join-Path $cargoBin 'cargo.exe')
        if (-not $rustup -or -not $cargo) { throw 'Rustup/Cargo were not found after setup. Restart your terminal and retry.' }
    }

    if ($SkipPrerequisites) {
        Confirm-Step 'Build mud-client using installed stable Rust? Cargo may download dependencies and will replace the installed executable. Existing configuration and scripts will be preserved.'
    }
    else {
        Confirm-Step 'Update stable Rust and build mud-client? This downloads compiler/dependency packages and replaces the installed mud-client executable. Existing configuration and scripts will be preserved.'
    }
    Refresh-Path $cargoBin
    if (-not $SkipPrerequisites) { Run-Checked $rustup @('update', 'stable') }
    # Explicit --root honors CARGO_HOME while avoiding a hidden Cargo install.root override.
    Run-Checked $cargo @('+stable', 'install', '--path', $sourceDir, '--locked', '--root', $cargoRoot)
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Installed executable not found at $binary." }
    Run-Checked $binary @('--help')

    $first = Join-Path $env:APPDATA 'mud-client'
    $second = Join-Path $first 'mud-client'
    if ((Ensure-PlainDirectory $first) -and (Ensure-PlainDirectory $second) -and (Ensure-PlainDirectory $configRoot)) {
        Copy-MissingFile $safeConfig (Join-Path $configRoot 'config.toml')
        Copy-MissingFile (Join-Path $sourceDir 'LICENSE') (Join-Path $configRoot 'LICENSE')
        $scriptsDir = Join-Path $configRoot 'scripts'
        if (Ensure-PlainDirectory $scriptsDir) {
            foreach ($script in Get-ChildItem -LiteralPath (Join-Path $sourceDir 'scripts') -Filter '*.lua' -File) {
                Copy-MissingFile $script.FullName (Join-Path $scriptsDir $script.Name)
            }
        }
    }
    Write-Host ''
    Write-Host "Installed: $binary"
    Write-Host 'The fresh configuration connects to RoTS, uses standard input, and disables Lua. Existing configuration was not changed.'
    Write-Host 'No game connection was opened. Open a new terminal and run mud-client when ready.'
    Write-Host "If it is not on PATH, run: & '$binary'"
    exit 0
}
catch {
    Write-Error -Message $_.Exception.Message -ErrorAction Continue
    exit 1
}
