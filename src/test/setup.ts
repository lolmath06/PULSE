import '@testing-library/jest-dom/vitest';
import '@/config/migrations';
import { afterEach } from 'vitest';
import { initI18n, setActiveLocale } from '@/i18n/i18n';

// Every test starts in English, the reference language; a test that switches
// language is put back afterwards.
initI18n('en');
afterEach(() => setActiveLocale('en'));
