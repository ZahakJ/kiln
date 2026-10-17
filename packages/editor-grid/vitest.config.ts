import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    // Each workspace package has its own node_modules; force one React instance.
    dedupe: ['react', 'react-dom'],
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    setupFiles: ['./src/test-setup.ts'],
    maxWorkers: 2,
  },
});
