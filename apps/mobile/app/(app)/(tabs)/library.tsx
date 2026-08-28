import { LibraryScreen } from '@/ui/library-screen';

/**
 * The Library tab. Not shown to owners and admins — they trade it for Billing
 * under the five-tab ceiling (`navigation/tabs.ts`), and reach the same screen
 * through Manage, which pushes `/(app)/library`.
 */
export default function Library() {
  return <LibraryScreen />;
}
