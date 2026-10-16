import { StrictMode, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { Hub } from './Hub.tsx';
import { bridgeRoute } from './routes/frame/index.tsx';
import { playgroundRoute } from './routes/playground/index.tsx';
import './index.css';

function subscribeHash(onChange: () => void) {
  window.addEventListener('hashchange', onChange);
  return () => window.removeEventListener('hashchange', onChange);
}

/** Placeholder routing until WP-58's app frame: the bridge routes (WP-52) or the Hub. */
function App() {
  const hash = useSyncExternalStore(subscribeHash, () => window.location.hash);
  return bridgeRoute(hash) ?? playgroundRoute(hash) ?? <Hub />;
}

const container = document.getElementById('root');
if (!container) throw new Error('Kiln: #root element missing from index.html');

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
