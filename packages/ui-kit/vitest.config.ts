import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    // Each workspace package has its own hoisted node_modules; force one React instance.
    dedupe: ['react', 'react-dom'],
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}', 'test/**/*.test.{ts,tsx}'],
    setupFiles: ['./test/setup.ts'],
    maxWorkers: 2,
  },
});
