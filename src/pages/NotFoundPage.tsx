import { Link } from 'react-router-dom';

export function NotFoundPage() {
  return (
    <section className="page">
      <h1 className="page__title">Not found</h1>
      <p className="page__subtitle">This part of PULSE does not exist.</p>
      <Link className="link" to="/">
        Back to Overview
      </Link>
    </section>
  );
}
