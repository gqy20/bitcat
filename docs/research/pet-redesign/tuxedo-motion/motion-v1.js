// Uses generated raster parts for deterministic, phase-linked puppet animation.
// Stance feet counteract root travel; the idle compositor keeps body pixels fixed.
// Research-only: no application state, pet catalog or default settings are changed.
(() => {
  'use strict';
  const S = 96, FLOOR = 82, PERIOD = 1120, STRIDE = 16, STANCE = 0.625;
  const idleTimeline = [[0,3100],[1,120],[0,2000],[2,220],[0,160],[3,220],[0,180],[2,200],[0,2400],[1,140],[0,1800]];
  const idlePeriod = idleTimeline.reduce((s,f)=>s+f[1],0);
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  const mode = document.getElementById('mode'), size = document.getElementById('size');
  const play = document.getElementById('play'), status = document.getElementById('status');
  const targets = [...document.querySelectorAll('.cat')];
  let playing = !media.matches, elapsed = 0, last = null, selected = 0;
  let idleFrames = [], walkFrames = [], parts;
  const canvas = (w=S,h=S) => Object.assign(document.createElement('canvas'),{width:w,height:h});
  const load = src => new Promise((resolve,reject)=>{const im=new Image();im.onload=()=>resolve(im);im.onerror=reject;im.src=src;});
  const legs = [
    {rect:[145,618,174,371],a:[244,657],b:[210,982],joint:[26,63],phase:0},
    {rect:[473,619,154,371],a:[545,657],b:[536,982],joint:[28,62],phase:0.5},
    {rect:[843,618,202,371],a:[920,657],b:[931,982],joint:[61,63],phase:0.25},
    {rect:[1200,619,196,371],a:[1275,657],b:[1278,982],joint:[63,62],phase:0.75}
  ];
  function foot(phase, offset=0) {
    const p=((phase+offset)%1+1)%1;
    if(p<STANCE)return {x:-STRIDE/2+STRIDE*p/STANCE,y:FLOOR,planted:true};
    const t=(p-STANCE)/(1-STANCE),ease=t*t*(3-2*t);
    return {x:STRIDE/2-STRIDE*ease,y:FLOOR-9*Math.sin(Math.PI*t),planted:false};
  }
  function drawLeg(ctx,spec,phase) {
    const f=foot(phase,spec.phase),j=spec.joint;
    const target=[j[0]+f.x,f.y];
    const vx=spec.b[0]-spec.a[0],vy=spec.b[1]-spec.a[1];
    const tx=target[0]-j[0],ty=target[1]-j[1];
    const scale=Math.hypot(tx,ty)/Math.hypot(vx,vy);
    const angle=Math.atan2(ty,tx)-Math.atan2(vy,vx);
    const [x,y,w,h]=spec.rect;
    ctx.save();ctx.translate(...j);ctx.rotate(angle);ctx.scale(scale,scale);
    ctx.drawImage(parts,x,y,w,h,x-spec.a[0],y-spec.a[1],w,h);ctx.restore();
  }
  function walking(phase) {
    const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;
    drawLeg(ctx,legs[1],phase);drawLeg(ctx,legs[3],phase);
    drawLeg(ctx,legs[0],phase);drawLeg(ctx,legs[2],phase);
    // Body overlays the upper attachment points, hiding the cutout seams.
    ctx.drawImage(parts,160,20,1240,585,8,20,80,48);
    return c;
  }
  function makeIdle(sheet) {
    const raw=Array.from({length:4},(_,i)=>{
      const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;
      ctx.drawImage(sheet,i*450,0,450,612,19,5,58,79);return c;
    });
    // Copy a single base frame, replacing only measured eye/tail regions.
    return raw.map((r,i)=>{
      const c=canvas(),ctx=c.getContext('2d');ctx.drawImage(raw[0],0,0);
      const patch=i===1?[25,26,23,7]:i>1?[59,44,19,33]:null;
      if(patch){const[x,y,w,h]=patch;ctx.clearRect(x,y,w,h);ctx.drawImage(r,x,y,w,h,x,y,w,h);}
      return c;
    });
  }
  function frames(){return mode.value==='idle'?idleFrames:walkFrames;}
  function frameAt(t){
    if(mode.value!=='idle')return Math.floor((t%PERIOD)/PERIOD*8);
    let p=t%idlePeriod;
    for(const[i,d]of idleTimeline){if(p<d)return i;p-=d;}return 0;
  }
  function render(index){
    selected=index;
    const source=frames()[index],display=Number(size.value),moving=mode.value==='travel';
    targets.forEach(c=>{
      const ctx=c.getContext('2d');ctx.clearRect(0,0,S,S);ctx.drawImage(source,0,0);
      c.style.width=display+'px';c.style.height=display+'px';
      const scene=c.parentElement,width=scene.clientWidth;
      // The eight-frame root also steps discretely, so stance contacts do not slide between poses.
      const phaseTicks=Math.floor(elapsed/(PERIOD/8))/8;
      const distance=phaseTicks*STRIDE/STANCE*display/S;
      c.style.left=(moving?width-(distance%(width+display)):(width-display)/2)+'px';
      c.style.bottom=(30-(S-FLOOR)*display/S)+'px';
    });
    status.textContent=(playing?'播放中':'已暂停')+' · '+(mode.value==='idle'?'待机':'走路')+' · '+(index+1)+' / '+frames().length;
    document.body.dataset.frame=String(index);
  }
  function thumbnails(){
    const parent=document.getElementById('frames');parent.replaceChildren();
    frames().forEach((f,i)=>{const figure=document.createElement('figure'),c=canvas(),label=document.createElement('figcaption');c.getContext('2d').drawImage(f,0,0);label.textContent=mode.value==='idle'?['睁眼','闭眼','尾尖向内','尾尖向外'][i]:'第 '+(i+1)+' 帧';figure.append(c,label);parent.append(figure);});
  }
  function controls(){play.textContent=playing?'暂停':'播放';}
  function tick(now){
    if(playing&&!document.hidden&&last!==null)elapsed+=Math.min(100,now-last)*Number(document.getElementById('speed').value);
    last=now;if(playing)render(frameAt(elapsed));requestAnimationFrame(tick);
  }
  function exportAtlas(kind){
    const fs=kind==='idle'?idleFrames:walkFrames,c=canvas(S*fs.length,S),ctx=c.getContext('2d');
    fs.forEach((f,i)=>ctx.drawImage(f,i*S,0));return c;
  }
  Promise.all([load('walk-parts.png'),load('../tuxedo-idle-stabilized.png')]).then(([p,idle])=>{
    parts=p;idleFrames=makeIdle(idle);walkFrames=Array.from({length:8},(_,i)=>walking(i/8));
    for(const id of ['play','step','download'])document.getElementById(id).disabled=false;
    play.onclick=()=>{playing=!playing;controls();render(selected);};
    document.getElementById('step').onclick=()=>{playing=false;selected=(selected+1)%frames().length;elapsed=mode.value==='idle'?idleTimeline.slice(0,idleTimeline.findIndex(v=>v[0]===selected)).reduce((s,f)=>s+f[1],0):selected*PERIOD/8;controls();render(selected);};
    document.getElementById('download').onclick=()=>{const kind=mode.value==='idle'?'idle':'walk',a=document.createElement('a');a.download='tuxedo-'+kind+'-96.png';a.href=exportAtlas(kind).toDataURL();a.click();};
    mode.onchange=()=>{elapsed=0;thumbnails();render(0);};size.onchange=()=>render(selected);
    window.addEventListener('resize',()=>render(selected));
    media.addEventListener('change',()=>{if(media.matches){playing=false;controls();render(selected);}});
    window.motionPreview={exportAtlas,foot,legs,idleTimeline,period:PERIOD,stride:STRIDE,stance:STANCE};
    controls();thumbnails();render(0);document.body.dataset.ready='true';requestAnimationFrame(tick);
  }).catch(error=>{status.textContent='动画未能加载。请保留完整文件夹后重新打开。';console.error(error);});
})();
