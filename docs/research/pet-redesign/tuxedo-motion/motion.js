// Research animation: generated pose art, two-bone legs and shared ground anchors.
// Body/face use the final rise pose so standing and walking share the same artwork.
// No application assets or defaults are changed by this standalone viewer.
(() => {
  'use strict';
  const S=96,FLOOR=82,PERIOD=1280,STRIDE=14,STANCE=.625,N=16;
  const mode=document.querySelector('#mode'),size=document.querySelector('#size');
  const play=document.querySelector('#play'),status=document.querySelector('#status');
  const targets=[...document.querySelectorAll('.cat')];
  const media=matchMedia('(prefers-reduced-motion: reduce)');
  const canvas=(w=S,h=S)=>Object.assign(document.createElement('canvas'),{width:w,height:h});
  const load=src=>new Promise((ok,fail)=>{const im=new Image();im.onload=()=>ok(im);im.onerror=fail;im.src=src;});
  const ease=t=>t*t*(3-2*t);
  let parts,riseArt,blinkArt,bank={},sequence=[],elapsed=0,last=null,selected=0,playing=!media.matches;
  const legSpecs=[
    {x:36,y:62,phase:0,rect:[145,618,174,371],a:[244,657],k:[253,800],b:[235,930],toe:[210,982],bend:-1},
    {x:41,y:62,phase:.5,rect:[473,619,154,371],a:[545,657],k:[559,800],b:[563,930],toe:[536,982],bend:-1},
    {x:67,y:64,phase:.25,rect:[843,618,202,371],a:[920,657],k:[922,800],b:[982,930],toe:[931,982],bend:1},
    {x:72,y:64,phase:.75,rect:[1200,619,196,371],a:[1275,657],k:[1278,800],b:[1328,930],toe:[1278,982],bend:1}
  ];
  function foot(p,offset=0){
    p=((p+offset)%1+1)%1;
    if(p<STANCE)return{x:-STRIDE/2+STRIDE*p/STANCE,y:FLOOR,planted:true};
    const t=(p-STANCE)/(1-STANCE);
    return{x:STRIDE/2-STRIDE*ease(t),y:FLOOR-6*Math.sin(Math.PI*t),planted:false};
  }
  function knee(a,b,l1,l2,bend){
    const dx=b[0]-a[0],dy=b[1]-a[1],raw=Math.hypot(dx,dy);
    const d=Math.max(.01,Math.min(raw,l1+l2-.001));
    const ux=dx/(raw||1),uy=dy/(raw||1);
    const along=(l1*l1-l2*l2+d*d)/(2*d),across=Math.sqrt(Math.max(0,l1*l1-along*along))*bend;
    return[a[0]+ux*along-uy*across,a[1]+uy*along+ux*across];
  }
  function segment(ctx,rect,a,b,j,t){
    const v=[b[0]-a[0],b[1]-a[1]],d=[t[0]-j[0],t[1]-j[1]];
    const scale=Math.hypot(...d)/Math.hypot(...v),angle=Math.atan2(d[1],d[0])-Math.atan2(v[1],v[0]);
    ctx.save();ctx.translate(...j);ctx.rotate(Math.atan2(d[1],d[0])-Math.PI/2);ctx.scale(.052,scale);ctx.rotate(Math.PI/2-Math.atan2(v[1],v[0]));
    const[x,y,w,h]=rect;ctx.drawImage(parts,x,y,w,h,x-a[0],y-a[1],w,h);ctx.restore();
  }
  function drawLeg(ctx,s,p,blend=1){
    const f=foot(p,s.phase),bob=.35*Math.sin(p*Math.PI*4)*blend;
    const j=[s.x,s.y+bob],toe=[s.x+f.x*blend,FLOOR+(f.y-FLOOR)*blend];
    const ankle=[toe[0]+1,toe[1]-3],k=knee(j,ankle,10.5,10,s.bend);
    const[x,,w]=s.rect;
    segment(ctx,[x,618,w,190],s.a,s.k,j,k);
    segment(ctx,[x,792,w,143],s.k,s.b,k,ankle);
    // Paw stays level throughout stance; limb angles no longer tip the foot.
    ctx.drawImage(parts,x,925,w,64,toe[0]+(x-s.toe[0])*.055,toe[1]+(925-s.toe[1])*.055,w*.055,64*.055);
  }
  const poseBounds=[[60,55,380,462],[530,60,435,455],[1000,76,522,440],[12,584,493,400],[510,576,485,408],[1004,564,526,421]];
  const anchors=[[154,503],[623,503],[1101,503],[121,971],[615,971],[1110,971]];
  function transform(ctx,i){ctx.translate(30,FLOOR);ctx.scale(.145,.145);ctx.translate(-anchors[i][0],-anchors[i][1]);}
  function pose(i,art=riseArt){const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;ctx.save();transform(ctx,i);const[x,y,w,h]=poseBounds[i];ctx.drawImage(art,x,y,w,h,x,y,w,h);ctx.restore();return c;}
  function body(ctx,p,blend){
    ctx.save();ctx.translate(0,.35*Math.sin(p*Math.PI*4)*blend);transform(ctx,5);
    // Silhouette mask retains the shared face, torso and tail but excludes legs.
    const outline=[[1004,564],[1530,564],[1530,823],[1434,840],[1415,866],[1371,883],[1325,884],[1272,858],[1215,850],[1190,857],[1170,845],[1140,813],[1080,795],[1004,762]];
    ctx.beginPath();outline.forEach(([x,y],i)=>i?ctx.lineTo(x,y):ctx.moveTo(x,y));ctx.closePath();ctx.clip();
    ctx.drawImage(riseArt,1004,564,526,421,1004,564,526,421);ctx.restore();
  }
  function walk(p,blend=1){const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;drawLeg(ctx,legSpecs[1],p,blend);drawLeg(ctx,legSpecs[3],p,blend);drawLeg(ctx,legSpecs[0],p,blend);drawLeg(ctx,legSpecs[2],p,blend);body(ctx,p,blend);return c;}
  function build(){
    bank.rise=Array.from({length:6},(_,i)=>pose(i));
    const closed=canvas(),ctx=closed.getContext('2d');ctx.drawImage(bank.rise[0],0,0);
    const blink=pose(0,blinkArt);ctx.clearRect(20,35,18,7);ctx.drawImage(blink,20,35,18,7,20,35,18,7);
    bank.idle=[bank.rise[0],closed];bank.walk=Array.from({length:N},(_,i)=>walk(i/N));
    bank.start=Array.from({length:5},(_,i)=>walk(0,ease(i/4)));
    bank.stop=bank.start.slice().reverse();
    bank.sit=bank.rise.slice().reverse();
  }
  function entries(kind,durations){return bank[kind].map((_,frame)=>({kind,frame,duration:Array.isArray(durations)?durations[frame]:durations,root:0}));}
  function setSequence(){
    if(mode.value==='idle')sequence=[{kind:'idle',frame:0,duration:3100,root:0},{kind:'idle',frame:1,duration:130,root:0},{kind:'idle',frame:0,duration:2400,root:0}];
    else if(mode.value==='rise')sequence=entries('rise',[180,140,140,140,160,350]);
    else if(mode.value==='sit')sequence=entries('sit',[180,160,180,180,180,350]);
    else if(mode.value==='cycle'){
      sequence=[{kind:'idle',frame:0,duration:2200,root:0},{kind:'idle',frame:1,duration:130,root:0},{kind:'idle',frame:0,duration:600,root:0},...entries('rise',[160,130,140,160,170,220]),...entries('start',80)];
      for(let k=0;k<N*3;k++)sequence.push({kind:'walk',frame:k%N,duration:PERIOD/N,root:k/N*STRIDE/STANCE});
      const root=3*STRIDE/STANCE;
      sequence.push(...entries('stop',80).map(e=>({...e,root})),...entries('sit',[200,170,160,160,170,250]).map(e=>({...e,root})),{kind:'idle',frame:0,duration:2000,root});
    } else sequence=entries('walk',PERIOD/N).map((e,i)=>({...e,root:i/N*STRIDE/STANCE}));
    elapsed=0;selected=0;thumbnails();render(0);
  }
  function duration(){return sequence.reduce((s,e)=>s+e.duration,0);}
  function locate(time){let t=time%duration();for(let i=0;i<sequence.length;i++){if(t<sequence[i].duration)return i;t-=sequence[i].duration;}return 0;}
  function render(index){
    selected=index;const e=sequence[index],display=Number(size.value),factor=display/S;
    targets.forEach(c=>{const ctx=c.getContext('2d');ctx.clearRect(0,0,S,S);ctx.drawImage(bank[e.kind][e.frame],0,0);c.style.width=display+'px';c.style.height=display+'px';
      const width=c.parentElement.clientWidth;
      let x=(width-display)/2;
      if(mode.value==='cycle')x=(width-display)/2+3*STRIDE/STANCE*factor/2-e.root*factor;
      if(mode.value==='travel'){const ticks=Math.floor(elapsed/(PERIOD/N))/N;x=width-(ticks*STRIDE/STANCE*factor%(width+display));}
      c.style.left=x+'px';c.style.bottom=(30-(S-FLOOR)*factor)+'px';});
    const names={idle:'坐着',rise:'起身',walk:'行走',start:'迈出第一步',stop:'站稳',sit:'坐下'};
    status.textContent=(playing?'播放中':'已暂停')+' · '+names[e.kind]+' · '+(e.frame+1)+' / '+bank[e.kind].length;
    document.body.dataset.phase=e.kind;document.body.dataset.frame=String(e.frame);
  }
  function thumbnails(){const parent=document.querySelector('#frames');parent.replaceChildren();const kinds=mode.value==='cycle'?['idle','rise','walk','sit']:mode.value==='travel'?['walk']:[mode.value];for(const kind of kinds)bank[kind].forEach((f,i)=>{const fig=document.createElement('figure'),c=canvas(),label=document.createElement('figcaption');c.getContext('2d').drawImage(f,0,0);label.textContent=({idle:'待机',rise:'起身',walk:'走路',sit:'坐下'})[kind]+' '+(i+1);fig.append(c,label);parent.append(fig);});}
  function controls(){play.textContent=playing?'暂停':'播放';}
  function tick(now){if(playing&&!document.hidden&&last!==null)elapsed+=Math.min(100,now-last)*Number(document.querySelector('#speed').value);last=now;if(playing)render(locate(elapsed));requestAnimationFrame(tick);}
  function exportAtlas(kind){const fs=bank[kind],c=canvas(S*fs.length,S),ctx=c.getContext('2d');fs.forEach((f,i)=>ctx.drawImage(f,i*S,0));return c;}
  Promise.all([load('walk-parts.png'),load('rise-sheet.png'),load('rise-blink-source.png')]).then(([p,r,b])=>{
    parts=p;riseArt=r;blinkArt=b;build();
    for(const id of ['play','step','download'])document.getElementById(id).disabled=false;
    play.onclick=()=>{playing=!playing;controls();render(selected);};
    document.querySelector('#step').onclick=()=>{playing=false;selected=(selected+1)%sequence.length;elapsed=sequence.slice(0,selected).reduce((s,e)=>s+e.duration,0);controls();render(selected);};
    document.querySelector('#download').onclick=()=>{const kind=sequence[selected].kind,a=document.createElement('a');a.download='tuxedo-'+kind+'-v2.png';a.href=exportAtlas(kind).toDataURL();a.click();};
    mode.onchange=setSequence;size.onchange=()=>render(selected);window.addEventListener('resize',()=>render(selected));
    media.addEventListener('change',()=>{if(media.matches){playing=false;controls();render(selected);}});
    window.motionPreview={exportAtlas,foot,knee,legs:legSpecs,period:PERIOD,stride:STRIDE,stance:STANCE,bank,getSequence:()=>sequence.map(e=>({...e})),duration,seek:time=>{elapsed=time;playing=false;controls();render(locate(time));}};
    controls();setSequence();document.body.dataset.ready='true';requestAnimationFrame(tick);
  }).catch(error=>{status.textContent='动画未能加载。请保留完整文件夹后重新打开。';console.error(error);});
})();
