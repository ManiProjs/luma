param(
    [string]$Version = "",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$Repository = "ManiProjs/luma"
$NodeVersion = "22.23.5"
$BunVersion = "1.4.4"
$PlaywrightVersion = "1.65.0"
$RipgrepVersion = "1.18.0"
$LumaHome = Join-Path $env:LOCALAPPDATA "Luma"
$BinDir = Join-Path $LumaHome "bin"
$Runtime = Join-Path $LumaHome "runtime"
$Api = "https://api.github.com/repos/$Repository"

function Write-Step([string]$Message) { Write-Host "`n==> $Message" -ForegroundColor Cyan }
function Get-Download([string]$Url, [string]$Path) {
    Invoke-WebRequest -Uri $Url -OutFile $Path -Headers @{ "User-Agent" = "Luma-Installer" }
}
function Assert-Sha256([string]$Path, [string]$Expected) {
    $Actual = (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected.ToLowerInvariant()) { throw "SHA-256 verification failed for $Path" }
}

if ($Uninstall) {
    Write-Step "Removing Luma program files"
    Remove-Item -Recurse -Force $LumaHome -ErrorAction SilentlyContinue
    [Environment]::SetEnvironmentVariable("LUMA_HOME", $null, "User")
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User") -split ';' | Where-Object { $_ -and $_ -ne $BinDir }
    [Environment]::SetEnvironmentVariable("Path", ($UserPath -join ';'), "User")
    Write-Host "Removed Luma runtimes. User configuration and skills in %APPDATA%\luma were preserved."
    exit 0
}

if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -notin @("AMD64", "ARM64")) {
    throw "This installer currently supports 64-bit Windows x64."
}
if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { throw "Windows ARM64 bundles are not published yet; use the x64 emulation bundle only after it is available." }

$Temp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
    if (-not $Version) {
        Write-Step "Finding the latest Luma release"
        $Release = Invoke-RestMethod -Uri "$Api/releases?per_page=1" -Headers @{ "User-Agent" = "Luma-Installer" }
        $Version = $Release[0].tag_name
    }
    if ($Version -notmatch '^v\d+\.\d+\.\d+([.-][A-Za-z0-9.-]+)?$') { throw "Invalid release tag: $Version" }

    $Asset = "luma-x86_64-pc-windows-msvc.tar.gz"
    $BaseUrl = "https://github.com/$Repository/releases/download/$Version"
    Write-Step "Downloading Luma $Version"
    Get-Download "$BaseUrl/$Asset" (Join-Path $Temp $Asset)
    Get-Download "$BaseUrl/$Asset.sha256" (Join-Path $Temp "$Asset.sha256")
    $Expected = (Get-Content (Join-Path $Temp "$Asset.sha256") -Raw).Trim().Split(' ')[0]
    Assert-Sha256 (Join-Path $Temp $Asset) $Expected
    & tar.exe -xzf (Join-Path $Temp $Asset) -C $Temp
    if ($LASTEXITCODE -ne 0) { throw "Could not extract the Luma bundle." }
    $DesktopExecutable = Join-Path $Temp "luma\desktop\Luma.exe"
    if (-not (Test-Path $DesktopExecutable -PathType Leaf)) { throw "Release bundle does not contain the Windows Luma Desktop app." }

    $NodePlatform = "win-x64"
    $NodeArchive = "node-v$NodeVersion-$NodePlatform.zip"
    Write-Step "Installing Node.js $NodeVersion"
    Get-Download "https://nodejs.org/dist/v$NodeVersion/SHASUMS256.txt" (Join-Path $Temp "SHASUMS256.txt")
    $ChecksumLine = Get-Content (Join-Path $Temp "SHASUMS256.txt") | Where-Object { $_ -match "\s+\*?$([regex]::Escape($NodeArchive))$" } | Select-Object -First 1
    if (-not $ChecksumLine) { throw "Node.js checksum not found for $NodeArchive" }
    $NodeExpected = ($ChecksumLine -split '\s+')[0]
    Get-Download "https://nodejs.org/dist/v$NodeVersion/$NodeArchive" (Join-Path $Temp $NodeArchive)
    Assert-Sha256 (Join-Path $Temp $NodeArchive) $NodeExpected
    $NodeExtract = Join-Path $Temp "node-extract"
    Expand-Archive -Path (Join-Path $Temp $NodeArchive) -DestinationPath $NodeExtract
    $RuntimeStage = Join-Path $Temp "runtime"
    New-Item -ItemType Directory -Force -Path $RuntimeStage | Out-Null
    Move-Item (Join-Path $NodeExtract "node-v$NodeVersion-$NodePlatform") (Join-Path $RuntimeStage "node")

    $Npm = Join-Path $RuntimeStage "node\npm.cmd"
    Write-Step "Installing pinned Bun and Playwright runtimes"
    & $Npm install --prefix $RuntimeStage --no-save --no-audit --no-fund "bun@$BunVersion" "playwright@$PlaywrightVersion" "@vscode/ripgrep@$RipgrepVersion"
    if ($LASTEXITCODE -ne 0) { throw "Could not install Bun and Playwright." }
    $Node = Join-Path $RuntimeStage "node\node.exe"
    $Bun = Join-Path $RuntimeStage "node_modules\bun\bin\bun.exe"
    if ((& $Node --version) -ne "v$NodeVersion") { throw "Node.js version verification failed." }
    if ((& $Bun --version) -ne $BunVersion) { throw "Bun version verification failed." }
    $PlaywrightPackage = Join-Path $RuntimeStage "node_modules\playwright\package.json"
    if ((Get-Content $PlaywrightPackage -Raw | ConvertFrom-Json).version -ne $PlaywrightVersion) { throw "Playwright version verification failed." }

    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { throw "Git is required. Install Git for Windows, then re-run this installer." }
        winget install --id Git.Git --exact --silent --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -ne 0) { throw "Could not install Git for Windows." }
        $env:PATH = "$env:ProgramFiles\Git\cmd;$env:ProgramFiles\Git\bin;$env:ProgramFiles\Git\usr\bin;$env:PATH"
    }
    $Ripgrep = Join-Path $RuntimeStage "node_modules\@vscode\ripgrep-win32-x64\bin\rg.exe"
    if (-not (Test-Path $Ripgrep)) { throw "The platform-specific ripgrep binary was not installed." }
    & $Ripgrep --version | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "ripgrep verification failed." }
    git --version | Out-Null
    $PlaywrightCli = Join-Path $RuntimeStage "node_modules\playwright\cli.js"
    $env:PLAYWRIGHT_BROWSERS_PATH = Join-Path $env:LOCALAPPDATA "ms-playwright"
    & $Node $PlaywrightCli install chromium
    if ($LASTEXITCODE -ne 0) { throw "Could not install Playwright Chromium." }

    Write-Step "Installing Luma Core, Ink client, embedded dashboard, and Desktop app"
    New-Item -ItemType Directory -Force -Path $LumaHome, $BinDir, (Join-Path $LumaHome "lib") | Out-Null
    Remove-Item -Recurse -Force $BinDir, (Join-Path $LumaHome "lib"), (Join-Path $LumaHome "desktop"), $Runtime -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $BinDir, (Join-Path $LumaHome "lib"), (Join-Path $LumaHome "desktop") | Out-Null
    Copy-Item (Join-Path $Temp "luma\bin\*") $BinDir -Recurse -Force
    Copy-Item (Join-Path $Temp "luma\lib\*") (Join-Path $LumaHome "lib") -Recurse -Force
    Copy-Item (Join-Path $Temp "luma\desktop\*") (Join-Path $LumaHome "desktop") -Recurse -Force
    Move-Item $RuntimeStage $Runtime
    Copy-Item (Join-Path $Temp "luma\bundle.json") (Join-Path $LumaHome "bundle.json") -Force
    Copy-Item (Join-Path $Temp "luma\LICENSE") (Join-Path $LumaHome "LICENSE") -Force
    $BundleVersion = (Get-Content (Join-Path $LumaHome "bundle.json") -Raw | ConvertFrom-Json).version
    $ConfigDir = Join-Path $env:APPDATA "luma"
    $SkillsDir = Join-Path $ConfigDir "skills"
    New-Item -ItemType Directory -Force -Path $SkillsDir | Out-Null
    $DefaultSkill = Join-Path $SkillsDir "ask-question"
    if (-not (Test-Path $DefaultSkill)) {
        Copy-Item (Join-Path $Temp "luma\skills\ask-question") $DefaultSkill -Recurse
    }

    @"
@echo off
set "LUMA_HOME=%LOCALAPPDATA%\Luma"
set "LUMA_BINARY=%LUMA_HOME%\bin\luma-core.exe"
set "LUMA_NODE_BIN=%LUMA_HOME%\runtime\node\node.exe"
set "LUMA_PLAYWRIGHT_MODULE=%LUMA_HOME%\runtime\node_modules\playwright"
set "PLAYWRIGHT_BROWSERS_PATH=%LOCALAPPDATA%\ms-playwright"
set "DEV=false"
set "PATH=%LUMA_HOME%\runtime\node;%LUMA_HOME%\runtime\node_modules\.bin;%LUMA_HOME%\runtime\node_modules\@vscode\ripgrep-win32-x64\bin;%ProgramFiles%\Git\cmd;%ProgramFiles%\Git\bin;%ProgramFiles%\Git\usr\bin;%PATH%"
set "LUMA_VERSION=$BundleVersion"
"%LUMA_HOME%\runtime\node_modules\bun\bin\bun.exe" "%LUMA_HOME%\lib\luma-cli.js" %*
"@ | Set-Content -Encoding ASCII (Join-Path $BinDir "luma.cmd")

@"
@echo off
"%LOCALAPPDATA%\Luma\desktop\Luma.exe" %*
"@ | Set-Content -Encoding ASCII (Join-Path $BinDir "luma-desktop.cmd")

    [Environment]::SetEnvironmentVariable("LUMA_HOME", $LumaHome, "User")
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User") -split ';' | Where-Object { $_ }
    if ($UserPath -notcontains $BinDir) { $UserPath += $BinDir }
    [Environment]::SetEnvironmentVariable("Path", ($UserPath -join ';'), "User")

    Write-Step "Luma $Version installed"
    Write-Host "Commands: luma, luma teammates, luma dashboard, luma-desktop"
    Write-Host "Runtime: Bun $BunVersion, Node.js $NodeVersion, Playwright $PlaywrightVersion + Chromium, ripgrep $RipgrepVersion, Git"
    Write-Host "Open a new terminal, then run: luma setup"
}
finally {
    Remove-Item -Recurse -Force $Temp -ErrorAction SilentlyContinue
}
