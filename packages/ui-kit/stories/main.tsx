import '@officina/tokens/index.css';
import './gallery.css';
import { applyTheme, isAppId, isThemePreference } from '@officina/tokens';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { GalleryPage, ThemeMatrix } from './Gallery.tsx';

// URL parameters drive screenshots: ?theme=light|dark|hc|system&app=write|grid|deck|notes|hub&view=matrix&icons=colorful
const params = new URLSearchParams(location.search);
const theme = params.get('theme');
const app = params.get('app');
applyTheme({ theme: isThemePreference(theme) ? theme : 'system', app: isAppId(app) ? app : 'write' });
if (params.get('icons') === 'colorful') document.documentElement.dataset.iconStyle = 'colorful';

const container = document.getElementById('root');
if (!container) throw new Error('#root missing');
createRoot(container).render(
  <StrictMode>
    {params.get('view') === 'matrix' ? <ThemeMatrix app={isAppId(app) ? app : 'write'} /> : <GalleryPage />}
  </StrictMode>,
);
