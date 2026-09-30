import { RouterProvider } from 'react-router-dom';
import { router } from '@/app/router';
import { OverlayApp } from '@/components/Overlay/OverlayApp';
import { MiniApp } from '@/components/Mini/MiniApp';
import { isValidId } from '@/dashboard/ids';
import { RootLook } from '@/design/LookContext';
import { useAppLook } from '@/design/hooks';

/**
 * One frontend, three kinds of window, chosen by the URL the backend opened:
 *
 * - `index.html?window=overlay&id=<id>` — a desktop overlay;
 * - `index.html?window=mini` — the Mini window;
 * - anything else — the main window with its routes.
 */
export function App() {
  const params = new URLSearchParams(window.location.search);
  const kind = params.get('window');
  const id = params.get('id');
  if (kind === 'overlay' && isValidId(id)) return <OverlayApp id={id} />;
  if (kind === 'mini') return <MiniApp />;
  return <MainWindow />;
}

/** The main window wears the app's style on `:root`. */
function MainWindow() {
  const look = useAppLook();
  return (
    <RootLook look={look}>
      <RouterProvider router={router} />
    </RootLook>
  );
}
