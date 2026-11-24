# 使用独立的临时 Git 仓库验证说明生成，不修改项目标签或工作区。
$ErrorActionPreference = 'Stop'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('kstart-notes-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
function Invoke-FixtureGit {
    param([string[]]$GitArguments)
    & git -C $fixture -c "safe.directory=$($fixture.Replace('\', '/'))" -c user.name=KStart -c user.email=test@example.invalid -c commit.gpgsign=false @GitArguments
    if ($LASTEXITCODE -ne 0) { throw '创建 Git 测试历史失败。' }
}
function Assert-Notes {
    param([bool]$Condition, [string]$Message)
    if (!$Condition) { throw $Message }
}
Invoke-FixtureGit -GitArguments @('init', '--quiet')
Invoke-FixtureGit -GitArguments @('commit', '--allow-empty', '--quiet', '-m', '最初版本')
Invoke-FixtureGit -GitArguments @('tag', '-a', 'v0.1.0', '-m', '初版')
$output = Join-Path $fixture 'notes.md'
$generator = Join-Path $PSScriptRoot 'generate-release-notes.ps1'
& $generator -Tag v0.1.0 -RepositoryPath $fixture -OutputPath $output
$notes = Get-Content -LiteralPath $output -Raw
Assert-Notes ($notes.Contains('最初版本') -and $notes.Contains('/commits/v0.1.0')) '首版应包含全部历史及历史链接。'
foreach ($title in @('feat(search): 支持搜索', 'fix: 修复边框', 'bugfix(ui): 修复焦点', 'perf: 优化绘制', 'docs: 更新说明', 'chore: 更新工作流', '普通中文提交', 'feat!: 修改格式')) {
    Invoke-FixtureGit -GitArguments @('commit', '--allow-empty', '--quiet', '-m', $title)
}
Invoke-FixtureGit -GitArguments @('commit', '--allow-empty', '--quiet', '-m', 'fix: 修改接口', '-m', 'BREAKING CHANGE: 旧接口不再支持')
Invoke-FixtureGit -GitArguments @('tag', 'v0.2.0')
Invoke-FixtureGit -GitArguments @('tag', 'v0.2.0-alias')
& $generator -Tag v0.2.0 -RepositoryPath $fixture -OutputPath $output
$notes = Get-Content -LiteralPath $output -Raw
Assert-Notes (!$notes.Contains('最初版本') -and $notes.Contains('/compare/v0.1.0...v0.2.0')) '应只汇总版本间提交，同提交别名不应成为基准。'
foreach ($section in @('Features', 'Bug Fixes', 'Performance', 'Documentation', 'Other Changes', 'Breaking Changes')) {
    Assert-Notes ($notes.Contains($section)) "缺少分类：$section"
}
Assert-Notes ($notes.Contains('search : 支持搜索') -and $notes.Contains('ui : 修复焦点')) '应保留提交作用域。'
Assert-Notes ($notes.Contains('普通中文提交')) '非规范提交不应丢失。'
$breakingSection = ($notes -split '### 新增功能')[0]
Assert-Notes ($breakingSection.Contains('修改接口') -and $breakingSection.Contains('修改格式')) '标题及正文不兼容标记均应生效。'
& $generator -Tag v0.2.0 -RepositoryPath $fixture -OutputPath $output
Assert-Notes ((Get-Content -LiteralPath $output -Raw) -ceq $notes) '重跑应生成相同结果。'
Write-Output '更新说明验证通过：首版、版本范围、标签别名、全部分类、作用域、不兼容标记、中文与重复生成。'
