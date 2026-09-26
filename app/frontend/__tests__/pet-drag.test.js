import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { PetStateMachine } from '../js/pet.js';
import { buildRuntimeFromManifest } from '../js/sprite-loader.js';
import { previewFrame } from '../tools/pixel-preview-playback.js';

const slugs = ['tuxedo','ginger','tabby','calico','cow','black','white','british-blue','siamese','ragdoll','maine-coon'];
const load = slug => JSON.parse(readFileSync(path.join(process.cwd(), '__fixtures__/pets/cat-pixel-'+slug+'/manifest.json'), 'utf8'));
const runtime = m => buildRuntimeFromManifest(m, { width:m.sprite.columns*96, height:m.sprite.rows*96 });
const create = (slug='tuxedo') => { const r=runtime(load(slug));return new PetStateMachine({stateConfig:r.stateConfig,actionConfig:r.actionConfig}); };
const duration = c => c.frames.reduce((sum,f)=>sum+f.duration,0);

describe('authored pickup / hold / drop', () => {
  it.each(slugs)('%s stays airborne until release and then lands', slug => {
    const p=create(slug),x=p.x;
    expect(p.beginDrag()).toBe(true);expect(p.visualState()).toBe('pickup');
    p.update(30_000);
    expect(p.dragHeld).toBe(true);expect(p.dragPhase).toBe('hold');expect(p.visualState()).toBe('dragging');
    expect(p.x).toBe(x);expect(p.targetX).toBeNull();
    p.endDrag();expect(p.visualState()).toBe('drop');
    p.update(duration(p.actionConfig.drop)-1);expect(p.visualState()).toBe('drop');
    p.update(1);expect(p.visualState()).toBe('idle');expect(p.dragPhase).toBeNull();
  });

  it('releases during pickup without waiting for pickup to finish', () => {
    const p=create();p.beginDrag();p.update(40);p.endDrag();
    expect(p.visualState()).toBe('drop');expect(p.dragHeld).toBe(false);
    p.update(100);expect(p.endDrag()).toBe(false);expect(p.actionTimeMs).toBe(100);
  });

  it('new pickup interrupts an unfinished landing', () => {
    const p=create();p.beginDrag();p.update(1000);p.endDrag();p.update(120);p.beginDrag();
    expect(p.visualState()).toBe('pickup');p.update(10_000);
    expect(p.dragHeld).toBe(true);expect(p.visualState()).toBe('dragging');
  });

  it('queues sleep while held and restores it after landing', () => {
    const p=create();p.walkTo(140);p.update(100);p.beginDrag();p.setMode('sleep');
    p.setNotification('ai_writing', 'reply', 10000);p.walkTo(200);
    expect(p.playAction('happy')).toBe(false);p.update(10_000);
    expect(p.visualState()).toBe('dragging');expect(p.targetX).toBeNull();
    p.endDrag();p.update(duration(p.actionConfig.drop));expect(p.visualState()).toBe('sleep');
  });

  it('cancellation clears the held layer without playing a fake landing', () => {
    const p=create();p.beginDrag();p.update(1000);p.cancelDrag();
    expect(p.visualState()).toBe('idle');expect(p.dragHeld).toBe(false);expect(p.endDrag()).toBe(false);
  });

  it('reduced-motion playback still shows requested pickup and landing frames', () => {
    const p=create();p.beginDrag();p.update(200);expect(previewFrame(p,true)).toBeGreaterThan(0);
    p.endDrag();p.update(250);expect(previewFrame(p,true)).toBeGreaterThan(0);
  });

  it('rejects incomplete drag lifecycle packs', () => {
    const m=load('tuxedo');delete m.actions.drop;
    expect(()=>runtime(m)).toThrow(/pickup, dragging and drop/);
    const second=load('tuxedo');second.actions.dragging.loop=false;
    expect(()=>runtime(second)).toThrow(/must loop/);
  });
});
