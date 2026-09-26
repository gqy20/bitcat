// Build v2 sprite packs from authored/generated raster sources using Canvas.
// All frames are baked ahead of time; the production app never runs this rig.
// Source bounds and prompts live in docs/research/pet-redesign/production-sources.
const S=96,GROUND=82;
const canvas=(w=S,h=S)=>Object.assign(document.createElement('canvas'),{width:w,height:h});
const load=src=>new Promise((ok,fail)=>{const im=new Image();im.onload=()=>ok(im);im.onerror=()=>fail(new Error('Image unavailable: '+src));im.src=src;});
function fit(im,b,scale,offsetY=0){const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;const[x,y,w,h]=b;ctx.drawImage(im,x,y,w,h,Math.round((S-w*scale)/2),Math.round(GROUND-h*scale+offsetY),Math.round(w*scale),Math.round(h*scale));return c;}
function variant(source,dx=0,dy=0,angle=0){const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;ctx.translate(48+dx,GROUND+dy);ctx.rotate(angle);ctx.drawImage(source,-48,-GROUND);return c;}
function anchor(image,b,fraction){const[x,y,w,h]=b,c=canvas(w,h),ctx=c.getContext('2d');ctx.drawImage(image,x,y,w,h,0,0,w,h);const data=ctx.getImageData(0,0,w,h).data;const row=Math.min(h-1,Math.max(0,Math.round(h*fraction)));let count=0,sum=0;for(let yy=Math.max(0,row-2);yy<=Math.min(h-1,row+2);yy++)for(let xx=0;xx<w;xx++)if(data[(yy*w+xx)*4+3]>160){sum+=xx;count++;}return[x+(count?sum/count:w/2),y+row];}
function knee(a,b,length,bend){const dx=b[0]-a[0],dy=b[1]-a[1],d=Math.hypot(dx,dy)||.001,across=Math.sqrt(Math.max(0,length*length-d*d/4))*bend;return[(a[0]+b[0])/2-dy/d*across,(a[1]+b[1])/2+dx/d*across];}
function segment(ctx,image,rect,a,b,j,t,widthScale){const v=[b[0]-a[0],b[1]-a[1]],d=[t[0]-j[0],t[1]-j[1]],scale=Math.hypot(...d)/Math.hypot(...v);ctx.save();ctx.translate(...j);ctx.rotate(Math.atan2(d[1],d[0])-Math.PI/2);ctx.scale(widthScale,scale);ctx.rotate(Math.PI/2-Math.atan2(v[1],v[0]));const[x,y,w,h]=rect;ctx.drawImage(image,x,y,w,h,x-a[0],y-a[1],w,h);ctx.restore();}
function rig(image,bounds){
 const body=bounds[0],bh=Math.min(44,body[3]/body[2]*80),by=64-bh;
 const legs=bounds.slice(1).map((b,i)=>({b,a:anchor(image,b,.09),k:anchor(image,b,.49),ankle:anchor(image,b,.81),toe:anchor(image,b,.97),x:[30,35,62,67][i],y:[61,60,61,60][i],phase:[0,.5,.25,.75][i],bend:i<2?-1:1}));
 return (phase,amount=1)=>{
  const c=canvas(),ctx=c.getContext('2d');ctx.imageSmoothingEnabled=false;
  for(const i of [1,3,0,2]){
   const l=legs[i],p=(phase+l.phase)%1;let x,y;
   if(p<.625){x=-7+14*p/.625;y=GROUND;}else{const t=(p-.625)/.375,e=t*t*(3-2*t);x=7-14*e;y=GROUND-6*Math.sin(Math.PI*t);}
   const toe=[l.x+x*amount,GROUND+(y-GROUND)*amount],j=[l.x,l.y],ankle=[toe[0]+1,toe[1]-3],joint=knee(j,ankle,12,l.bend),[sx,sy,w,h]=l.b;
   const thickness=19/h;
   segment(ctx,image,[sx,sy,w,h*.51],l.a,l.k,j,joint,thickness);
   segment(ctx,image,[sx,sy+h*.47,w,h*.36],l.k,l.ankle,joint,ankle,thickness);
   ctx.drawImage(image,sx,sy+h*.79,w,h*.21,toe[0]+(sx-l.toe[0])*thickness,toe[1]+(sy+h*.79-l.toe[1])*thickness,w*thickness,h*.21*thickness);
  }
  ctx.drawImage(image,...body,8,by,80,bh);return c;
 };
}
function timeline(refs,durations,extra={}){return{spriteFrames:[...new Set(refs)],frames:refs.map((sprite,i)=>({sprite,duration:Array.isArray(durations)?durations[i]:durations})),...extra};}
export async function buildPixelPack(spec){
 let bank={};
 if(spec.id==='tuxedo'){
  for(const kind of ['idle','rise','walk','sit']){const im=await load('/docs/research/pet-redesign/tuxedo-motion/'+kind+'-atlas-v2.png');bank[kind]=Array.from({length:im.width/S},(_,i)=>{const c=canvas();c.getContext('2d').drawImage(im,i*S,0,S,S,0,0,S,S);return c;});}
  const im=await load(spec.extra.url),scale=66/spec.extra.bounds[1][3];
  bank.sleep=[fit(im,spec.extra.bounds[0],scale)];bank.happy=[fit(im,spec.extra.bounds[1],scale)];bank.curious=[fit(im,spec.extra.bounds[3],scale)];
 }else{
  const[poses,rise,parts]=await Promise.all([load(spec.poses.url),load(spec.rise.url),load(spec.parts.url)]);
  const pb=spec.poses.bounds,scale=Math.min(68/pb[0][3],86/Math.max(...pb.map(b=>b[2])));
  const idle=fit(poses,pb[0],scale),blinkSource=fit(poses,pb[1],scale),blink=canvas(),ctx=blink.getContext('2d');ctx.drawImage(idle,0,0);
  // Eye-only patch: fixed body outside the central face region.
  const w=pb[0][2]*scale,h=pb[0][3]*scale,x=Math.round((96-w)/2+w*.06),y=Math.round(GROUND-h+h*.25),pw=Math.ceil(w*.55),ph=Math.ceil(h*.15);
  ctx.clearRect(x,y,pw,ph);ctx.drawImage(blinkSource,x,y,pw,ph,x,y,pw,ph);
  bank.idle=[idle,blink];bank.sleep=[fit(poses,pb[4],scale)];bank.happy=[fit(poses,pb[5],scale)];bank.curious=[variant(idle,0,0,-.025)];
  const walking=rig(parts,spec.parts.bounds);bank.walk=Array.from({length:16},(_,i)=>walking(i/16));
  const rs=Math.min(68/spec.rise.bounds[0][3],86/Math.max(...spec.rise.bounds.map(b=>b[2])));
  bank.rise=[idle,...spec.rise.bounds.slice(1,5).map(b=>fit(rise,b,rs)),walking(0,0),walking(0,.5),bank.walk[0]];
  bank.sit=bank.rise.slice().reverse();
 }
 // All breeds have authored pickup, suspended and touchdown art.
 const dragArt=await load(spec.drag.url),db=spec.drag.bounds;
 const heldHeight=Math.max(db[1][3],db[2][3]);
 const dragScale=Math.min(74/heldHeight,84/Math.max(...db.map(b=>b[2])));
 const suspended=i=>{
  const c=canvas(),ctx=c.getContext('2d'),[x,y,w,h]=db[i];ctx.imageSmoothingEnabled=false;
  const head=anchor(dragArt,db[i],.22);
  ctx.drawImage(dragArt,x,y,w,h,Math.round(48-(head[0]-x)*dragScale),Math.round(GROUND-6-heldHeight*dragScale),Math.round(w*dragScale),Math.round(h*dragScale));
  return c;
 };
 const hold=suspended(1);
 bank.pickup=[bank.idle[0],fit(dragArt,db[0],dragScale,-4),hold];
 bank.dragging=[hold,suspended(2)];
 bank.drop=[variant(hold,0,4),fit(dragArt,db[3],dragScale),fit(dragArt,db[4],dragScale),fit(dragArt,db[5],Math.min(68/db[5][3],84/db[5][2])),bank.idle[0]];
 // Reactions stay within this cat's own art; shared poses are explicit in metadata.
 bank.happy.push(variant(bank.happy[0],0,-1),bank.happy[0]);
 bank.sleep.push(variant(bank.sleep[0],0,-1));
 const index={},all=[];for(const[k,fs]of Object.entries(bank)){index[k]=fs.map(f=>{all.push(f);return all.length-1;});}
 const columns=8,rows=Math.ceil(all.length/columns),sheet=canvas(columns*S,rows*S),ctx=sheet.getContext('2d');all.forEach((f,i)=>ctx.drawImage(f,i%columns*S,Math.floor(i/columns)*S));
 const states={
  idle:timeline([index.idle[0],index.idle[1],index.idle[0]],[3100,130,2400],{loop:true}),
  walk:timeline(index.walk,80,{loop:true,locomotion:{enterAction:'rise',exitAction:'sit',speed:17.5}}),
  sleep:timeline(index.sleep,[1600,1200],{loop:true}),
  happy:timeline(index.happy,[240,140,300],{repeat:2,fallback:'idle'}),
  curious:timeline(index.curious,600,{repeat:2,fallback:'idle'}),
  attentive:timeline([index.idle[0],index.idle[1],index.idle[0]],[900,130,1400],{loop:true})
 };
 const actions={rise:timeline(index.rise,140,{repeat:1,fallback:'idle'}),sit:timeline(index.sit,160,{repeat:1,fallback:'idle'}),pickup:timeline(index.pickup,[70,150,100],{repeat:1,fallback:'idle'}),dragging:timeline([index.dragging[0],index.dragging[1],index.dragging[0]],[1100,160,1000],{loop:true,fallback:'idle'}),drop:timeline(index.drop,[80,110,130,150,200],{repeat:1,fallback:'idle'})};
 for(const key of ['observe','nudge','blocked','shake'])actions[key]=timeline(index.curious,300,{repeat:1,fallback:'idle'});
 for(const key of ['acknowledge','wave','jump'])actions[key]=timeline(index.happy,180,{repeat:1,fallback:'idle'});
 actions.spin=timeline(index.idle,180,{repeat:1,fallback:'idle'});
 const manifest={schemaVersion:2,id:'cat-pixel-'+spec.slug,displayName:spec.name+' · 像素',description:'像素猫试用资源，带起身、行走和坐下动作。',render:{mode:'sheet',displayWidth:96,displayHeight:96,scale:1,pixelated:true,facing:'left',stableBody:true},sprite:{image:'spritesheet.png',frameWidth:S,frameHeight:S,columns,rows,frameCount:all.length},hotspots:{observe:{x:.12,y:.15,w:.45,h:.4},input:{x:.15,y:.5,w:.6,h:.35}},states,actions,aliases:{talk:'attentive',focused:'attentive',preparing:'attentive',gameplay:'attentive',gamewin:'happy',gamelose:'curious',confused:'curious',working:'attentive',waiting:'curious',review:'happy',failed:'curious'},metadata:{qualityTier:'preview',assetClass:'pixel-companion',releaseTier:'trial',style:'generated pixel art with baked articulated walk',recommendedUse:'desktop companionship',optimizedFor:'96px desktop',sharedPoses:'talk/focused/preparing/gameplay share attentive; blocked/failed share curious',source:'docs/research/pet-redesign/production-sources/prompts.json'}};
 return{manifest,sheet,bank};
}
