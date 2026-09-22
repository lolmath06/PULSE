import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent } from 'react';

/** One entry of a context menu. */
export interface MenuItem {
  readonly id: string;
  readonly label: string;
  /** Disabled items stay visible and focusable, and show why. */
  readonly disabled?: boolean;
  readonly reason?: string | null;
  readonly onSelect?: () => void;
  readonly submenu?: readonly MenuItem[];
  /** Marks the current choice in a submenu (e.g. the active priority). */
  readonly checked?: boolean;
  readonly tone?: 'danger';
  /** Draws a separator above this item. */
  readonly separated?: boolean;
}

/**
 * A keyboard-accessible context menu.
 *
 * * opens at the pointer (or the row, for Shift+F10 / the Menu key) and is
 *   clamped so it never leaves the window;
 * * ↑/↓ move, → or Enter opens a submenu, ← closes it, Escape closes the
 *   menu and returns focus to where it came from;
 * * a click anywhere outside closes it;
 * * a disabled item is still announced and shows its reason inline, rather
 *   than silently doing nothing.
 */
export function ContextMenu({
  x,
  y,
  label,
  items,
  onClose,
}: {
  readonly x: number;
  readonly y: number;
  readonly label: string;
  readonly items: readonly MenuItem[];
  readonly onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const menu = root.current;
    if (!menu) return;
    const rect = menu.getBoundingClientRect();
    const margin = 8;
    const left = Math.max(margin, Math.min(x, window.innerWidth - rect.width - margin));
    const top = Math.max(margin, Math.min(y, window.innerHeight - rect.height - margin));
    setPosition({ left, top });
  }, [x, y]);

  useEffect(() => {
    const onPointer = (event: MouseEvent) => {
      if (root.current && !root.current.contains(event.target as Node)) onClose();
    };
    const onBlurWindow = () => onClose();
    document.addEventListener('mousedown', onPointer);
    window.addEventListener('blur', onBlurWindow);
    return () => {
      document.removeEventListener('mousedown', onPointer);
      window.removeEventListener('blur', onBlurWindow);
    };
  }, [onClose]);

  return (
    <div
      ref={root}
      className="context-menu"
      style={{ left: position.left, top: position.top }}
      onContextMenu={(event) => event.preventDefault()}
    >
      <MenuList label={label} items={items} onClose={onClose} autoFocus />
    </div>
  );
}

function MenuList({
  label,
  items,
  onClose,
  onBack,
  autoFocus = false,
}: {
  readonly label: string;
  readonly items: readonly MenuItem[];
  readonly onClose: () => void;
  readonly onBack?: () => void;
  readonly autoFocus?: boolean;
}) {
  const list = useRef<HTMLUListElement>(null);
  const [openSubmenu, setOpenSubmenu] = useState<string | null>(null);
  const [flip, setFlip] = useState(false);

  useEffect(() => {
    if (autoFocus) focusAt(list.current, 0);
  }, [autoFocus]);

  useLayoutEffect(() => {
    const element = list.current;
    if (!element) return;
    const rect = element.getBoundingClientRect();
    setFlip(rect.right > window.innerWidth - 8);
  }, [openSubmenu]);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLUListElement>) => {
    const buttons = menuButtons(list.current);
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        event.stopPropagation();
        buttons[(index + 1) % buttons.length]?.focus();
        break;
      case 'ArrowUp':
        event.preventDefault();
        event.stopPropagation();
        buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
        break;
      case 'Home':
        event.preventDefault();
        event.stopPropagation();
        buttons[0]?.focus();
        break;
      case 'End':
        event.preventDefault();
        event.stopPropagation();
        buttons[buttons.length - 1]?.focus();
        break;
      case 'ArrowLeft':
        if (onBack) {
          event.preventDefault();
          event.stopPropagation();
          onBack();
        }
        break;
      case 'Escape':
        event.preventDefault();
        event.stopPropagation();
        if (onBack) onBack();
        else onClose();
        break;
      case 'Tab':
        event.preventDefault();
        onClose();
        break;
      default:
        break;
    }
  };

  return (
    <ul
      ref={list}
      role="menu"
      aria-label={label}
      className={flip ? 'context-menu__list context-menu__list--flip' : 'context-menu__list'}
      onKeyDown={onKeyDown}
    >
      {items.map((item) => {
        const hasSubmenu = item.submenu !== undefined;
        const open = openSubmenu === item.id;
        const classes = ['context-menu__item'];
        if (item.tone === 'danger') classes.push('context-menu__item--danger');
        if (item.disabled) classes.push('context-menu__item--disabled');

        return (
          <li
            key={item.id}
            data-id={item.id}
            role="none"
            className={
              item.separated
                ? 'context-menu__entry context-menu__entry--separated'
                : 'context-menu__entry'
            }
            onMouseEnter={() => setOpenSubmenu(hasSubmenu && !item.disabled ? item.id : null)}
          >
            <button
              type="button"
              role={item.checked !== undefined ? 'menuitemradio' : 'menuitem'}
              aria-checked={item.checked}
              aria-haspopup={hasSubmenu ? 'menu' : undefined}
              aria-expanded={hasSubmenu ? open : undefined}
              aria-disabled={item.disabled || undefined}
              className={classes.join(' ')}
              onClick={() => {
                if (item.disabled) return;
                if (hasSubmenu) {
                  setOpenSubmenu(item.id);
                  return;
                }
                onClose();
                item.onSelect?.();
              }}
              onKeyDown={(event) => {
                if (
                  hasSubmenu &&
                  !item.disabled &&
                  (event.key === 'ArrowRight' || event.key === 'Enter')
                ) {
                  event.preventDefault();
                  event.stopPropagation();
                  setOpenSubmenu(item.id);
                }
              }}
            >
              <span className="context-menu__check" aria-hidden="true">
                {item.checked ? '✓' : ''}
              </span>
              <span className="context-menu__text">
                <span className="context-menu__label">{item.label}</span>
                {item.disabled && item.reason && (
                  <span className="context-menu__reason">{item.reason}</span>
                )}
              </span>
              {hasSubmenu && (
                <span className="context-menu__arrow" aria-hidden="true">
                  ›
                </span>
              )}
            </button>
            {hasSubmenu && open && item.submenu && (
              <div className="context-menu__submenu">
                <MenuList
                  label={item.label}
                  items={item.submenu}
                  onClose={onClose}
                  autoFocus
                  onBack={() => {
                    setOpenSubmenu(null);
                    focusItem(list.current, item.id);
                  }}
                />
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function menuButtons(list: HTMLUListElement | null): HTMLButtonElement[] {
  if (!list) return [];
  return Array.from(list.children).flatMap((entry) => {
    const button = entry.querySelector(':scope > button');
    return button instanceof HTMLButtonElement ? [button] : [];
  });
}

function focusAt(list: HTMLUListElement | null, index: number) {
  menuButtons(list)[index]?.focus();
}

function focusItem(list: HTMLUListElement | null, id: string) {
  const entries = Array.from(list?.children ?? []);
  const buttons = menuButtons(list);
  const position = entries.findIndex((entry) => entry.getAttribute('data-id') === id);
  (buttons[position] ?? buttons[0])?.focus();
}
