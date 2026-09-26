import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { describe, it, expect } from 'vitest';
import { loadPetAssetPack, buildRuntimeFromManifest } from '../js/sprite-loader.js';
import { PetStateMachine } from '../js/pet.js';

const slugs = ['tuxedo','ginger','tabby','calico','cow','black','white','british-blue','siamese','ragdoll','maine-coon'];
const manifestFor = slug => JSON.parse(readFileSync(path.join(process.cwd(),'__fixtures__/pets','cat-pixel-'+slug,'manifest.json'),'utf8'));
const dimensions = m => ({width:m.sprite.columns*m.sprite.frameWidth,height:m.sprite.rows*m.sprite.frameHeight});
function petFor(slug='tuxedo') {
  const m=manifestFor(slug),r=buildRuntimeFromManifest(m,dimensions(m));
  return new PetStateMachine({stateConfig:r.stateConfig,actionConfig:r.actionConfig});
}
const duration = config => config.frames.reduce((sum,f)=>sum+f.duration,0)*(config.repeat||1);

describe('pixel companion packs',()=>{
  it.each(slugs)('loads %s, with all referenced states and native image dimensions',async slug=>{
    const m=manifestFor(slug),file=path.join(process.cwd(),'__fixtures__/pets',m.id,m.sprite.image);
    const png=readFileSync(file),d=dimensions(m);
    expect(png.readUInt32BE(16)).toBe(d.width);expect(png.readUInt32BE(20)).toBe(d.height);
    expect(statSync(file).size).toBeGreaterThan(1000);
    const r=await loadPetAssetPack('/fixtures/'+m.id,{fetch:async()=>({ok:true,json:async()=>m}),imageData:async()=>d});
    expect(r.assetSource.id).toBe(m.id);expect(r.stableBody).toBe(true);
    expect(r.SPRITES.walk).toHaveLength(16);
    for(const action of ['rise','sit','dragging','observe','acknowledge','blocked','jump','spin','wave','shake'])expect(r.actionConfig[action].frames.length).toBeGreaterThan(0);
  });

  it('waits for rise, moves to the exact target, then sits without drifting',()=>{
    const p=petFor(),x=p.x;p.walkTo(x+7);
    expect(p.visualState()).toBe('rise');
    const rise=duration(p.actionConfig.rise);p.update(rise-1);expect(p.x).toBe(x);
    p.update(1);expect(p.state).toBe('walk');expect(p.action).toBeNull();
    p.update(400);expect(p.x).toBe(x+7);expect(p.visualState()).toBe('sit');
    p.update(duration(p.actionConfig.sit));expect(p.visualState()).toBe('idle');expect(p.x).toBe(x+7);
  });

  it.each(['rise','walk','sit'])('sleep interrupts %s and clears its target',phase=>{
    const p=petFor();p.walkTo(p.x+3.5);
    if(phase!=='rise')p.update(duration(p.actionConfig.rise));
    if(phase==='sit')p.update(200);
    p.setMode('sleep');p.update(20_000);
    expect(p.visualState()).toBe('sleep');expect(p.targetX).toBeNull();expect(p.locomotionAction).toBeNull();
  });

  it('retargets mid-rise without restarting and faces the new direction',()=>{
    const p=petFor();p.walkTo(p.x+30);p.update(100);p.walkTo(p.x-10);
    expect(p.actionTimeMs).toBe(100);expect(p.facingRight).toBe(false);
    p.update(duration(p.actionConfig.rise)-100+100);expect(p.x).toBeCloseTo(62.25);
  });

  it('dragging cancels walking, then returns to a semantic state',()=>{
    const p=petFor();p.walkTo(120);p.update(200);p.playAction('dragging');
    expect(p.targetX).toBeNull();expect(p.visualState()).toBe('dragging');
    p.update(duration(p.actionConfig.dragging));expect(p.state).toBe('idle');
  });

  it('rejects invalid locomotion references and speed',()=>{
    const m=manifestFor('tuxedo');m.states.walk.locomotion.enterAction='missing';
    expect(()=>buildRuntimeFromManifest(m,dimensions(m))).toThrow(/missing action/);
    m.states.walk.locomotion.enterAction='rise';m.states.walk.locomotion.speed=0;
    expect(()=>buildRuntimeFromManifest(m,dimensions(m))).toThrow(/speed/);
  });

  it('does not stand or sit again for a zero-distance request',()=>{
    const p=petFor();p.walkTo(p.x);expect(p.state).toBe('idle');expect(p.action).toBeNull();
  });

  it('keeps the default skin and replaces choices without adding more controls',()=>{
    const settings=readFileSync(path.join(process.cwd(),'js/settings.js'),'utf8');
    const presets=settings.slice(settings.indexOf('const PET_ASSET_PRESETS'),settings.indexOf('const PET_ASSET_DEFAULT'));
    expect((presets.match(/value:/g)||[]).length).toBe(16);
    for(const slug of slugs)expect(presets).toContain('/__fixtures__/pets/cat-pixel-'+slug);
    expect(settings).toContain('const PET_ASSET_DEFAULT = "/__fixtures__/pets/cat-tabby"');
  });

  it('mirrors left-authored sheets when facing right',async()=>{
    const m=manifestFor('tuxedo');const r=await loadPetAssetPack('/fixture',{fetch:async()=>({ok:true,json:async()=>m}),imageData:async()=>dimensions(m)});
    const scales=[];const ctx={canvas:{width:96,height:96},clearRect(){},save(){},restore(){},translate(){},scale:(...args)=>scales.push(args),drawImage(){},fillRect(){}};
    r.renderSprite(ctx,'idle',0,true,1);expect(scales).toEqual([[-1,1]]);
    scales.length=0;r.renderSprite(ctx,'idle',0,false,1);expect(scales).toEqual([]);
  });
});
