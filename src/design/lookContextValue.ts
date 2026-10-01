import { createContext } from 'react';
import type { Look } from '@/design/look';
import { resolveLook } from '@/design/look';

/** The look of the surface being drawn (see `LookContext.tsx`). */
export const LookContext = createContext<Look>(resolveLook('clean'));
