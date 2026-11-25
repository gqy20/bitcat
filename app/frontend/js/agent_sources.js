// 编程助手来源的唯一前端映射：设置页、Agent Watch 浮窗共用。
// 新增受看管的 CLI 时只改这里的 SOURCES；Rust 侧真值在
// core/src/agent_session.rs 的 AgentSource（display_name）。
(function () {
  'use strict';

  const SOURCES = [
    { id: "claude_code", label: "Claude Code", compact: "Claude", badge: "CC" },
    { id: "codex", label: "Codex", compact: "Codex", badge: "CX" },
    { id: "zcode", label: "ZCode", compact: "ZCode", badge: "Z" },
    { id: "pi", label: "pi", compact: "pi", badge: "PI" },
    { id: "opencode", label: "opencode", compact: "opencode", badge: "OC" },
  ];
  const BY_ID = new Map(SOURCES.map(item => [item.id, item]));
  const BY_LABEL = new Map(SOURCES.map(item => [item.label, item]));

  window.AgentSources = {
    list: SOURCES,
    // snake_case source id → 展示名，未知 id 原样返回。
    label(id) {
      return BY_ID.get(id)?.label || id || "Agent";
    },
    // 完整展示名 → 浮窗窄条用的短名。
    compact(label) {
      return BY_LABEL.get(label)?.compact || label;
    },
    // 窄卡片里的中性来源徽标；悬浮/无障碍文本仍使用完整名称。
    badge(id) {
      return BY_ID.get(id)?.badge || String(id || "?").slice(0, 2).toUpperCase();
    },
  };
})();
