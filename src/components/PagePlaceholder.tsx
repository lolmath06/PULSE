import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

interface PagePlaceholderProps {
  readonly title: string;
  readonly subtitle: string;
  readonly children?: ReactNode;
}

/** Shared frame for pages whose real content arrives in a later PULSE phase. */
export function PagePlaceholder({ title, subtitle, children }: PagePlaceholderProps) {
  const { t } = useTranslation();
  return (
    <section className="page">
      <h1 className="page__title">{title}</h1>
      <p className="page__subtitle">{subtitle}</p>
      <p className="page__note">{t('common.comingLater')}</p>
      {children}
    </section>
  );
}
