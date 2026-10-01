import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import type { TFunction } from 'i18next';
import { AppLayout } from '@/layouts/AppLayout';
import { OverviewPage } from '@/pages/OverviewPage';
import { NAV_ROUTES } from '@/app/routes';
import { readSection, resetUiConfigForTesting } from '@/config/uiConfig';
import { normalizeSettings } from '@/overlay/settings';
import { Welcome } from '@/components/Welcome/Welcome';
import { LanguageSelect } from '@/components/Language/LanguageSelect';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';
import { DashboardPage } from '@/components/Dashboard/DashboardPage';
import * as processService from '@/services/processes';
import * as metricsService from '@/services/metrics';
import * as historyService from '@/services/history';
import { resetMetricCatalogForTesting } from '@/hooks/useMetricCatalog';
import { reloadVisualizationStoreForTesting } from '@/visualization/store';
import { setLiveBackendForTesting } from '@/live/liveFeed';
import { setSourceRefsForTesting, sourceRefsFrom } from '@/dashboard/bindings';
import { CATALOG, SOURCE_REFS } from '@/test/dashboard';
import { historyResponse } from '@/test/visualization';
import type { ProcessEntry, ProcessSnapshot } from '@/types/processes';
import type { Availability } from '@/types/metrics';
import type { LocaleCode } from '@/i18n/locales';
import { LOCALES } from '@/i18n/locales';
import { activeLocale, i18n, setActiveLocale } from '@/i18n/i18n';
import { startLanguageSync, stopLanguageSyncForTesting } from '@/i18n/language';
import { LOCALE_MESSAGES } from '@/i18n/resources';

/**
 * The interface in languages other than English: rendered for real, read
 * back through the accessibility tree, and checked for any translation key
 * that leaked onto the screen instead of its text.
 */

const SAMPLE: readonly LocaleCode[] = ['fr', 'de', 'ru', 'ja', 'zh-TW', 'hi'];

function flatKeys(node: unknown, prefix = '', out = new Set<string>()) {
  if (node && typeof node === 'object') {
    for (const [key, value] of Object.entries(node)) {
      flatKeys(value, prefix ? `${prefix}.${key}` : key, out);
    }
  } else {
    out.add(prefix);
    out.add(prefix.replace(/_(zero|one|two|few|many|other)$/, ''));
  }
  return out;
}
const KEYS = flatKeys(LOCALE_MESSAGES.en);

/** Every translation key visible as text or in a user-facing attribute. */
function leakedKeys(root: HTMLElement = document.body): string[] {
  const found = new Set<string>();
  const scan = (text: string | null) => {
    for (const match of (text ?? '').matchAll(/[a-zA-Z][\w-]*(?:\.[\w-]+)+/g)) {
      if (KEYS.has(match[0])) found.add(match[0]);
    }
  };
  scan(root.textContent);
  for (const element of root.querySelectorAll('*')) {
    for (const name of ['aria-label', 'title', 'placeholder', 'alt', 'aria-description']) {
      scan(element.getAttribute(name));
    }
  }
  return [...found];
}

const tr = (code: LocaleCode): TFunction => i18n.getFixedT(code);

beforeEach(() => {
  resetUiConfigForTesting();
});

afterEach(() => {
  stopLanguageSyncForTesting();
  vi.restoreAllMocks();
  setLiveBackendForTesting(null);
  setSourceRefsForTesting(null);
});

// --- navigation ------------------------------------------------------------

async function renderShell() {
  const router = createMemoryRouter(
    [{ path: '/', element: <AppLayout />, children: [{ index: true, element: <OverviewPage /> }] }],
    { initialEntries: ['/'] },
  );
  render(<RouterProvider router={router} />);
  await waitFor(() => expect(document.querySelector('nav')).not.toBeNull());
}

describe.each(SAMPLE)('shell in %s', (code) => {
  it('names the navigation and every mode in the language', async () => {
    setActiveLocale(code);
    await renderShell();
    const t = tr(code);
    const nav = await screen.findByRole('navigation', { name: t('nav.main') });
    for (const route of NAV_ROUTES) {
      expect(nav).toHaveTextContent(t(`nav.routes.${route.id}.label`));
    }
    expect(screen.getByText(t('app.tagline'))).toBeInTheDocument();
    await act(async () => {
      await Promise.resolve();
    });
    expect(leakedKeys()).toEqual([]);
  });
});

describe('switching language with the shell open', () => {
  it('re-renders the navigation in place', async () => {
    await renderShell();
    expect(await screen.findByRole('navigation', { name: 'Main' })).toBeInTheDocument();
    act(() => setActiveLocale('ko'));
    expect(screen.getByRole('navigation', { name: tr('ko')('nav.main') })).toBeInTheDocument();
    expect(screen.getByText(tr('ko')('app.tagline'))).toBeInTheDocument();
    expect(leakedKeys()).toEqual([]);
  });
});

// --- Welcome and the selector ------------------------------------------------

describe('Welcome', () => {
  it.each(SAMPLE)('renders in %s without raw keys', async (code) => {
    setActiveLocale(code);
    render(<Welcome onDone={() => undefined} />);
    const t = tr(code);
    expect(screen.getAllByText(t('welcome.title')).length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: new RegExp(t('welcome.start')) })).toBeVisible();
    await act(async () => {
      await Promise.resolve();
    });
    expect(leakedKeys()).toEqual([]);
  });

  it('switches the whole sheet when a language is picked in it', async () => {
    const user = userEvent.setup();
    startLanguageSync({ system: () => ['en-US'] });
    render(<Welcome onDone={() => undefined} />);
    expect(screen.getAllByText('Welcome to PULSE').length).toBeGreaterThan(0);

    await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'es');
    expect(activeLocale()).toBe('es');
    expect(screen.getAllByText(tr('es')('welcome.title')).length).toBeGreaterThan(0);
    expect(normalizeSettings(readSection('settings')).language).toBe('es');
    expect(leakedKeys()).toEqual([]);
  });
});

describe('language selector', () => {
  it('offers System first, then every language in its own name', () => {
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['de-CH']);
    startLanguageSync();
    render(<LanguageSelect />);
    const select = screen.getByRole('combobox', { name: tr('de')('language.label') });
    const options = within(select).getAllByRole('option');
    expect(options.map((option) => (option as HTMLOptionElement).value)).toEqual([
      'system',
      ...LOCALES.map((locale) => locale.code),
    ]);
    // System names the language it resolves to, in that language.
    expect(options[0]).toHaveTextContent('Deutsch');
    expect(options.slice(1).map((option) => option.textContent)).toEqual(
      LOCALES.map((locale) => locale.nativeName),
    );
    expect(options[6]).toHaveAttribute('lang', 'pt-BR');
    expect(select).toHaveValue('system');
  });

  it('keeps native names whatever the interface language', async () => {
    const user = userEvent.setup();
    startLanguageSync({ system: () => ['en-US'] });
    render(<LanguageSelect />);
    await user.selectOptions(screen.getByRole('combobox'), 'zh-CN');
    expect(screen.getByRole('combobox', { name: tr('zh-CN')('language.label') })).toHaveValue(
      'zh-CN',
    );
    expect(screen.getByRole('option', { name: 'Français' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: '日本語' })).toBeInTheDocument();
    expect(document.documentElement.lang).toBe('zh-CN');

    await user.selectOptions(screen.getByRole('combobox'), 'system');
    expect(activeLocale()).toBe('en');
    expect(normalizeSettings(readSection('settings')).language).toBe('system');
  });
});

// --- dashboard flow -----------------------------------------------------------

function mockDashboardServices() {
  resetUiConfigForTesting();
  reloadVisualizationStoreForTesting();
  resetMetricCatalogForTesting();
  setSourceRefsForTesting(sourceRefsFrom(SOURCE_REFS));
  setLiveBackendForTesting({
    setSubscription: () => Promise.resolve({ refused: [] }),
    buffer: () => Promise.resolve([]),
    onTick: () => () => undefined,
  });
  vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(CATALOG);
  vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue([]);
  vi.spyOn(historyService, 'getMetricHistory').mockImplementation((metrics, range) =>
    Promise.resolve(
      historyResponse(
        metrics.map((metric) => ({ metric, points: [] })),
        { range },
      ),
    ),
  );
}

describe('dashboard flow in French', () => {
  it('edits the layout and opens the widget library in French', async () => {
    mockDashboardServices();
    setActiveLocale('fr');
    const t = tr('fr');
    const user = userEvent.setup();
    render(<DashboardPage />);

    await user.click(await screen.findByRole('button', { name: t('dashboard.editLayout') }));
    expect(screen.getByRole('button', { name: t('dashboard.lockLayout') })).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: t('dashboard.addWidget') })[0]!);
    const library = await screen.findByRole('dialog', { name: t('dashboard.addWidget') });
    expect(library).toBeInTheDocument();
    expect(leakedKeys()).toEqual([]);
  });
});

// --- process flow and search --------------------------------------------------

const AVAILABLE: Availability = { status: 'available' };
const field = (value: number) => ({ value, availability: AVAILABLE });

function proc(pid: number, name: string): ProcessEntry {
  return {
    instanceId: `process:${pid}-1`,
    pid,
    parentPid: 1,
    name,
    executablePath: { value: `/usr/bin/${name}`, availability: AVAILABLE },
    state: 'sleepingOrWaiting',
    stateAvailability: AVAILABLE,
    classification: 'userApplication',
    cpuPercent: field(1.5),
    residentMemoryBytes: field(1024 * 1024),
    memoryPercent: field(0.1),
    threadCount: field(1),
    readBytesPerSecond: field(0),
    writeBytesPerSecond: field(0),
    applicationKey: `exe:/usr/bin/${name}`,
  };
}

function processSnapshot(): ProcessSnapshot {
  const processes = [proc(4242, 'firefox'), proc(7, 'sshd')];
  return {
    takenAt: 1_700_000_000_000,
    durationMs: 12,
    counts: { total: 2, running: 1, threads: 2 },
    processes,
    applications: processes.map((p) => ({
      key: p.applicationKey,
      displayName: p.name,
      identity: 'executable',
      classification: 'userApplication',
      processCount: 1,
      threadCount: field(1),
      cpuPercent: field(1.5),
      residentMemoryBytes: field(1024 * 1024),
      readBytesPerSecond: field(0),
      writeBytesPerSecond: field(0),
    })),
    unsupportedReason: null,
  } as ProcessSnapshot;
}

describe('process flow', () => {
  it('views and searches processes in Japanese, keeping names and PIDs as they are', async () => {
    vi.spyOn(processService, 'getProcessSnapshot').mockResolvedValue(processSnapshot());
    setActiveLocale('ja');
    const t = tr('ja');
    const user = userEvent.setup();
    render(<ProcessDetailsCard />);

    await screen.findByLabelText(t('processes.table.applications'));
    const view = screen.getByRole('group', { name: t('processes.table.processView') });
    await user.click(within(view).getByRole('button', { name: t('processes.table.processes') }));
    const table = screen.getByLabelText(t('processes.table.processes'));
    expect(within(table).getByText('firefox')).toBeInTheDocument();
    expect(within(table).getByText('4242')).toBeInTheDocument();
    // Numbers in the Japanese locale, units international.
    expect(within(table).getAllByText(/1\.5\s?%/).length).toBeGreaterThan(0);

    await user.type(screen.getByLabelText(t('processes.table.searchLabel')), '4242');
    expect(within(table).getByText('firefox')).toBeInTheDocument();
    expect(within(table).queryByText('sshd')).not.toBeInTheDocument();
    expect(leakedKeys()).toEqual([]);
  });

  it('keeps the search working after the language changes', async () => {
    vi.spyOn(processService, 'getProcessSnapshot').mockResolvedValue(processSnapshot());
    const user = userEvent.setup();
    render(<ProcessDetailsCard />);
    await screen.findByLabelText('Applications');

    await user.type(screen.getByLabelText('Search processes by name or PID'), 'ssh');
    act(() => setActiveLocale('de'));
    const t = tr('de');
    const search = screen.getByLabelText(t('processes.table.searchLabel'));
    expect(search).toHaveValue('ssh');
    const apps = screen.getByLabelText(t('processes.table.applications'));
    expect(within(apps).getByText('sshd')).toBeInTheDocument();
    expect(within(apps).queryByText('firefox')).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, 'kein-treffer');
    expect(screen.getByText(t('processes.noApplicationMatch'))).toBeInTheDocument();
    // The French separator only in French: the value itself never changed.
    act(() => setActiveLocale('fr'));
    expect(screen.getByText(tr('fr')('processes.noApplicationMatch'))).toBeInTheDocument();
    expect(leakedKeys()).toEqual([]);
  });
});
