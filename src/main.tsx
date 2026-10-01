import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from '@/app/App';
import { initUiConfig } from '@/config/uiConfig';
import { startLanguageSync } from '@/i18n/language';
import '@/config/migrations';
import '@/styles/global.css';
import '@/styles/design.css';

const container = document.getElementById('root');

if (!container) {
  throw new Error('PULSE: root container not found in index.html');
}

// The shared configuration is loaded before the first render, so no window
// ever flashes default layouts or styles before the saved ones arrive — and
// the language it names is resolved and applied synchronously before React
// mounts, so no window flashes English either.
void initUiConfig().finally(() => {
  startLanguageSync();
  ReactDOM.createRoot(container).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
