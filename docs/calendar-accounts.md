# Calendar accounts: Rivet's client IDs

Rivet Calendar signs in to Google and Microsoft with OAuth 2.0 for desktop programs
(RFC 8252): the browser opens the provider's sign-in page, comes back to a port on
`127.0.0.1` (`localhost` for Microsoft), and the code it brings is swapped for tokens with PKCE.
The code is in `crates/rivet-cal/src/oauth.rs`.

Rivet signs in as its own registered app. Its client IDs aren't in the source; a release build
is given them when it's compiled:

```sh
RIVET_GOOGLE_CLIENT_ID=… RIVET_GOOGLE_CLIENT_SECRET=… RIVET_MICROSOFT_CLIENT_ID=… \
  cargo build --release -p rivet-calendar
```

Or put them in `keys.env` at the top of the workspace, as `NAME=value` lines with those
names (`GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`, or `clientID` and `clientSecret`, are read
as Google's too); `crates/rivet-cal/build.rs`
reads it when the environment doesn't set them. **Never commit `keys.env`.** After adding it
for the first time, `touch crates/rivet-cal/build.rs` so the next build reads it.

A build without them still works: the Add Account dialog then asks for the person's own client
ID. Anyone can also choose their own in a build that has Rivet's ("Sign in with my own client
ID"). The steps below are the same for Rivet's registration and for someone's own.

Client IDs of desktop apps aren't secret: anyone can read them out of the program. Google's
desktop "client secret" isn't either (Google says so for installed apps). PKCE is what stops a
caught code being used by anyone else.

## Google

1. In the Google Cloud console, make a project (or pick one).
2. APIs & Services, Library: turn on the **Google Calendar API** and the **CalDAV API**.
   Rivet lists calendars with the first and syncs their events with the second.
3. OAuth consent screen (Google Auth Platform, Branding and Audience): the app's name, support
   address and logo. Audience: External.
4. Data Access: add the scope `https://www.googleapis.com/auth/calendar`.
5. Clients: Create client, type **Desktop app**. Keep the client ID and client secret.
   A desktop client accepts the loopback address on any port, so there's no redirect address
   to enter.
6. While the app is in Testing, only the test users listed can sign in, and their sign-ins run
   out after 7 days. The calendar scope is a sensitive one, so publishing it for everyone means
   Google's verification (a privacy policy, the home page on a verified domain, a video of the
   sign-in). Someone using their own client for themselves can leave it in Testing with
   themselves as the test user, or publish it unverified and click through the warning.

## Microsoft

1. In the Microsoft Entra admin centre (or Azure portal), App registrations, New registration.
2. Supported account types: **Accounts in any organizational directory and personal Microsoft
   accounts** (so Outlook.com and work accounts both sign in). Someone registering for one
   organisation only can pick that instead, but Rivet signs in through `common`, so personal
   accounts need the first.
3. Redirect URI: platform **Public client/native (mobile & desktop)**, address
   `http://localhost`. Entra lets a desktop app come back to it on any port.
4. Authentication: **Allow public client flows**: Yes. There's no client secret: Rivet is a
   public client and uses PKCE.
5. API permissions, Microsoft Graph, Delegated: `Calendars.ReadWrite`, `User.Read`,
   `offline_access`. None needs an administrator's consent for a personal account; a work
   organisation may require one anyway.
6. Keep the Application (client) ID. A publisher domain and verification (Microsoft Partner
   Network) take the "unverified" mark off the consent screen.

## What Rivet keeps

- The account (provider, the address it signed in as, and a client ID of the person's own) in
  `~/.config/rivet-calendar/accounts`.
- The refresh token in the keyring (`service=rivet-calendar account=ID kind=token`), and a
  client secret of the person's own beside it (`kind=client-secret`). Access tokens are kept
  only in memory and asked for again as they run out. Microsoft gives a new refresh token with
  each; the new one replaces the old.
- Removing the account clears both from the keyring.
