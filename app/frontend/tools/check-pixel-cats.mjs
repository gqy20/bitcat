// Real Canvas playback regression, including OS reduced-motion preference.
// Run against a static server rooted at app/frontend; requires agent-browser.
// Reports are temporary artifacts under .playwright-cli, outside the product.
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const base=process.argv[2]||'http://127.0.0.1:4191';
const slugs=['tuxedo','ginger','tabby','calico','cow','black','white','british-blue','siamese','ragdoll','maine-coon'];
const run=(...args)=>execFileSync('agent-browser',['--session','pixel-regression',...args],{encoding:'utf8',maxBuffer:2*1024*1024});
const reports=[];
try {
 run('open',base+'/tools/pixel-cats-preview.html');
 run('wait','--fn','Boolean(window.preview && document.body.dataset.ready)');
 for(const reduced of [false,true]) {
  run('set','media',...(reduced?['light','reduced-motion']:['light']));
  for(const slug of slugs) {
   const result=JSON.parse(run('eval',`(async()=>{
    const sleep=ms=>new Promise(r=>setTimeout(r,ms));
    const wait=async(predicate,timeout)=>{const end=performance.now()+timeout;while(!predicate()){if(performance.now()>end)throw Error('Timed out: '+document.body.dataset.state);await sleep(15);}};
    const select=document.querySelector('#cat');select.value=${JSON.stringify(slug)};select.dispatchEvent(new Event('change'));
    await wait(()=>document.body.dataset.ready===${JSON.stringify(slug)},5000);
    const p=window.preview.pet;p.x=60;p.facingRight=true;
    const ctx=document.querySelector('canvas').getContext('2d');
    const hash=(legs=false)=>{const a=ctx.getImageData(0,legs?58:0,96,legs?30:96).data;let h=2166136261;for(const v of a)h=Math.imul(h^v,16777619)>>>0;return h;};
    document.querySelector('[data-command="walk"]').click();
    await wait(()=>p.visualState()==='walk',3500);
    const walk=new Set();let start=performance.now();
    while(performance.now()-start<1400){await sleep(20);if(document.body.dataset.state==='walk')walk.add(hash(true));}
    const x=p.x,previousFacing=p.facingRight;
    document.querySelector('[data-command="turn"]').click();
    await wait(()=>document.body.dataset.state==='turn',1000);
    const turn=new Set();let stayed=true;let phaseDeadline=performance.now()+2000;
    while(p.visualState()==='turn'){if(performance.now()>phaseDeadline)throw Error('Turn did not complete');turn.add(hash());stayed&&=Math.abs(p.x-x)<1e-8;await sleep(15);}
    const changedFacing=p.facingRight!==previousFacing;
    const sit=new Set();await wait(()=>p.visualState()==='sit',3500);phaseDeadline=performance.now()+2500;
    while(p.visualState()==='sit'){if(performance.now()>phaseDeadline)throw Error('Sit did not complete');if(document.body.dataset.state==='sit')sit.add(hash());await sleep(15);}
    return {cat:${JSON.stringify(slug)},reduced:matchMedia('(prefers-reduced-motion: reduce)').matches,walkImages:walk.size,turnImages:turn.size,sitImages:sit.size,stayed,changedFacing,final:p.visualState()};
   })()`));
   reports.push(result);
   assert.equal(result.reduced,reduced);assert.equal(result.walkImages,16);
   assert.ok(result.turnImages>=4);assert.ok(result.sitImages>=4);
   assert.equal(result.stayed,true);assert.equal(result.changedFacing,true);assert.equal(result.final,'idle');
   console.log(`${slug} reduced=${reduced}: walk=${result.walkImages} turn=${result.turnImages} sit=${result.sitImages}`);
  }
 }
} finally {
 mkdirSync('.playwright-cli',{recursive:true});
 writeFileSync('.playwright-cli/pixel-regression.json',JSON.stringify(reports,null,2)+'\n');
 run('close');
}
