// vitest.setup.js — 测试环境修补，在所有测试文件之前执行。
//
// 问题：Node ≥ 22 在 globalThis 上暴露了实验性的 `localStorage`（未传
// `--localstorage-file` 时，访问它会打印 ExperimentalWarning 并得到 undefined）。
// vitest 的 jsdom 环境里 `window === globalThis`，且不会覆盖已存在的同名属性，
// 于是 jsdom 自己的 Storage 实现装不上：
//   typeof window.sessionStorage === 'object'    // jsdom 提供，正常
//   typeof window.localStorage   === 'undefined' // 被 Node 全局遮蔽
// 这不是 opaque origin 问题（location.href 是 http://localhost:3000/），
// 也不是 Node 26 独有（Node 22 容器里同样复现）。
//
// 处理：只有当 localStorage 确实被运行时的 accessor 遮蔽（或完全缺失）时，
// 才装一个符合 Web Storage 语义的内存实现，让测试和被测代码
// （js/sprite-loader.js 用 `window.localStorage && ...` 做特性检测）
// 在任意 Node 版本下行为一致。sessionStorage 由 jsdom 正常提供，不重复实现。
//
// 注意：探测过程刻意不读取 `window.localStorage` 的值——那会触发 Node 的
// getter 并打印 ExperimentalWarning。改为沿原型链找属性描述符来判断。

/// 沿原型链查找属性的描述符，返回第一个命中的定义。
function findDescriptor(target, key) {
  let current = target;
  while (current) {
    const descriptor = Object.getOwnPropertyDescriptor(current, key);
    if (descriptor) return descriptor;
    current = Object.getPrototypeOf(current);
  }
  return undefined;
}

/// 构造一个满足 getItem/setItem/removeItem/clear/key/length 的内存 Storage。
function createMemoryStorage() {
  const store = new Map();
  return {
    getItem(key) {
      const k = String(key);
      return store.has(k) ? store.get(k) : null;
    },
    setItem(key, value) {
      store.set(String(key), String(value));
    },
    removeItem(key) {
      store.delete(String(key));
    },
    clear() {
      store.clear();
    },
    key(index) {
      const keys = [...store.keys()];
      return index >= 0 && index < keys.length ? keys[index] : null;
    },
    get length() {
      return store.size;
    },
  };
}

if (typeof globalThis.window !== 'undefined') {
  const descriptor = findDescriptor(globalThis, 'localStorage');
  // accessor = Node 运行时的实验性 getter；undefined = 谁都没装。
  // 两种情况都需要补一个真实可用的 Storage；jsdom 的 data 属性则原样保留。
  const needsShim = descriptor === undefined || typeof descriptor.get === 'function';
  if (needsShim) {
    Object.defineProperty(globalThis, 'localStorage', {
      value: createMemoryStorage(),
      configurable: true,
      writable: true,
      enumerable: true,
    });
  }
}
