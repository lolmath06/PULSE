import { findMode } from '@/modes/modes';
import { ModePage } from '@/components/Modes/ModePage';

export function PersonalPage() {
  return <ModePage mode={findMode('personal')!} />;
}
