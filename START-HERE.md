# Start here

> **If you were sent a zip, open `START HERE.html` instead** — same instructions,
> formatted, and it opens in your browser with a double-click. This file is the
> plain-text twin of it, for reading on GitHub.

This is a gym and coaching platform — the software a gym uses to run its
members, its coaches, its training programmes and its billing.

There are two ways to see it. **Most people want the first one.**

---

## The easy way: someone sends you a link

If you were sent a web address, that is all you need. Open it in any browser —
Windows, Mac, an iPhone, an Android. Nothing to install, nothing to set up.

**On an iPhone or Android**, open the link, then:

- **iPhone:** tap the Share button, then *Add to Home Screen*
- **Android:** tap the ⋮ menu, then *Add to Home screen*

It then opens full-screen and behaves like a normal app.

> The link only works while the person who sent it has it running, and it
> changes each time they restart it. If it stops working, ask them for a new
> one.

Skip to [**3. Sign in**](#3-sign-in) — the rest of section 1 and 2 is only for
running it yourself.

---

## Running it yourself

Everything below is for putting the whole system on your own computer. You only
need this if nobody has sent you a link.

## 1. Install Docker Desktop

Docker is a free tool that runs software in a self-contained box. It is the only
thing you need to install — it takes care of everything else on its own.

**Download it here:** <https://www.docker.com/products/docker-desktop/>

Pick the version for your computer (Windows, or Mac — and on a Mac, choose
**Apple silicon** if your Mac is from 2021 or later, otherwise **Intel**).

Install it and open it. You may be asked to restart your computer — that is
normal. When it is ready, the Docker whale icon in your menu bar or system tray
stops animating.

> **Give it enough memory.** Open Docker Desktop → Settings → Resources and make
> sure memory is set to at least **4 GB**. The default is sometimes lower, and a
> low setting is the most common reason the demo fails to start.

---

## 2. Start the app

| Your computer | Double-click |
|---------------|--------------|
| **Windows**   | `start-demo.bat` |
| **Mac**       | `start-demo.command` |
| **Linux**     | `start-demo.sh` |

A black window opens and starts printing progress. **Leave it alone.**

> **The first run takes 5–15 minutes.** It is building the whole system from
> scratch. This happens once — every start after that takes a few seconds.

When it is finished, your browser opens the app automatically. If it does not,
open your browser yourself and go to:

```
http://localhost:8210
```

### On a Mac, if it refuses to open

macOS blocks files downloaded from the internet the first time. **Right-click**
`start-demo.command` and choose **Open** — that gives you an *Open* button that
the plain double-click does not.

---

## 3. Sign in

The sign-in screen has a row of buttons, one per demo account — tap one and it
signs you straight in. No typing.

If you would rather type them, the password for every account is
`demopassword`:

| Account | Who they are | Worth looking at |
|---------|--------------|------------------|
| `owner@demo.test` | Runs the gym | Members, invitations, the audit trail, billing, writing programmes |
| `trainer@demo.test` | Coaches a handful of members | Sees **only their own clients**, nobody else's |
| `member@demo.test` | Trains at the gym | **The richest account** — workout history, progress charts, goals, measurements |

---

## 4. Things worth trying

0. **Write a programme.** As `owner@demo.test`, Today → *Programmes* → the
   **+** button. Add a week, add a workout, add exercises to it — notice the
   form changes depending on how the movement is measured. Then submit it for
   review and approve it yourself: this gym has one owner and no separate head
   coach, so self-approval is allowed rather than blocked.

1. **Log a workout.** Sign in as `member@demo.test`, tap *Continue* on the
   Today screen, and log some sets. There is a rest timer and a session clock.
2. **Watch the app change shape.** Sign in as `owner@demo.test`, then as
   `member@demo.test`. The bottom tabs are different — the app rebuilds itself
   around what you are allowed to do, rather than showing greyed-out buttons.
3. **See a trainer's narrow view.** As `trainer@demo.test`, open *People* — only
   `member@` appears, because that is the one relationship they hold.
4. **Look at the money.** As `owner@demo.test`, open *Billing*: membership
   plans, who is subscribed, invoices, and what is overdue.
5. **See who did what.** As `owner@demo.test`, open *Activity*. Every change
   anyone made to the gym is recorded there and cannot be edited afterwards.

---

## 5. Stopping it

| Your computer | Double-click |
|---------------|--------------|
| **Windows**   | `stop-demo.bat` |
| **Mac**       | `stop-demo.command` |
| **Linux**     | `stop-demo.sh` |

Your data stays where it is, so starting it again picks up exactly where you
left off — and takes seconds rather than minutes.

---

## If something goes wrong

**"Docker is not installed" / "Docker is not running"**
Open Docker Desktop and wait for the whale icon to stop animating, then try
again.

**It stopped partway through, or the window closed**
Almost always Docker running out of memory. Docker Desktop → Settings →
Resources → set memory to at least 4 GB → *Apply & Restart*, then try again.

**"Port 8210 is already in use"**
Another program on your computer is using that number. Close it, or open
`docker-compose.demo.yml` in any text editor and change `8210` to `8310` in
both places it appears, then start again — and use `http://localhost:8310`.

**The page loads but says it cannot reach the server**
Give it another minute — the backend may still be starting. Then reload.

**Nothing above helped**
Open the black window again and run this, then send whoever gave you this the
last 30 lines:

```
docker compose -f docker-compose.demo.yml logs
```

---

## What you are looking at

Everything on screen is real. The demo data was created through the app's own
front door — the accounts really did sign up, the invitations really were sent
and accepted, the workouts really were logged. Nothing was pasted into a
database behind the app's back.

A few things are deliberately *not* built yet, so you will not find them:
class booking and timetables, nutrition plans, and card payments — invoices and
payments are recorded, but no real money moves. Nobody is chased for an unpaid
bill either: an overdue member can still train, on purpose, until that decision
is made properly.

This browser version exists so the app can be looked at without a phone. The
product itself is a phone app; a few things (haptic feedback, the secure key
store) simply do nothing in a browser.
