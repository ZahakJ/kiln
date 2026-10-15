import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}', 'test/**/*.test.ts'],
    maxWorkers: 2,
    benchmark: {
      include: ['bench/**/*.bench.ts'],
      // Timings include the module runner's export-getter overhead, so they are pessimistic.
      suppressExportGetterWarnings: true,
    },
  },
});
