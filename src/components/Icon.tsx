/**
 * PULSE's icon set: 24×24 stroke icons drawn inline, inheriting `currentColor`
 * and the surrounding font size. Decorative by default (`aria-hidden`).
 */

const PATHS = {
  overview: 'M3 12h4l3-8 4 16 3-8h4',
  dashboard: 'M4 4h7v7H4zM13 4h7v4h-7zM13 10h7v10h-7zM4 13h7v7H4z',
  overlays: 'M12 3 3 8l9 5 9-5-9-5zM3 13l9 5 9-5M3 17.5 12 22l9-4.5',
  gaming:
    'M7 9h-.01M6 8v2M5 9h2M15 9h.01M17 11h.01M8 5h8a5 5 0 0 1 5 5v3a4 4 0 0 1-7.2 2.4L12.5 14h-1l-1.3 1.4A4 4 0 0 1 3 13v-3a5 5 0 0 1 5-5z',
  development: 'M8 8l-4 4 4 4M16 8l4 4-4 4M13.5 5l-3 14',
  personal: 'M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z',
  mini: 'M5 6h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM7 14l3-3 2 2 4-4',
  appearance:
    'M12 3a9 9 0 1 0 0 18c1 0 1.5-.8 1.5-1.6 0-.9-.6-1.3-.6-2.1 0-1 .8-1.8 1.9-1.8H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3zM7.5 11.5h.01M9.5 7.5h.01M14.5 7.5h.01',
  sparkles:
    'M12 3l1.8 4.6L18.5 9l-4.7 1.4L12 15l-1.8-4.6L5.5 9l4.7-1.4zM19 15l.8 2 2 .8-2 .7-.8 2-.8-2-2-.7 2-.8z',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  plus: 'M12 5v14M5 12h14',
  close: 'M6 6l12 12M18 6 6 18',
  lock: 'M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z',
  unlock: 'M7 11V8a5 5 0 0 1 9.6-2M6 11h12v9H6z',
  eye: 'M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  cpu: 'M8 8h8v8H8zM5 5h14v14H5zM9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3',
  thermometer: 'M14 14.8V5a2 2 0 0 0-4 0v9.8a4 4 0 1 0 4 0zM12 9v8',
  network: 'M7 17 3 13l4-4M17 7l4 4-4 4M3 13h11M21 11H10',
  layout: 'M3 4h18v16H3zM3 9h18M9 9v11',
  refresh: 'M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6',
  copy: 'M9 9h11v11H9zM5 15H4V4h11v1',
  arrowRight: 'M5 12h14M13 6l6 6-6 6',
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({
  name,
  size = '1.15em',
  className,
  title,
}: {
  readonly name: IconName;
  readonly size?: number | string;
  readonly className?: string;
  readonly title?: string;
}) {
  return (
    <svg
      className={`icon${className ? ` ${className}` : ''}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden={title ? undefined : true}
      role={title ? 'img' : undefined}
    >
      {title && <title>{title}</title>}
      <path d={PATHS[name]} />
    </svg>
  );
}
