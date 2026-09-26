// Rebuild baked packs through a local browser. Requires agent-browser and a local
// static server serving the repository root (for example localhost:4189).
// Never calls a generation API; generated source art and bounds are checked in.
import {execFileSync} from 'node:child_process';
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const base=process.argv[2]||'http://127.0.0.1:4189';
const specs=JSON.parse(readFileSync(path.join(root,'docs/research/pet-redesign/production-sources/sources.json'),'utf8'));
const run=(...args)=>execFileSync('agent-browser',['--session','pixel-cat-build',...args],{encoding:'utf8',maxBuffer:20*1024*1024});
try{
 run('open',base+'/docs/research/pet-redesign/production-sources/build.html');
 run('wait','--fn','document.body.dataset.ready === "true"');
 for(const spec of specs){
  const data=JSON.parse(run('eval',`window.buildPixelPack(${JSON.stringify(spec.id)})`));
  const dir=path.join(root,'app/frontend/__fixtures__/pets',data.manifest.id);mkdirSync(dir,{recursive:true});
  writeFileSync(path.join(dir,'spritesheet.png'),Buffer.from(data.png.split(',')[1],'base64'));
  writeFileSync(path.join(dir,'manifest.json'),JSON.stringify(data.manifest,null,2)+'\n');
  process.stdout.write(data.manifest.id+' '+data.manifest.sprite.frameCount+' frames\n');
 }
}finally{run('close');}
