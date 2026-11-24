param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$')]
    [string]$Version,
    [string]$BinaryPath = 'target/x86_64-pc-windows-msvc/release/kstart.exe',
    [string]$OutputDirectory = 'dist'
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$executable = if ([IO.Path]::IsPathRooted($BinaryPath)) { $BinaryPath } else { Join-Path $projectRoot $BinaryPath }
if (!(Test-Path -LiteralPath $executable -PathType Leaf)) { throw "找不到待打包程序：$executable" }
$outputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory } else { Join-Path $projectRoot $OutputDirectory }
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$packageName = "KStart-$Version-windows-x64"
$stageRoot = Join-Path ([IO.Path]::GetTempPath()) ('kstart-package-' + [guid]::NewGuid().ToString('N'))
$packageRoot = Join-Path $stageRoot $packageName
New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
Copy-Item -LiteralPath $executable -Destination (Join-Path $packageRoot 'kstart.exe')
foreach ($resource in @('skin', 'assets', 'icons')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $resource) -Destination $packageRoot -Recurse
}
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $packageRoot
foreach ($license in @('LICENSE', 'LICENSE.md', 'LICENSE.txt')) {
    $licensePath = Join-Path $projectRoot $license
    if (Test-Path -LiteralPath $licensePath -PathType Leaf) { Copy-Item -LiteralPath $licensePath -Destination $packageRoot }
}
# user/ 是运行期数据。发行包仅携带程序和公共资源，首次运行时生成默认配置。
$archive = Join-Path $outputRoot "$packageName.zip"
if (Test-Path -LiteralPath $archive) { throw "输出文件已存在：$archive" }
Compress-Archive -LiteralPath $packageRoot -DestinationPath $archive -CompressionLevel Optimal
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $packageName.zip" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
Write-Output $archive
Write-Output "$archive.sha256"
