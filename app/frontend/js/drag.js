// drag.js — 纯坐标计算模块（可独立测试，不依赖 Tauri API）

const DragCalc = {
  /**
   * 计算拖拽后的新窗口物理位置。
   *
   * @param {number} winPhysX - 窗口当前 X（物理像素，来自 outerPosition）
   * @param {number} winPhysY - 窗口当前 Y（物理像素）
   * @param {number} startScreenX - mousedown 时 screenX（逻辑像素）
   * @param {number} startScreenY - mousedown 时 screenY
   * @param {number} currentScreenX - 当前 mousemove 的 screenX
   * @param {number} currentScreenY - 当前 mousemove 的 screenY
   * @param {number} scaleFactor - DPI 缩放因子（如 1.0, 1.25, 1.5, 2.0）
   * @returns {{x: number, y: number}} 新位置（物理像素）
   */
  calcNewPhysicalPosition(winPhysX, winPhysY, startScreenX, startScreenY,
                           currentScreenX, currentScreenY, scaleFactor) {
    const dx = (currentScreenX - startScreenX) * scaleFactor;
    const dy = (currentScreenY - startScreenY) * scaleFactor;
    return {
      x: Math.round(winPhysX + dx),
      y: Math.round(winPhysY + dy),
    };
  },
};

// ========== 测试 ==========

function runDragTests() {
  const results = [];

  function assert(name, condition) {
    results.push({ name, pass: !!condition });
  }

  function eq(a, b) {
    return a.x === b.x && a.y === b.y;
  }

  // --- 100% DPI：直接映射 ---
  // 窗口(100,200)，鼠标从(500,400)移到(600,450)
  // delta_logical=(100,50), scale=1 → delta_physical=(100,50)
  // 新位置=(200,250)
  const r1 = DragCalc.calcNewPhysicalPosition(100, 200, 500, 400, 600, 450, 1.0);
  assert('100%_dpi_basic', eq(r1, { x: 200, y: 250 }));

  // --- 150% DPI：逻辑差值需乘以缩放因子 ---
  // 窗口(300,450)，鼠标从(400,300)移到(500,350)
  // delta_logical=(100,50), scale=1.5 → delta_physical=(150,75)
  // 新位置=(450,525)
  const r2 = DragCalc.calcNewPhysicalPosition(300, 450, 400, 300, 500, 350, 1.5);
  assert('150%_dpi_scaled', eq(r2, { x: 450, y: 525 }));

  // --- 负方向（向左上拖） ---
  // 窗口(500,500)，鼠标从(200,200)移到(100,100)
  // delta_logical=(-100,-100), scale=1 → delta_physical=(-100,-100)
  // 新位置=(400,400)
  const r3 = DragCalc.calcNewPhysicalPosition(500, 500, 200, 200, 100, 100, 1.0);
  assert('negative_direction', eq(r3, { x: 400, y: 400 }));

  // --- 零位移 ---
  const r4 = DragCalc.calcNewPhysicalPosition(123, 456, 100, 100, 100, 100, 1.25);
  assert('zero_delta_no_change', eq(r4, { x: 123, y: 456 }));

  // --- 非整数结果需四舍五入 ---
  // 窗口(100,200)，delta_logical=(1,1), scale=1.25 → delta_physical=(1.25,1.25)
  // Math.round → (101,201)
  const r5 = DragCalc.calcNewPhysicalPosition(100, 200, 0, 0, 1, 1, 1.25);
  assert('non_integer_rounded', eq(r5, { x: 101, y: 201 }));

  // --- 125% DPI（Windows 常见） ---
  // 窗口(960,540)，鼠标从(800,400)移到(900,420)
  // delta_logical=(100,20), scale=1.25 → delta_physical=(125,25)
  // 新位置=(1085,565)
  const r6 = DragCalc.calcNewPhysicalPosition(960, 540, 800, 400, 900, 420, 1.25);
  assert('125%_dpi_windows_common', eq(r6, { x: 1085, y: 565 }));

  // --- 200% DPI（高 DPI 屏幕） ---
  // 窗口(1920,1080)，鼠标从(300,200)移到(400,300)
  // delta_logical=(100,100), scale=2 → delta_physical=(200,200)
  // 新位置=(2120,1280)
  const r7 = DragCalc.calcNewPhysicalPosition(1920, 1080, 300, 200, 400, 300, 2.0);
  assert('200%_dpi_high_dpi', eq(r7, { x: 2120, y: 1280 }));

  return results;
}

if (typeof window !== 'undefined') {
  window.DragCalc = DragCalc;
  window.runDragTests = runDragTests;
}
