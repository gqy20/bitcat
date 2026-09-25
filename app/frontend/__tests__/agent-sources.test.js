import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import fs from 'node:fs';
import { resolve } from 'node:path';

// 唯一来源映射的真值测试：settings 与 agent_watch 浮窗都从这里拿展示名，
// 新增 CLI 时本测试强制同步 compact 短名，避免浮窗窄条出现全名截断。
const script = fs.readFileSync(resolve(process.cwd(), 'js/agent_sources.js'), 'utf8');

function load() {
  const dom = new JSDOM('<!doctype html><body></body>', {
    url: 'http://localhost/agent_sources.html',
    runScripts: 'outside-only',
  });
  dom.window.eval(script);
  return dom.window.AgentSources;
}

describe('AgentSources shared mapping', () => {
  it('exposes the four supported connectors in display order', () => {
    const sources = load();
    expect(sources.list.map(item => item.id)).toEqual([
      'claude_code', 'codex', 'pi', 'opencode',
    ]);
  });

  it('maps snake_case ids to display labels with a fallback', () => {
    const sources = load();
    expect(sources.label('claude_code')).toBe('Claude Code');
    expect(sources.label('codex')).toBe('Codex');
    expect(sources.label('pi')).toBe('pi');
    expect(sources.label('opencode')).toBe('opencode');
    expect(sources.label('gemini_cli')).toBe('gemini_cli');
    expect(sources.label('')).toBe('Agent');
  });

  it('maps display labels to compact names for the floating window', () => {
    const sources = load();
    expect(sources.compact('Claude Code')).toBe('Claude');
    expect(sources.compact('Codex')).toBe('Codex');
    expect(sources.compact('pi')).toBe('pi');
    expect(sources.compact('opencode')).toBe('opencode');
    expect(sources.compact('未知来源')).toBe('未知来源');
  });
});
