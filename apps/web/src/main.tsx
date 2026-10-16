import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Hub } from './Hub.tsx';
import './index.css';

const container = document.getElementById('root');
if (!container) throw new Error('Kiln: #root element missing from index.html');

createRoot(container).render(
  <StrictMode>
    <Hub />
  </StrictMode>,
);
