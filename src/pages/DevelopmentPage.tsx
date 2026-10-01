import { findMode } from '@/modes/modes';
import { ModePage } from '@/components/Modes/ModePage';

export function DevelopmentPage() {
  return <ModePage mode={findMode('development')!} />;
}
