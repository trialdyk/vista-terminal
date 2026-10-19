# vista installer for Windows.
#   irm https://raw.githubusercontent.com/trialdyk/vista-terminal/main/install.ps1 | iex
#
# Downloads the latest release into %LOCALAPPDATA%\Programs\vista, adds that
# folder to your user PATH and runs `vista init`. Undo with `vista uninstall`,
# then delete the folder.

$ErrorActionPreference = 'Stop'
$repo = 'trialdyk/vista-terminal'

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
$asset = "vista-$arch-pc-windows-msvc.zip"
$url = "https://github.com/$repo/releases/latest/download/$asset"
$dir = Join-Path $env:LOCALAPPDATA 'Programs\vista'
$zip = Join-Path ([IO.Path]::GetTempPath()) $asset

Write-Host "Downloading $asset"
Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
New-Item -ItemType Directory -Force $dir | Out-Null
Expand-Archive $zip -DestinationPath $dir -Force
Remove-Item $zip

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
    $newPath = (($userPath -split ';') + $dir | Where-Object { $_ }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "Added $dir to your PATH"
}
$env:Path = "$env:Path;$dir"

& (Join-Path $dir 'vista.exe') init
