import { startInterfaceLanguage } from '@kiln/ui-kit';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.ts';
import { App } from './App.tsx';
import './index.css';

const container = document.getElementById('root');
if (!container) throw new Error('Kiln: #root element missing from index.html');

// the service worker and the installed app's launches (the GitHub Pages build only).
if (__KILN_PWA__) void import('./pwa/index.ts').then((pwa) => pwa.start());

const root = createRoot(container);
// The chosen interface language loads before the first paint; English is built in.
void startInterfaceLanguage()
  .catch(() => undefined)
  .finally(() =>
    root.render(
      <StrictMode>
        <App />
      </StrictMode>,
    ),
  );
