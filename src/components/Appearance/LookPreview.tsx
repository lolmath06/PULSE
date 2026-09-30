import { useMemo } from 'react';
import type { WidgetInstance } from '@/dashboard/model';
import { createWidget, findBlueprint } from '@/dashboard/library';
import type { Look } from '@/design/look';
import { StyleScope } from '@/design/LookContext';
import { useElementSize } from '@/visualization/useElementSize';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';

/** Stable widgets for the preview: the same ids every render, so data flows. */
function previewWidgets(): Record<
  'summary' | 'cpu' | 'memory' | 'temp' | 'gpu' | 'net',
  WidgetInstance
> {
  const make = (blueprint: string, id: string, patch: Partial<WidgetInstance> = {}) => ({
    ...createWidget(findBlueprint(blueprint)!),
    id: `w-preview-${id}`,
    ...patch,
  });
  const memory = make('memory', 'memory');
  return {
    summary: make('summary', 'summary'),
    cpu: make('cpu-total', 'cpu'),
    memory: {
      ...memory,
      visual: {
        ...memory.visual,
        config: { ...memory.visual.config, renderer: 'gauge' },
      },
    },
    temp: make('cpu-temp-value', 'temp'),
    gpu: make('gpu-value', 'gpu'),
    net: make('network-down-value', 'net'),
  };
}

/**
 * A small, real composition — summary strip, a chart, a gauge and three
 * tiles — drawn live with `look`. What you tune is what you see.
 */
export function LookPreview({
  look,
  label = 'Preview',
}: {
  readonly look: Look;
  readonly label?: string;
}) {
  const widgets = useMemo(() => previewWidgets(), []);
  const [ref, size] = useElementSize<HTMLDivElement>();
  const width = Math.max(280, size.width || 520);
  const gap = look.gridGap;
  const chartW = Math.round((width - gap) * 0.6);
  const gaugeW = width - gap - chartW;
  const tileW = Math.floor((width - gap * 2) / 3);

  return (
    <StyleScope look={look} className="look-preview" label={label}>
      <div ref={ref} className="look-preview__stage" style={{ gap }}>
        <WidgetCard widget={widgets.summary} width={width} height={64} editing={false} />
        <div className="look-preview__row" style={{ gap }}>
          <WidgetCard widget={widgets.cpu} width={chartW} height={196} editing={false} />
          <WidgetCard widget={widgets.memory} width={gaugeW} height={196} editing={false} />
        </div>
        <div className="look-preview__row" style={{ gap }}>
          <WidgetCard widget={widgets.temp} width={tileW} height={62} editing={false} />
          <WidgetCard widget={widgets.gpu} width={tileW} height={62} editing={false} />
          <WidgetCard widget={widgets.net} width={tileW} height={62} editing={false} />
        </div>
      </div>
    </StyleScope>
  );
}
