import type { CSSProperties, ReactNode } from 'react';

export const SUITE_VERSION: string = __KILN_VERSION__;

interface Product {
  id: 'write' | 'grid' | 'deck' | 'notes';
  name: string;
  kind: string;
  office: string;
  formats: string;
  glyph: ReactNode;
}

const stroke = {
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
} as const;

const PRODUCTS: Product[] = [
  {
    id: 'write',
    name: 'Kiln Write',
    kind: 'Documents',
    office: 'Word',
    formats: '.docx · .odt · .md',
    glyph: <path {...stroke} d="M6 18l1-4 8.5-8.5a2.1 2.1 0 0 1 3 3L10 17l-4 1zM14 7l3 3" />,
  },
  {
    id: 'grid',
    name: 'Kiln Grid',
    kind: 'Spreadsheets',
    office: 'Excel',
    formats: '.xlsx · .csv · .ods',
    glyph: (
      <>
        <rect {...stroke} x="6" y="6" width="12" height="12" rx="1.5" />
        <path {...stroke} d="M12 6v12M6 12h12" />
        <rect x="12" y="12" width="6" height="6" rx="1" fill="currentColor" />
      </>
    ),
  },
  {
    id: 'deck',
    name: 'Kiln Deck',
    kind: 'Presentations',
    office: 'PowerPoint',
    formats: '.pptx · .odp',
    glyph: (
      <>
        <rect {...stroke} x="4.5" y="7" width="15" height="10" rx="1.5" />
        <circle cx="15" cy="10.5" r="1.6" fill="currentColor" />
      </>
    ),
  },
  {
    id: 'notes',
    name: 'Kiln Notes',
    kind: 'Notebooks',
    office: 'OneNote',
    formats: 'notebook folders',
    glyph: <path {...stroke} d="M7 5h7l3 3v11H7zM14 5v3h3M10 12h4M10 15h3" />,
  },
];

/** Neutral wordmark placeholder; WP-51 imports the brand kit's mark and WP-58 builds Kiln Home. */
function Wordmark() {
  return (
    <span
      aria-hidden="true"
      className="grid h-7 place-items-center rounded-md bg-fg px-2 text-[12px] font-bold tracking-tight text-canvas"
    >
      Kiln
    </span>
  );
}

function ProductCard({ product }: { product: Product }) {
  const accent = { '--card-accent': `var(--kiln-accent-${product.id})` } as CSSProperties;
  return (
    <li
      style={accent}
      className="group relative flex flex-col gap-4 overflow-hidden rounded-md border border-line bg-surface p-5 shadow-(--kiln-shadow-1)"
      aria-label={`${product.name}, ${product.kind}`}
    >
      <span aria-hidden="true" className="absolute inset-x-0 top-0 h-1 bg-(--card-accent)" />
      <div className="flex items-center gap-3">
        <span className="grid size-10 place-items-center rounded-md bg-(--card-accent) text-white">
          <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true">
            {product.glyph}
          </svg>
        </span>
        <div className="min-w-0">
          <h3 className="m-0 text-[15px] font-semibold leading-tight">{product.name}</h3>
          <p className="m-0 text-[12px] text-muted">{product.kind}</p>
        </div>
      </div>
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12px]">
        <dt className="text-muted">Instead of</dt>
        <dd className="m-0 font-medium">{product.office}</dd>
        <dt className="text-muted">Opens</dt>
        <dd className="m-0 font-medium">{product.formats}</dd>
      </dl>
      <span className="mt-auto inline-flex w-fit items-center gap-1.5 rounded-full border border-line bg-surface-2 px-2 py-0.5 text-[11px] text-muted">
        <span aria-hidden="true" className="size-1.5 rounded-full bg-(--card-accent)" />
        In development
      </span>
    </li>
  );
}

/**
 * Placeholder start screen. The real Kiln Home (PLAN.md §1.6) is built by WP-58 from the brand kit's
 * mockups; until then this page shows the suite name, its version and the four apps.
 */
export function Hub() {
  return (
    <div className="flex min-h-screen flex-col">
      <header className="flex h-12 items-center gap-3 border-b border-line bg-surface px-4 sm:px-6">
        <Wordmark />
        <span className="text-[15px] font-semibold tracking-tight">Kiln Home</span>
        <span
          className="ml-auto rounded-full border border-line bg-surface-2 px-2.5 py-0.5 text-[11px] font-medium text-muted [font-feature-settings:'tnum']"
          data-testid="suite-version"
        >
          Version {SUITE_VERSION}
        </span>
      </header>

      <main className="mx-auto flex w-full max-w-5xl flex-1 flex-col gap-8 px-4 py-10 sm:px-6 sm:py-14">
        <section className="flex flex-col gap-2">
          <h1 className="m-0 text-[28px] font-semibold tracking-tight sm:text-[32px]">Welcome to Kiln</h1>
          <p className="m-0 max-w-2xl text-[15px] leading-relaxed text-muted">
            Kiln is an open-source office suite for documents, spreadsheets, presentations and notes. It is
            being rebuilt on a native Rust core; the apps below are in development.
          </p>
        </section>

        <section aria-labelledby="apps-heading" className="flex flex-col gap-3">
          <h2 id="apps-heading" className="m-0 text-[13px] font-semibold uppercase tracking-wide text-muted">
            Apps
          </h2>
          <ul className="m-0 grid list-none grid-cols-1 gap-4 p-0 sm:grid-cols-2 lg:grid-cols-4">
            {PRODUCTS.map((product) => (
              <ProductCard key={product.id} product={product} />
            ))}
          </ul>
        </section>
      </main>

      <footer className="border-t border-line bg-surface-2 py-3 text-[12px] text-muted">
        <div className="mx-auto flex max-w-5xl flex-wrap px-4 sm:px-6 items-center justify-between gap-2">
          <span>Kiln {SUITE_VERSION} · Free software under the GNU AGPL v3 or later</span>
          <a className="text-hub underline-offset-2 hover:underline" href="https://github.com/ZahakJ/kiln">
            Source code
          </a>
        </div>
      </footer>
    </div>
  );
}
