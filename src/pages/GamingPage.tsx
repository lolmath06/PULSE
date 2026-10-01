import { findMode } from '@/modes/modes';
import { ModePage } from '@/components/Modes/ModePage';

export function GamingPage() {
  return <ModePage mode={findMode('gaming')!} />;
}
