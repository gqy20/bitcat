//! hook / 转发器安装公共工具。
//!
//! claude_hooks、codex_hooks、pi_hooks、opencode_hooks 都需要"原子写入脚本 +
//! 内容比对 + 写前备份"三件套。集中在这里实现，避免每个安装器各维护一份
//! 细微不同的文件操作逻辑。所有函数失败时返回带上下文的中文错误信息，
//! 不静默吞错。

use std::path::Path;
use tracing::info;

/// 内容一致时跳过写入，用于幂等安装。
pub fn file_content_matches(path: &Path, expected: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|actual| actual == expected)
        .unwrap_or(false)
}

/// 原子写入：先写同目录临时文件再 rename，避免半写入文件被宿主 agent 读到。
pub fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {e}"))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("替换文件失败: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("保存文件失败: {e}"))
}

/// 覆盖用户配置前先备份；时间戳后缀由调用方决定扩展名风格。
pub fn backup_if_exists(path: &Path, backup_suffix: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_file_name(format!("{backup_suffix}-{stamp}"));
    std::fs::copy(path, &backup).map_err(|e| format!("备份 {} 失败: {e}", path.display()))?;
    Ok(())
}

/// 清理 hooks 目录中已不再被任何配置引用的遗留脚本。
///
/// 只删除文件名匹配已知旧产品前缀的 `.ps1`（ai-pad-、ooclaw-），
/// 不碰当前 bitcat 脚本和用户自己的文件。返回删除数量。
pub fn cleanup_stale_scripts(hooks_dir: &Path) -> usize {
    const STALE_PREFIXES: &[&str] = &["ai-pad-", "ooclaw-"];
    let Ok(entries) = std::fs::read_dir(hooks_dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".ps1") {
            continue;
        }
        if STALE_PREFIXES.iter().any(|p| name.starts_with(p))
            && std::fs::remove_file(entry.path()).is_ok()
        {
            info!(file = %name, "removed stale hook script");
            removed += 1;
        }
    }
    removed
}

/// 生成 Claude Code / Codex / ZCode 共用的只读 PowerShell 转发脚本。
///
/// 脚本只把 stdin JSON 包成 Agent Watch envelope 后发到本机 TCP；单次连接
/// 250ms 超时，监控未运行时用状态文件把探测降到 45 秒一次，始终不阻塞宿主。
pub fn powershell_hook_script(marker: &str, source: &str, agent_label: &str, port: u16) -> String {
    format!(
        r#"# {marker}
# Installed by BitCat. Read-only: sends {agent_label} hook JSON to local BitCat monitor.
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = "Stop"

$payload = $null
$logDir = Join-Path $HOME ".bitcat\logs"
$logFile = Join-Path $logDir "agent_hook_bridge.jsonl"
$stateDir = Join-Path $HOME ".bitcat\hooks\state"
$stateFile = Join-Path $stateDir "monitor-state"
$probeIntervalSec = 45

function Get-Field($obj, [string[]]$names) {{
  foreach ($name in $names) {{
    if ($null -ne $obj -and $obj.PSObject.Properties.Name -contains $name) {{
      return [string]$obj.$name
    }}
  }}
  return $null
}}

function Write-BitCatHookLog([string]$status, [string]$detail, [string]$target, [string]$errorText, [int]$attempt) {{
  try {{
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    if ((Test-Path $logFile) -and ((Get-Item $logFile).Length -gt 1048576)) {{
      Move-Item -Force $logFile "$logFile.1"
    }}
    $entry = [ordered]@{{
      ts = (Get-Date).ToUniversalTime().ToString("o")
      source = "{source}"
      status = $status
      detail = $detail
      target = $target
      error = $errorText
      attempt = $attempt
      hook = Get-Field $payload @("hook_event_name", "hookEventName", "event")
      session_id = Get-Field $payload @("session_id", "sessionId")
      tool = Get-Field $payload @("tool_name", "toolName")
      bytes = if ($null -eq $raw) {{ 0 }} else {{ $raw.Length }}
    }} | ConvertTo-Json -Depth 8 -Compress
    Add-Content -LiteralPath $logFile -Value $entry -Encoding UTF8
  }} catch {{}}
}}

function Get-BitCatErrorText($errorRecord) {{
  if ($null -eq $errorRecord -or $null -eq $errorRecord.Exception) {{ return $null }}
  $type = $errorRecord.Exception.GetType().Name
  $message = $errorRecord.Exception.Message
  if ([string]::IsNullOrWhiteSpace($message)) {{ return $type }}
  return "$type`: $message"
}}

function Get-StateField([string]$key) {{
  try {{
    if (-not (Test-Path $stateFile)) {{ return $null }}
    foreach ($line in Get-Content -LiteralPath $stateFile -ErrorAction Stop) {{
      $parts = $line -split "=", 2
      if ($parts.Length -eq 2 -and $parts[0] -eq $key) {{ return $parts[1] }}
    }}
  }} catch {{}}
  return $null
}}

function Write-BitCatMonitorState([string]$status, [string]$target) {{
  try {{
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    @(
      "status=$status"
      "target=$target"
      "checked_at=$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())"
    ) | Set-Content -LiteralPath $stateFile -Encoding UTF8
  }} catch {{}}
}}

function Test-BitCatProbeAllowed([string]$target) {{
  $status = Get-StateField "status"
  $stateTarget = Get-StateField "target"
  $checkedAt = Get-StateField "checked_at"
  if ($status -ne "down" -or [string]::IsNullOrWhiteSpace($checkedAt)) {{ return $true }}
  if ($stateTarget -ne $target) {{ return $true }}
  $parsed = 0L
  if (-not [Int64]::TryParse($checkedAt, [ref]$parsed)) {{ return $true }}
  return (([DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - $parsed) -ge $probeIntervalSec)
}}

function Send-BitCatEnvelope([string]$body, [int]$port) {{
  $target = "127.0.0.1:$port"
  $lastError = $null
  if (-not (Test-BitCatProbeAllowed $target)) {{ return }}
  $client = $null
  $stream = $null
  try {{
    $client = [System.Net.Sockets.TcpClient]::new()
    $connect = $client.BeginConnect("127.0.0.1", $port, $null, $null)
    if (-not $connect.AsyncWaitHandle.WaitOne(250)) {{
      throw "connect timeout after 250ms"
    }}
    $client.EndConnect($connect)
    $stream = $client.GetStream()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($body)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush()
    $client.Client.Shutdown([System.Net.Sockets.SocketShutdown]::Send)
    Write-BitCatMonitorState "up" $target
    Write-BitCatHookLog "sent" $target $target $null 1
    return
  }} catch {{
    $lastError = Get-BitCatErrorText $_
  }} finally {{
    if ($null -ne $stream) {{ $stream.Dispose() }}
    if ($null -ne $client) {{ $client.Dispose() }}
  }}
  Write-BitCatMonitorState "down" $target
  Write-BitCatHookLog "failed" "send-failed" $target $lastError 1
}}

$raw = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($raw)) {{ exit 0 }}

try {{
  $payload = $raw | ConvertFrom-Json
  $envelope = [ordered]@{{
    schema = "bitcat.agent-hook.v1"
    source = "{source}"
    machine = $env:COMPUTERNAME
    payload = $payload
  }} | ConvertTo-Json -Depth 100 -Compress

  Send-BitCatEnvelope $envelope {port}
}} catch {{
  Write-BitCatHookLog "failed" "prepare-failed" "127.0.0.1:{port}" (Get-BitCatErrorText $_) 0
  # Keep {agent_label} hooks non-blocking for the user.
  exit 0
}}
"#,
        marker = marker,
        source = source,
        agent_label = agent_label,
        port = port
    )
}
