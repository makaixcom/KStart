param(
    [Parameter(Mandatory)]
    [ValidatePattern('^v[A-Za-z0-9][A-Za-z0-9._-]*$')]
    [string]$Tag,
    [Parameter(Mandatory)]
    [string]$OutputPath,
    [ValidatePattern('^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$')]
    [string]$Repository = 'makaixcom/KStart',
    [string]$RepositoryPath = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
$RepositoryPath = [IO.Path]::GetFullPath($RepositoryPath).Replace('\', '/')
function Invoke-ReleaseGit {
    param([string[]]$GitArguments)
    $result = & git -c "safe.directory=$RepositoryPath" -C $RepositoryPath @GitArguments
    if ($LASTEXITCODE -ne 0) { throw "读取 Git 历史失败：$($GitArguments -join ' ')" }
    return $result
}

$commit = Invoke-ReleaseGit -GitArguments @('rev-parse', '--verify', "refs/tags/$Tag^{commit}")
# 从父提交寻找最近的版本标签，避免将当前版本或同一提交上的别名作为比较起点。
$parents = (Invoke-ReleaseGit -GitArguments @('rev-list', '--parents', '-n', '1', $commit)) -split ' '
$previous = $null
if ($parents.Count -gt 1) {
    $candidate = & git -c "safe.directory=$RepositoryPath" -C $RepositoryPath describe --tags --abbrev=0 --match 'v[0-9]*' $parents[1] 2>$null
    if ($LASTEXITCODE -eq 0) { $previous = $candidate }
}
$range = if ($previous) { "$previous..$commit" } else { $commit }
$history = (Invoke-ReleaseGit -GitArguments @('log', '--no-merges', '--reverse', '--format=%H%x1f%s%x1f%b%x1e', $range)) -join "`n"
$groups = [ordered]@{
    breaking = @{ Title = '不兼容变更（Breaking Changes）'; Items = [Collections.Generic.List[string]]::new() }
    feat = @{ Title = '新增功能（Features）'; Items = [Collections.Generic.List[string]]::new() }
    fix = @{ Title = '问题修复（Bug Fixes）'; Items = [Collections.Generic.List[string]]::new() }
    perf = @{ Title = '性能优化（Performance）'; Items = [Collections.Generic.List[string]]::new() }
    docs = @{ Title = '文档更新（Documentation）'; Items = [Collections.Generic.List[string]]::new() }
    other = @{ Title = '其他更新（Other Changes）'; Items = [Collections.Generic.List[string]]::new() }
}
foreach ($record in ($history -split [char]0x1e)) {
    if ([string]::IsNullOrWhiteSpace($record)) { continue }
    $fields = $record.Trim() -split [char]0x1f, 3
    if ($fields.Count -lt 3) { throw '无法解析 Git 提交记录。' }
    $hash, $subject, $body = $fields
    $key = 'other'
    $label = $subject
    if ($subject -match '^(?<type>[a-z]+)(?:\((?<scope>[^)]+)\))?(?<breaking>!)?:\s*(?<text>.+)$') {
        $type, $scope, $breaking, $label = $Matches.type, $Matches.scope, $Matches.breaking, $Matches.text
        $key = switch ($type) {
            'feat' { 'feat' }
            'fix' { 'fix' }
            'bugfix' { 'fix' }
            'perf' { 'perf' }
            'docs' { 'docs' }
            default { 'other' }
        }
        if ($scope) { $label = "$scope : $label" }
        if ($breaking) { $key = 'breaking' }
    }
    if ($body -match '(?m)^BREAKING[ -]CHANGE:\s*') { $key = 'breaking' }
    # 提交标题作为文本展示，避免 Markdown 符号改变更新说明布局。
    $label = $label -replace '([\\`*_{}\[\]<>#|])', '\$1'
    $groups[$key].Items.Add("- $label ([$($hash.Substring(0, 7))](https://github.com/$Repository/commit/$hash))")
}
$lines = [Collections.Generic.List[string]]::new()
$lines.Add("## KStart $Tag 更新内容")
foreach ($group in $groups.Values) {
    if ($group.Items.Count -eq 0) { continue }
    $lines.Add('')
    $lines.Add("### $($group.Title)")
    $lines.Add('')
    $lines.AddRange($group.Items)
}
if (!($groups.Values | Where-Object { $_.Items.Count -gt 0 })) {
    $lines.Add('')
    $lines.Add('本版本没有新增的非合并提交。')
}
$lines.Add('')
$changelog = if ($previous) { "https://github.com/$Repository/compare/$previous...$Tag" } else { "https://github.com/$Repository/commits/$Tag" }
$lines.Add("**完整更新记录**：[$(if ($previous) { "$previous → $Tag" } else { $Tag })]($changelog)")
[IO.File]::WriteAllText($OutputPath, ($lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
Write-Output "已生成 $Tag 更新说明：$OutputPath"

