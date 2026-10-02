import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { PetStateMachine } from '../js/pet.js';
import { buildRuntimeFromManifest } from '../js/sprite-loader.js';

const duration = config => config.frames.reduce((s,f)=>s+f.duration,0)*(config.repeat||1);
function fixture(slug='tuxedo') {
  const m=JSON.parse(readFileSync(path.join(process.cwd(),'__fixtures__/pets/cat-pixel-'+slug+'/manifest.json'),'utf8'));
  const r=buildRuntimeFromManifest(m,{width:m.sprite.columns*96,height:m.sprite.rows*96});
  return {pet:new PetStateMachine({stateConfig:r.stateConfig,actionConfig:r.actionConfig}),manifest:m};
}
describe('authored turning and settling',()=>{
  it.each(['tuxedo','ginger','tabby','calico','cow','black','white','british-blue','siamese','ragdoll','maine-coon'])('%s pauses, turns, then moves in the new direction',slug=>{
    const {pet:p}=fixture(slug),x=p.x;p.walkTo(x-20);
    expect(p.visualState()).toBe('rise');expect(p.facingRight).toBe(true);
    p.update(duration(p.actionConfig.rise));expect(p.visualState()).toBe('turn');
    p.update(duration(p.actionConfig.turn)-1);expect(p.x).toBe(x);expect(p.facingRight).toBe(true);
    p.update(1);expect(p.visualState()).toBe('walk');expect(p.facingRight).toBe(false);expect(p.x).toBe(x);
    p.update(100);expect(p.x).toBeLessThan(x);
  });

  it('uses only the leftover time for movement across rise and turn',()=>{
    const{pet:p}=fixture(),x=p.x;p.walkTo(x-100);
    p.update(duration(p.actionConfig.rise)+duration(p.actionConfig.turn)+100);
    expect(p.x).toBeCloseTo(x-1.75);expect(p.action).toBeNull();
  });

  it('finishes the current turn before reversing a changed target',()=>{
    const{pet:p}=fixture(),x=p.x;p.walkTo(x-20);p.update(duration(p.actionConfig.rise));p.update(150);
    p.walkTo(x+20);expect(p.actionTimeMs).toBe(150);expect(p.facingRight).toBe(true);
    p.update(duration(p.actionConfig.turn)-150);expect(p.facingRight).toBe(false);expect(p.visualState()).toBe('turn');expect(p.x).toBe(x);
    p.update(duration(p.actionConfig.turn));expect(p.facingRight).toBe(true);expect(p.visualState()).toBe('walk');
  });

  it('dragging cancels a turn without a delayed facing flip',()=>{
    const{pet:p}=fixture();p.walkTo(0);p.update(duration(p.actionConfig.rise));p.update(150);p.beginDrag();p.update(10000);
    expect(p.visualState()).toBe('dragging');expect(p.turnFacing).toBeNull();expect(p.targetX).toBeNull();expect(p.facingRight).toBe(true);
  });

  it('keeps legacy immediate-facing behavior without turnAction',()=>{
    const {manifest:m}=fixture();delete m.states.walk.locomotion.turnAction;
    const r=buildRuntimeFromManifest(m,{width:m.sprite.columns*96,height:m.sprite.rows*96});
    const p=new PetStateMachine({stateConfig:r.stateConfig,actionConfig:r.actionConfig});p.walkTo(0);expect(p.facingRight).toBe(false);
  });

  it('validates an explicitly configured turn action',()=>{
    const{manifest:m}=fixture();m.states.walk.locomotion.turnAction='missing';
    expect(()=>buildRuntimeFromManifest(m,{width:m.sprite.columns*96,height:m.sprite.rows*96})).toThrow(/turnAction/);
  });
});
