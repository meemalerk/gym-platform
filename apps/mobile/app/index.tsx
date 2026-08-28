import { Redirect } from 'expo-router';

import { landingRoute, needsPlanChoice } from '@/session/routing';
import { useSession } from '@/session/store';

/**
 * Entry point. Picks the landing screen for a cold start.
 *
 * The decision itself lives in `@/session/routing`, shared with the root
 * layout's mount guards — because a `Redirect` into a group the layout has
 * left unmounted is not a no-op, it is a `REPLACE` no navigator handles, and
 * the router says so out loud: *"Do you have a route named '(app)'?"* That is
 * what a three-way split here and a four-way guard there produced.
 */
export default function Index() {
  const status = useSession((s) => s.status);
  const hasGym = useSession((s) => s.membership !== null);
  const userId = useSession((s) => s.user?.id ?? null);
  const planPendingFor = useSession((s) => s.planPendingFor);

  const signedIn = status === 'signedIn';
  // Mid-registration, for this account only — never a billing state. See
  // `@/session/onboarding` for why the distinction is the whole point.
  const needsPlan = needsPlanChoice({ planPendingFor, userId });

  const href = landingRoute({
    signedIn,
    hasGym,
    needsPlan,
    // The layout holds a loader while the session restores, so by the time this
    // renders the answer is known.
    settling: status === 'restoring',
  });

  return href ? <Redirect href={href} /> : null;
}
