import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['__tests__/**/*.test.js'],
    globals: true,
    // Node ≥ 22 的实验性全局 localStorage 会遮蔽 jsdom 实现，见 vitest.setup.js
    setupFiles: ['./vitest.setup.js'],
  },
});
