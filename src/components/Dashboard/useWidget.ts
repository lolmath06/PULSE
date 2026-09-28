import { useMemo } from 'react';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import type { WidgetInstance } from '@/dashboard/model';
import type { ResolvedBinding } from '@/dashboard/bindings';
import { resolveBindings, useSourceRefs } from '@/dashboard/bindings';

/** A widget's bindings, resolved against the live catalog. */
export function useResolvedBindings(widget: WidgetInstance): readonly ResolvedBinding[] {
  const { catalog } = useMetricCatalog();
  const refs = useSourceRefs();
  return useMemo(
    () => resolveBindings(widget.bindings, catalog, refs),
    [widget.bindings, catalog, refs],
  );
}
