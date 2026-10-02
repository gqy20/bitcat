// Generated keyframes share a fixed body; only eyes and the upper tail animate.
// Source rectangles compensate for the generated sheet's unequal outer margins.
// This isolated preview does not change the application's asset catalog.
(() => {
  'use strict';
  const W = 450, H = 612;
  const origins = [[180, 20], [704, 20], [180, 628], [704, 628]];
  const names = ['睁眼', '眨眼', '尾尖向内', '尾尖向外'];
  const timeline = [[0, 2600], [1, 140], [0, 1800], [2, 240], [0, 180], [3, 260], [0, 180], [2, 200], [0, 1900], [1, 130], [0, 2300]];
  const cycle = timeline.reduce((sum, entry) => sum + entry[1], 0);
  const outputs = ['light', 'dark'].map(id => document.getElementById(id));
  const play = document.getElementById('play');
  const status = document.getElementById('status');
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  let playing = !media.matches, elapsed = 0, last = null, frame = -1;
  let frames = [];
  const makeCanvas = () => Object.assign(document.createElement('canvas'), { width: W, height: H });
  function resize() {
    const h = Number(document.getElementById('size').value);
    outputs.forEach(c => { c.style.height = h + 'px'; c.style.width = (h * W / H) + 'px'; });
  }
  function render(index) {
    frame = index;
    outputs.forEach(c => {
      const ctx = c.getContext('2d');
      ctx.clearRect(0, 0, W, H);
      ctx.drawImage(frames[index], 0, 0);
    });
    status.textContent = (playing ? '播放中' : '已暂停') + ' · ' + names[index];
    document.body.dataset.frame = String(index);
  }
  function controls() { play.textContent = playing ? '暂停' : '播放'; }
  function tick(now) {
    if (playing && !document.hidden && last !== null) elapsed += Math.min(now - last, 100);
    last = now;
    if (playing) {
      let t = elapsed % cycle;
      for (const [index, duration] of timeline) {
        if (t < duration) { if (index !== frame) render(index); break; }
        t -= duration;
      }
    }
    requestAnimationFrame(tick);
  }
  function exportSheet() {
    const sheet = Object.assign(document.createElement('canvas'), { width: W * 4, height: H });
    const ctx = sheet.getContext('2d');
    frames.forEach((f, i) => ctx.drawImage(f, i * W, 0));
    return sheet;
  }
  const image = new Image();
  image.onload = () => {
    const raw = origins.map(([x, y]) => {
      const c = makeCanvas();
      c.getContext('2d').drawImage(image, x, y, W, H, 0, 0, W, H);
      return c;
    });
    frames = raw.map((source, index) => {
      const c = makeCanvas(), ctx = c.getContext('2d');
      ctx.drawImage(raw[0], 0, 0);
      // Rectangles deliberately exclude the chest, paws, ears and tail root.
      const patch = index === 1 ? [48, 170, 165, 47] : index > 1 ? [312, 304, 138, 248] : null;
      if (patch) {
        const [x, y, w, h] = patch;
        ctx.clearRect(x, y, w, h);
        ctx.drawImage(source, x, y, w, h, x, y, w, h);
      }
      return c;
    });
    frames.forEach((c, i) => {
      const figure = document.createElement('figure');
      const caption = document.createElement('figcaption');
      c.style.height = '96px'; c.style.width = (96 * W / H) + 'px';
      caption.textContent = names[i];
      figure.append(c, caption); document.getElementById('frames').append(figure);
    });
    play.disabled = false;
    document.getElementById('step').disabled = false;
    document.getElementById('export').disabled = false;
    play.onclick = () => { playing = !playing; controls(); render(frame); };
    document.getElementById('step').onclick = () => { playing = false; controls(); render((frame + 1) % 4); };
    document.getElementById('export').onclick = () => {
      const link = document.createElement('a');
      link.download = 'tuxedo-idle-stabilized.png'; link.href = exportSheet().toDataURL(); link.click();
    };
    media.addEventListener('change', () => { if (media.matches) { playing = false; controls(); render(0); } });
    window.idlePreview = { exportSheet, timeline, width: W, height: H };
    controls(); resize(); render(0); document.body.dataset.ready = 'true';
    requestAnimationFrame(tick);
  };
  image.onerror = () => { status.textContent = '动画未能加载。请保持图集与预览文件在同一个文件夹，再重新打开。'; };
  document.getElementById('size').onchange = resize;
  image.src = 'tuxedo-idle-sheet-v1.png';
})();
