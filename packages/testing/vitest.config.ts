import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}', 'test/**/*.test.ts'],
    maxWorkers: 2,
    // LibreOffice conversions are slow; oracle tests set their own, longer timeouts.
    testTimeout: 30_000,
  },
});
