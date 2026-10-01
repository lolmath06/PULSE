import { useLayoutEffect } from 'react';
import type { ReactNode } from 'react';
import type { StyleId } from '@/design/styles';
import type { Look } from '@/design/look';
import { scopeProps, tokensFor } from '@/design/look';
import { LookContext } from '@/design/lookContextValue';
import { useScopedLook } from '@/design/hooks';

/** Provides `look` to everything below without drawing anything. */
export function LookProvider({
  look,
  children,
}: {
  readonly look: Look;
  readonly children: ReactNode;
}) {
  return <LookContext.Provider value={look}>{children}</LookContext.Provider>;
}

/**
 * A container that wears `look`: its tokens as CSS custom properties, so
 * everything inside — cards, charts in theme colours, type — takes the style,
 * whatever the rest of the window wears.
 */
export function StyleScope({
  look,
  className,
  children,
  label,
}: {
  readonly look: Look;
  readonly className?: string;
  readonly children: ReactNode;
  readonly label?: string;
}) {
  const props = scopeProps(look);
  return (
    <LookContext.Provider value={look}>
      <div
        className={`style-scope${className ? ` ${className}` : ''}`}
        aria-label={label}
        {...props}
      >
        {children}
      </div>
    </LookContext.Provider>
  );
}

/**
 * A page that may wear its own style: with `styleId` it gets that style's
 * canvas, filling the content area; without, it simply follows the app.
 */
export function PageStyle({
  styleId,
  children,
}: {
  readonly styleId: StyleId | null | undefined;
  readonly children: ReactNode;
}) {
  const look = useScopedLook(styleId);
  if (!styleId) return <>{children}</>;
  return (
    <StyleScope look={look} className="page-scope">
      {children}
    </StyleScope>
  );
}

/** Applies `look` to the whole window (`:root`), and provides it. */
export function RootLook({
  look,
  children,
}: {
  readonly look: Look;
  readonly children: ReactNode;
}) {
  useLayoutEffect(() => {
    const root = document.documentElement;
    for (const [name, value] of Object.entries(tokensFor(look)))
      root.style.setProperty(name, value);
    root.dataset.pulseStyle = look.style.id;
    root.dataset.density = look.density;
    root.dataset.motion = look.motion;
  }, [look]);
  return <LookContext.Provider value={look}>{children}</LookContext.Provider>;
}
