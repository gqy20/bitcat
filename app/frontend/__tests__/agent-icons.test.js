import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import fs from 'node:fs';
import { resolve } from 'node:path';

const script = fs.readFileSync(resolve(process.cwd(), 'js/agent_icons.js'), 'utf8');

function load() {
  const dom = new JSDOM('<!doctype html><body></body>', {
    url: 'http://localhost/agent-icons.html',
    runScripts: 'outside-only',
  });
  dom.window.eval(script);
  return dom.window.AgentIcons;
}

describe('shared agent state icons', () => {
  it('maps structured action labels to stable scene icons', () => {
    const icons = load();
    expect(icons.iconForSession({ status: 'tool_running', display: { action_label: 'Shell' } })).toBe('shell');
    expect(icons.iconForSession({ status: 'tool_running', display: { action_label: 'Patch' } })).toBe('edit');
    expect(icons.iconForSession({ status: 'tool_running', display: { action_label: 'Read' } })).toBe('read');
    expect(icons.iconForSession({ status: 'tool_running', display: { action_label: 'Search' } })).toBe('search');
    expect(icons.iconForSession({ status: 'working', background: true })).toBe('agent');
  });

  it('uses explicit state icons for terminal and permission states', () => {
    const icons = load();
    expect(icons.iconForSession({ status: 'done' })).toBe('done');
    expect(icons.iconForSession({ status: 'error' })).toBe('error');
    expect(icons.iconForSession({ status: 'interrupted' })).toBe('interrupted');
    expect(icons.iconForSession({ status: 'waiting' })).toBe('permission');
    expect(icons.iconForSession({ status: 'compacting' })).toBe('compacting');
  });

  it('renders accessible inline SVG with semantic state classes', () => {
    const icons = load();
    const html = icons.render('shell');
    expect(html).toContain('<svg');
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain('stroke="currentColor"');
    expect(html).toContain('state-muted');
    expect(icons.render('done')).toContain('state-success');
    expect(icons.render('error')).toContain('state-danger');
  });

  it('maps statuses to the shared semantic color tokens', () => {
    const icons = load();
    expect(icons.stateForStatus('working')).toBe('active');
    expect(icons.stateForStatus('tool_running')).toBe('active');
    expect(icons.stateForStatus('waiting')).toBe('waiting');
    expect(icons.stateForStatus('done')).toBe('success');
    expect(icons.stateForStatus('error')).toBe('danger');
    expect(icons.stateForStatus('idle')).toBe('muted');
  });
});
