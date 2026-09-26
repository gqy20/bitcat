import { describe, it, expect, vi } from 'vitest';
import { WindowWalk } from '../js/window-walk.js';
function fixture() {
  const pet={x:64,state:'idle',locomotionAction:null,currentVisualState:()=> 'idle',setState(s){this.state=s;},clearAction(){this.locomotionAction=null;},walkTo(x){this.target=x;this.state='walk';}};
  const win={outerPosition:vi.fn(async()=>({x:-500,y:120})),outerSize:vi.fn(async()=>({width:192,height:192})),scaleFactor:vi.fn(async()=>2),setPosition:vi.fn(async()=>{})};
  const api={PhysicalPosition:class{constructor(x,y){this.x=x;this.y=y;}},currentMonitor:vi.fn(async()=>({position:{x:-1000,y:0},size:{width:1000,height:800}}))};
  const walker=new WindowWalk({pet,getWindow:()=>win,getApi:()=>api,getScale:()=>1});return{pet,win,api,walker};
}
describe('native window walking',()=>{
  it('uses physical pixels and clamps the window inside a negative-origin monitor',async()=>{
    const{pet,win,walker}=fixture();await walker.walkTo(1000);
    expect(pet.target).toBe(218);pet.x=pet.target;await walker.update();
    expect(win.setPosition).toHaveBeenCalledWith({x:-192,y:120});
  });
  it('does not start an old request after dragging cancels it',async()=>{
    const{pet,win,walker}=fixture();let resolve;
    win.outerPosition.mockImplementation(()=>new Promise(r=>{resolve=r;}));
    const request=walker.walkTo(120);walker.cancel();resolve({x:-500,y:120});await request;
    expect(pet.state).toBe('idle');expect(pet.target).toBeUndefined();expect(walker.active).toBeNull();
  });
  it('serializes position writes and flushes the latest position',async()=>{
    const{pet,win,walker}=fixture();await walker.walkTo(120);let release;
    win.setPosition.mockImplementationOnce(()=>new Promise(r=>{release=r;}));
    pet.x=70;const write=walker.update();pet.x=80;await walker.update();expect(win.setPosition).toHaveBeenCalledTimes(1);
    release();await write;await walker.update();expect(win.setPosition).toHaveBeenLastCalledWith({x:-468,y:120});
  });

  it('allows dragging to wait for the final in-flight write',async()=>{
    const{pet,win,walker}=fixture();await walker.walkTo(120);let release;
    win.setPosition.mockImplementationOnce(()=>new Promise(r=>{release=r;}));
    pet.x=70;const write=walker.update();let stopped=false;
    const cancel=walker.cancel().then(()=>{stopped=true;});
    await Promise.resolve();expect(stopped).toBe(false);expect(walker.active).toBeNull();
    release();await write;await cancel;expect(stopped).toBe(true);
  });
});
