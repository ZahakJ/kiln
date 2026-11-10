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
    // Interaction tests drive React through user-event in jsdom: about 6 s at most here, and a CI
    // runner under coverage is several times slower (vitest's default limit is 5 s).
    testTimeout: 30_000,
  },
});
