// Compare generated assets using measured alpha bounds, without changing PNGs.
// A shared scale per cat preserves the relative height of standing/sleeping poses.
// Keep this research viewer separate from the application's pet catalog.
(() => {
  'use strict';
  const data = window.catAudit;
  const names = ['待机', '眨眼', '迈步 A', '迈步 B', '蜷睡', '开心'];
  const selector = document.getElementById('cat');
  const status = document.getElementById('status');
  let generation = 0;
  for (const cat of data) selector.add(new Option(cat.name, cat.id));
  selector.value = '05-black';
  async function render() {
    const token = ++generation;
    delete document.body.dataset.ready;
    const cat = data.find(c => c.id === selector.value);
    const size = Number(document.getElementById('size').value);
    const maxSide = Math.max(...['original', 'refined'].flatMap(v => cat[v].regions.map(b => Math.max(b.width, b.height))));
    status.textContent = '正在加载…';
    try {
      await Promise.all(['original', 'refined'].map(async version => {
        const path = (version === 'original' ? '../' : '') + cat.id + '.png';
        const sheet = new Image(); sheet.src = path; await sheet.decode();
        if (token !== generation) return;
        document.getElementById(version + '-sheet').src = path;
        document.getElementById(version + '-download').href = path;
        const container = document.getElementById(version + '-poses');
        container.replaceChildren();
        cat[version].regions.forEach((b, i) => {
          const figure = document.createElement('figure');
          const canvas = document.createElement('canvas');
          canvas.width = size; canvas.height = size;
          canvas.style.width = size + 'px'; canvas.style.height = size + 'px';
          canvas.setAttribute('aria-label', cat.name + ' · ' + names[i]);
          const ctx = canvas.getContext('2d'); ctx.imageSmoothingEnabled = false;
          // Extra source padding preserves detached whisker tips near the body.
          const pad = 8, x = Math.max(0, b.x-pad), y = Math.max(0, b.y-pad);
          const w = Math.min(sheet.width-x, b.width+pad*2), h = Math.min(sheet.height-y, b.height+pad*2);
          const scale = (size-8)/(maxSide+pad*2);
          const dw = Math.round(w*scale), dh = Math.round(h*scale);
          ctx.drawImage(sheet, x, y, w, h, Math.round((size-dw)/2), size-4-dh, dw, dh);
          const caption = document.createElement('figcaption'); caption.textContent = names[i];
          figure.append(canvas, caption); container.append(figure);
        });
      }));
      if (token === generation) {
        status.textContent = cat.name + ' · 六个关键姿势 · 初版与精修版均已加载';
        document.body.dataset.ready = cat.id;
      }
    } catch (error) {
      if (token === generation) status.textContent = '图片未能加载。请保留完整设计文件夹，再重新打开预览。';
      console.error(error);
    }
  }
  document.getElementById('background').onchange = event => {
    const palette = {light:['#fafafa','#292932'],dark:['#252730','#f4f4f7'],blue:['#3a536a','#ffffff']}[event.target.value];
    document.documentElement.style.setProperty('--surface', palette[0]);
    document.documentElement.style.setProperty('--ink', palette[1]);
  };
  selector.onchange = render;
  document.getElementById('size').onchange = render;
  render();
})();
