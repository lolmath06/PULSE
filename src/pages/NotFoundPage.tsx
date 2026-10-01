import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

export function NotFoundPage() {
  const { t } = useTranslation();
  return (
    <section className="page">
      <h1 className="page__title">{t('notFound.title')}</h1>
      <p className="page__subtitle">{t('notFound.subtitle')}</p>
      <Link className="link" to="/">
        {t('notFound.back')}
      </Link>
    </section>
  );
}
