import { useState } from 'react';
import type { WidgetInstance } from '@/dashboard/model';
import { DashboardPage } from '@/components/Dashboard/DashboardPage';
import { SendToOverlayDialog } from '@/components/Overlay/SendToOverlayDialog';

export function DashboardRoute() {
  const [sending, setSending] = useState<WidgetInstance | null>(null);
  return (
    <>
      <DashboardPage onSendToOverlay={setSending} />
      {sending && <SendToOverlayDialog widget={sending} onClose={() => setSending(null)} />}
    </>
  );
}
