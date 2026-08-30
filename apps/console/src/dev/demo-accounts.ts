/**
 * The seeded demo accounts, offered on the console's sign-in screen.
 *
 * The console had no hint at all: `npm run dev` opened a bare form, and the
 * credentials lived only in docs/test-accounts.md, which you have to already
 * know exists. The phone app has offered one-tap sign-in since it was built
 * (apps/mobile/src/dev/demo-accounts.ts); this is the same idea at a desk.
 *
 * Gated on `import.meta.env.DEV`, which Vite substitutes at BUILD time, so a
 * production `vite build` dead-strips the list and the password with it. The
 * mobile equivalent carries a second, deliberate env door because the browser
 * demo is a production export that still needs the buttons. The console is not
 * part of that demo — docker-compose.demo.yml serves the app only — so it
 * needs no such door, and does not get one.
 *
 * Staff only, matching what this screen already says. A member CAN sign in
 * here; they would find Overview and an empty client list, which is a worse
 * first impression than not being offered.
 *
 * Keep in step with docs/test-accounts.md.
 */
export const SHOW_DEMO_ACCOUNTS = import.meta.env.DEV

export const DEMO_PASSWORD = 'demopassword'

export type DemoAccount = {
  email: string
  label: string
  /** The one thing worth checking as this account. */
  hint: string
}

export const DEMO_ACCOUNTS: DemoAccount[] = [
  {
    email: 'owner@demo.test',
    label: 'Owner',
    hint: 'Every tab: roster, billing, catalogue, activity, settings',
  },
  {
    email: 'trainer@demo.test',
    label: 'Trainer',
    hint: 'Their own clients only, and no staff controls',
  },
  {
    email: 'trainer2@demo.test',
    label: 'Trainer 2',
    hint: 'The other trainer — proves the boundary is real',
  },
]
