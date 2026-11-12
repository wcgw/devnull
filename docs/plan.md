# devnull: plan and decisions

A Rust/ratatui terminal client for [dev-null.sh](https://dev-null.sh) with a Neovim look & feel.

Reference implementation: [WendellAdriel/dev-null-tui](https://github.com/WendellAdriel/dev-null-tui) (Go, Bubble Tea,
MIT), studied at `76c3413` (v0.10.0). Its `internal/api` and `internal/auth` packages are the de-facto API spec. Its UX is
explicitly **not** a model.

## Decisions

### D1. Neovim look & feel

The original's controls are the main thing this project exists to replace.

- **Modal**: Normal, Insert, Command (`:`). Every action is an ex command (`:reply`, `:star`, `:e notifications`,
  `:Profile @ada`); Normal-mode keys are shortcuts for the common ones.
- **Buffers & windows**: each view (a log, a thread, a profile, the notification inbox, …) is a buffer. `:sp`/`:vs`
  splits, `<C-w>hjkl` window navigation, `:ls`/`:b`, and `<C-o>`/`<C-i>` jumplist instead of a screen stack.
- **Chrome**: no sidebars, no action chips. Bufferline on top; statusline (mode, buffer, unread counts, position) and
  cmdline at the bottom. Toasts become cmdline messages; `:messages` shows history.
- **`q` / `:q` close a window**, as in Vim. Quote gets another binding.
- **Discoverability**: `<leader>` key with a which-key popup; telescope-style fuzzy pickers.
- **Composer**: real Vim editing, via `$EDITOR` (D3).
- **No mouse support** at all.

### D3. Composer is `$EDITOR`

Git-commit style: the TUI suspends and opens `$EDITOR` (falling back to `$VISUAL`, then `nvim`/`vi`) on a temp file.

- A commented header carries the context: target (`# Replying to @paulinevos: …`), author/org, character budget, and a
  scissors line (`# ------------------------ >8 ------------------------`) so `#` in the body is never stripped.
- On `:wq` the TUI resumes and asks in the cmdline, showing the server preview and the count:
  `commit 312/4000 → @paulinevos? [y]es/[e]dit/[n]o`. `:cq` or an empty body aborts.
- No live counter or emoji popup: budget and `:shortcode:` replacement (offline, outside code) are applied on submit.
- Temp file lives in `$XDG_RUNTIME_DIR` (tmpfs), mode 0600, deleted after submit or abort. Keep-draft holds the text in
  memory and reopens it next time.

### D2. Scope

No commitment to feature parity with the original. The buffer model must leave room for Remotes later.

| Tier | Features |
| --- | --- |
| **v1** | Device-approval login and saved connection; logs (Following, Latest, Trending) with paging and refresh; threads; profiles incl. followers/following; notifications with category filters and groups; Starred; composer for commit/reply/quote with emoji shortcodes; star, follow, delete own commit, copy link, open in browser; background polling for new commits and unread counts; inline images, avatars and link-card pictures, animated (D7) |
| **v2** | Remotes (DMs and orgs, text only: inbox, conversation, reactions, replies, drafts); polls (vote and create); search (commits, profiles); attaching images; settings and safety |
| **Maybe never** | Voice notes; go live; pastéis; scheduled commits; org administration; custom theme editor (use colorschemes); self-update; "What's new"; desktop notifications |

### D4. Text cursor in read-only buffers

Buffers behave like `nomodifiable` Neovim buffers: rendered lines, a (row, col) cursor, real Vim motions (`hjkl`, `w`,
`b`, `f`, `0`, `$`, `gg`, `G`, …), `/`/`?` search with `n`/`N`, and `v`/`V` + `y` to yank text.

- Commit-wise motion with `]]`/`[[`. Commit actions apply to **the commit containing the cursor**.
- **Screen-row semantics** (agreed at M1.2): `j`/`k`, `0`/`^`/`$`, counts and `G` act on screen rows, like a pager or
  Vim's `gj`. Logical lines (a paragraph before wrapping) stay internal: anchors and search use them, so search
  matches across wrap points. May become configurable later.
- Under-cursor semantics: `@mention` opens the profile, a commit hash or quote opens its thread, a URL opens in the
  browser (`gx`). `gd` opens the author of the commit under the cursor.
- Core abstraction, built first: a **document model**, meaning rendered lines carrying span metadata (commit, link,
  mention, hash ranges), with the cursor anchored to its commit across new arrivals above it and across re-wraps on
  resize.

### D5. Keymap

Principles:

1. **Motions stay pure Vim** (`hjkl w b e f t / ? n N * gg G { } ( ) H M L zz zt zb` …). Never repurposed.
2. **Edit keys become actions** (fugitive/oil style): `c r R s d x p o i a u` are meaningless in read-only buffers.
3. **Everything else lives under `<leader>`** (default `<Space>`), surfaced by which-key.
4. **Every action has an ex command**, so `:map` can rebind it.

Defaults:

| Key | Action | Ex |
| --- | --- | --- |
| `<CR>`, `<C-]>` | Open under cursor: mention → profile, hash/quote → thread, link → browser, else the commit's thread | `:open` |
| `<C-o>`, `<C-i>` (`<Tab>`) | Jumplist back/forward, across buffers | |
| `<C-t>` | Back to where `<CR>`/`gd` left for another buffer (tag stack) | `:pop` |
| `gd` | Profile of the commit's author | `:profile` |
| `K` | Hover card (author, link card, poll) | `:hover` |
| `gx` / `gX` | Open URL under cursor / the commit on the web | `:browse` |
| `]]`, `[[` | Next/previous commit | |
| `zo zc za zR zM` | Folds: long commits, reply groups, notification groups | |
| `c` | New commit | `:commit` |
| `r` / `R` | Reply / quote | `:reply`, `:quote` |
| `s` | Toggle star | `:star` |
| `dd` | Delete own commit (confirms) | `:delete` |
| `yy`, `v…y` | Yank text (Vim semantics) | |
| `gy` | Yank commit URL to the clipboard (OSC 52) | `:yanklink` |
| `:e!`, `<C-l>` | Refresh buffer | `:e!` |
| `q`, `:q` | Close window; the last one quits | |
| — | Follow/unfollow (no plain key; `f` stays a motion) | `:follow`, `:unfollow` |
| `<leader>1/2/3` | Following, Latest, Trending | `:e log/following` … |
| `<leader>n` `s` `p` | Notifications, Starred, own profile | `:e notifications` … |
| `<leader><leader>` | Buffer picker | `:pick buffers` |
| `<leader>/` | User picker | `:pick users` |

Older pages load automatically as the cursor nears the end of a buffer; there is no paging key.

### D6. Configuration, colors, icons

- **Config is an rc file of ex commands** (`~/.config/devnull/init.vim`): `set`, `map`/`nnoremap`, `hi`,
  `colorscheme`. Same parser as `:`; `:source` re-reads it. No Lua, no TOML.
- **Default colorscheme: tokyonight** (night). Palettes from [folke/tokyonight.nvim](https://github.com/folke/tokyonight.nvim)
  (Apache-2.0, attribute it). Theming is done through named highlight groups (`StatusLine`, `CursorLine`,
  `DevnullHandle`, `DevnullHash`, …) that `:hi` can override.
- **A Nerd Font is assumed.** Icons everywhere; no plain-glyph fallback mode.
- **Images, including animated GIFs, are supported** (see D7 for how).

### D7. Graphics

- **Target terminal: Ghostty ≥ 1.4** (1.4.0 claims full Kitty graphics spec coverage, including animation).
- **Kitty graphics protocol with Unicode placeholders** (virtual placements), as the original does: images are
  uploaded once and placed as text cells, so they scroll, clip and fold with the D4 document model.
- **Animation (GIF, animated WebP) is native**: upload frames once (`a=f`), let the terminal animate. No frame timer on
  our side for Kitty-protocol terminals.
- **Fallback: half-blocks** (`▀` with fg/bg), still text, so the buffer model holds. Animation there uses our own
  frame timer.
- **No sixel or iTerm2 protocol**: grid-painted images fight the scrolling buffer model.
- **Scope**: viewing inline images, avatars and link-card pictures moves to **v1**; attaching images stays in v2.
- **Verified in Ghostty 1.4.0 (M0.1)**, by a probe that draws images purely through `U+10EEEE` cells (24-bit fg
  colour = image id, row and column diacritics) in a redrawn alternate-screen buffer:
  - Placeholders render; images partially scrolled past an edge are **cropped** to their surviving rows, not squashed.
  - Native animation (`a=f` frames, `a=a,s=3,v=1`) keeps running inside placeholders, also while scrolling.
  - The same image can be placed twice at once; two images can share a row; no ghosts after scrolling off and back.
  - Cell size reported by `CSI 16 t`: 10×20 px.
- **Detection**: Ghostty 1.4.0 does **not** implement feature queries (`e=` key); it answers them as plain queries
  (`EINVAL: dimensions required`). Detect with a plain `a=q` query plus `TERM`/`TERM_PROGRAM`
  (`xterm-ghostty`, `xterm-kitty`); override with `:set graphics=kitty|halfblocks`.

### D8. Identity

- **Binary: `dn`.** Crate and repo stay `devnull`.
- **Replaces the Go client**; no side-by-side coexistence required. Own config dir `~/.config/devnull/`, own device
  enrollment (no import of the Go client's `connection.json`). Owning the `devnull://` handler is fine.
- **No `devnull://` handler at all** (verified in M0.3): the server accepts a loopback `redirect_uri`
  (`http://127.0.0.1:<port>/callback`, RFC 8252 §7.3), as the current Go client uses. Over SSH, where the browser
  cannot reach the remote loopback, the fallback is pasting the `127.0.0.1` callback URL from the browser's address bar
  (`:connect <url>`).

**Enrollment protocol** (Go `auth/attempt.go`, `callback.go`, `flow.go`; verified against dev-null.sh in M0.3):

1. Generate the device key; fingerprint is OpenSSH `SHA256:…`.
2. PKCE: verifier = base64url-nopad(32 random bytes); challenge = base64url-nopad(SHA-256(verifier)), `S256`;
   state = 16 random bytes, lowercase hex. Attempts expire after 10 minutes (client-side).
3. Browser opens `{origin}/mobile/authorize?fingerprint=…&device_name=dn&code_challenge=…&code_challenge_method=S256&state=…&redirect_uri=http://127.0.0.1:<port>/callback`.
   The approval page shows the fingerprint; `dn` displays it too, for comparison.
4. The loopback listener accepts only `GET /callback` with the right `Host` and `state`, answers plain text
   (`Cache-Control: no-store`, `Referrer-Policy: no-referrer`), and yields `code` (≤255 chars) or `error`
   (`access_denied` = user canceled).
5. `POST /api/v1/devices` `{"code","code_verifier"}`, signed with the new key → `200 {data: User,
   unread_notifications_count}`.
6. `GET /api/v1/me` → same shape. If `terms_accepted` is false, show the terms; `PUT /api/v1/me/terms {"version"}` only
   on an explicit `:agree`.

Observed `User` fields not in the Go struct: `html_url`, `feed_url`, `created_at`, `pasteis_resets_at`,
`pasteis_received`, `terms_accepted_at`. `organizations` and `can_post_as_organization` were absent: model them as
optional.

### D9. Device key and request signing

Every API request carries an SSHSIG signature (Ed25519, sha512, namespace `devnull-api`; image uploads use
`devnull-api-images`) over timestamp, nonce, method, URI and signed body lines, sent as `X-Devnull-Timestamp`,
`X-Devnull-Nonce`, `X-SSH-Signature`.

- Enrollment generates `~/.config/devnull/device_ed25519` (OpenSSH format, 0600). One key per device; never an existing
  `~/.ssh` key.
- **Sign via `ssh-agent`** when `$SSH_AUTH_SOCK` holds the key (user's setup: plain `ssh-agent`). The user can
  `ssh-keygen -p` and `ssh-add` it.
- Otherwise read the file; prompt once in the cmdline for a passphrase if it has one.
- Agents that confirm every signature are unsupported in practice (polling signs every few seconds).

**Canonical message** (from the Go `auth/client.go`): lines joined with `\n`, no trailing newline added:

1. timestamp (Unix seconds), 2. nonce (16 random bytes, lowercase hex), 3. method, 4. request URI (path and query
   exactly as sent), then the signed lines:
   - JSON requests (`devnull-api`): the body (empty string when there is none, so a GET ends in `\n`).
   - Multipart uploads (`devnull-api-images`): comma-joined lowercase hex SHA-256 of each file, then the payload JSON.

`X-SSH-Signature` is the **standard base64 of the raw SSHSIG blob** (magic preamble included), not the armored PEM.

**Verified in M0.2** (scratch crate, 16/16 checks):

- `ssh-key` 0.6.7 signing from the key file and `ssh-agent-client-rs` 1.1.4 signing through an agent produce blobs
  **byte-identical** to the original's unchanged `key.go` (`x/crypto` v0.58.0) and to `ssh-keygen -Y sign`
  (Ed25519 is deterministic), for GET and POST messages.
- `ssh-keygen -Y verify` accepts them; rejects a wrong namespace and a tampered message. The upload namespace verifies.
- Passphrase-encrypted key files (`ssh-keygen -p`) decrypt and sign identically; a wrong passphrase fails.
- Generated key file is 0600. An agent without the key fails loudly (no silent fallback).
- Crates: `ssh-key` 0.6.x (0.7 is still a release candidate), `ssh-encoding` 0.2 for the blob, `ssh-agent-client-rs`
  1.1 (blocking; call it from `spawn_blocking`).

**Test rule**: agent tests start their own `ssh-agent -D` and refuse to run unless `SSH_AUTH_SOCK` points at it and it
is empty; never `ssh-add -D`. Absolute scratch or `target/` paths can exceed the 108-byte Unix socket path limit: use
a relative socket path or a short temp dir.

### D10. Architecture

- **Workspace, two crates**:
  - `devnull-api`: typed models, endpoints, SSHSIG signing, enrollment. No UI deps. Spec: the Go `internal/api` and
    `internal/auth` packages and their tests.
  - `devnull`: the `dn` binary and all UI.
- **Async**: `tokio`, `reqwest` + `rustls`.
- **Elm-style core**: pure `update(&mut State, Msg) -> Vec<Effect>`; a small runtime executes effects (HTTP, timers,
  `$EDITOR`, clipboard, graphics uploads) and feeds results back as `Msg`. Main loop `select!`s over crossterm
  `EventStream`, the effect-result channel and timers.
- **Rendering** is a pure function of `State` into ratatui. Kitty uploads are the one side channel: an effect transmits
  image data, render only emits placeholder cells.
- **UI modules**: `doc` (D4 document model), `buffer` (log, thread, profile, notifications, starred), `window` (split
  tree, jumplist), `keymap` (key trie, counts, pending keys, which-key data), `ex` (parser, command table), `rc`, `hl`
  (highlight groups, colorschemes), `gfx` (Kitty protocol, half-block fallback), `compose` (`$EDITOR` round trip).
- **Testing**: unit tests on `update`, `doc`, `keymap`, `ex`; `insta` snapshots of full screens via ratatui's
  `TestBackend`; `wiremock` API tests with fixtures ported from the Go testdata; signatures checked with
  `ssh-keygen -Y verify`.
- **Toolchain**: edition 2024, stable Rust, `clippy -D warnings` and `rustfmt` in CI.

### D11. Platforms

- **Linux**: first-class, tested in CI.
- **macOS**: best effort; must build in CI. Platform-specific code is limited to opening the browser (no URL handler,
  see D8).
- **Windows**: not supported.

### D12. Process

- **Test-first** (red → green → refactor). Each step reports what ran (`fmt`, `clippy`, tests) and what didn't.
- **Review checkpoints** inside milestones (marked ⏸ below), not one diff per milestone.
- **Commits**: Claude commits between stages only when authorized in that session; otherwise the user commits.
  Conventional-commit subjects (`feat:`, `fix:`, `docs:`, `chore:`, `test:`), no AI attribution. Never push; no
  GitHub writes.
- **Tracking**: the checklists below. No GitHub issues unless the user creates them.
- **Spikes** live in the scratchpad, never the repo. Probes that need eyes on Ghostty or a browser are run by the user.
- Default branch: `main`.

### Defaults (decided without discussion; override any)

- **Polling** cadences copied from the original: logs and unread counts every 15 s while focused, 30 s unfocused
  (crossterm focus events).
- **Startup**: one window on `log/following`. CLI mirrors `nvim`: `dn [+{cmd}] [@handle | <hash>]` opens that buffer.
- **Clipboard**: OSC 52 (works in Ghostty and over SSH). Unnamed register `"` for in-app yanks.
- **Browser**: `xdg-open` / `open`.
- **Terms consent**: shown as a buffer; accept with `:agree`. Never automatic.
- **Fixtures**: committed fixtures are synthetic or ported from the Go testdata. Real captured responses stay in the
  scratchpad as schema reference (they contain other people's posts).

## API notes (observed in M0.4)

Laravel backend. Captured responses live in the scratchpad only (they contain other people's posts).

- **IDs** are UUIDv7 strings; `short_hash` is the last 7 hex digits of the id.
- **Cursor pagination** (Laravel): `links.next` is an absolute URL on the API origin, `meta.next_cursor` the opaque
  cursor, `per_page` 20. Validate `links.next` stays on `/api/v1/log/<same log>` before following it (as the Go client
  does).
- **Mentions** `offset`/`length` count Unicode characters (code points) of `body`, `@` included: all 13 captured
  mentions match that way; byte and UTF-16 offsets fail on the 4 after non-ASCII text.
- **Commit**: `body`, `body_segments` (`text {text}`, inline `code {code}`, code `block {language, filename, html}`
  where `html` is server-highlighted markup, so `dn` either parses it or re-highlights from source), `preview_segments`
  (excerpt for long commits, else null), `mentions` (`handle`, `kind` user|organization, `offset`, `length`), `images` (`url`,
  `width`, `height`, `description`), `link_preview` (`is_pending` while the server fetches it), `quoted_commit` (one
  level, embedded), `reply_preview` (`data`, `next_cursor`), `poll`, counts, `starred`, `status` (`approved`; others
  under moderation), `deleted_at`.
- **Author**: `type` user|organization; `lane` is an int, a `#rrggbb` string, or null.
- **Thread** `GET /commits/{id}`: `{data, ancestors, reply_participants}`; replies are a separate paged
  `GET /commits/{id}/replies`.
- **Profile** `GET /users/{handle}`: `{data, achievements, activity, bio_profiles, follows_you, is_following,
  is_muting, pasteis_received, pinned_commit, streaks}`; lists at `/users/{handle}/{commits[?replies=1],followers,
  following}`.
- **Organization** `GET /organizations/{handle}`: `{data{…, admins}, can_claim, is_following, settings_url}`; tabs at
  `/organizations/{handle}/{commits,mentions}`. `GET /users/{organization}` is a 404 (checked live in M3.3), so a
  handle of unknown kind is tried as a person, then as an organization.
- **People lists** (followers, following) carry no account `type`.
- **Inbox** `GET /notifications?type=all|replies,quotes,stars,mentions,follows,moderation`: `{data, read_at, meta}`,
  newest first, one entry per group (`group_id`, `event_count`, `actors_count`, `actors`). **Reading it marks every
  notification read**, whatever the filter (the Go client's docs); entries read at or after the page's `read_at` are
  new to this visit. Types: `followed`, `starred`, `replied`, `quoted`, `mentioned`, `approved`, `rejected`; `commit`
  is the reply/quote/mention, or the account's own for stars and moderation. The unread count is
  `unread_notifications_count` in `/me`'s envelope, and reading it marks nothing. From the Go client; not checked live.
- **Commit writing**: `POST /commits/preview {body}` answers `{data: {body, body_segments, …}}` with the text as the
  server would keep it (shortcodes replaced outside code) and changes nothing; it refuses what publishing would, with
  a 422 and Laravel's message ("The body field is required.", "The body field must not be greater than 4000
  characters."). `POST /commits {body, parent_id | quoted_commit_id}` publishes; `DELETE /commits/{id}`;
  `POST`/`DELETE /commits/{id}/star` answer the commit; `POST`/`DELETE /users|organizations/{handle}/follow`. The
  budget rules (links count 23, code whole, blank runs collapsed) are ported from the Go client with its vectors.
- **Profile search** `GET /search/profiles?q=`: `{users, organizations}`, not paged, best match first; terms up to 100
  characters (the Go client's limit). Prefixes match (`jolic` finds `jolicode`); an empty term finds nothing.
- **Errors** are `{message, code?, errors?}`:
  - 404: `{message: "No query results for model [App\\Models\\Commit] …"}` (no `code`).
  - 401 `invalid_signature`: timestamp outside **±300 s** ("Signature expired. Check that your clock is right.").
    `dn` should say so, not ask to reconnect.
  - 401 `device_not_approved`: unknown or revoked key; offer `:connect`.
- **Nulls and minimal objects**: some strings arrive as `null` (an author's `avatar_url`, a link card's `title` and
  `description`), and a deleted reply in `reply_preview` is only `{id, deleted_at, parent_id, replies_count}`. The
  models treat null like absent and require nothing but a commit's `id`.
- **Not observed** (no data on the account, or side effects): starred items, polls, image descriptions,
  notifications. Use the Go testdata for those.

## Milestones

Vertical slices; each ends runnable. Images affect layout, so the document model supports fixed-height block spans
from M1 even though graphics land in M6.

### M0 Spikes (scratchpad, throwaway)

- [x] **M0.1 Graphics probe**: placeholders inside a scrolling buffer, native animation, and both at once, run by the
  user in Ghostty 1.4.0. All passed; D7 holds as designed (results in D7).
- [x] **M0.2 Signing**: generate a key, SSHSIG-sign from file and via `ssh-agent`, verify with `ssh-keygen -Y verify`,
  and compare with the Go implementation's output for the same input. All byte-identical (results in D9).
- [x] **M0.3 Enrollment**: enrolled a `dn` device against dev-null.sh via the loopback callback (Rust, `reqwest` 0.12 +
  `rustls`); `POST /api/v1/devices` and signed `GET /api/v1/me` both 200. Protocol recorded in D8.
- [x] **M0.4 Capture**: 23 responses (`/me`, three logs and a second page, threads with and without ancestors,
  replies, own and another profile, followers/following, starred, an organization and its tabs, suggested users,
  404s) plus auth failures (clock skew, unapproved key). Notifications skipped: `GET /api/v1/notifications` marks
  every category read on the server (the unread count lives in `/me`'s `unread_notifications_count`, which does not).
  Findings in "API notes". The enrolled spike key now lives at `~/.config/devnull/device_ed25519` for M2.

### M1 Skeleton & feel (no network)

- [x] **M1.1** Workspace (`crates/devnull-api`, `crates/devnull` → bin `dn`); CI: fmt, clippy, test on Linux; build on
  macOS.
- [x] **M1.2** `doc`: lines, spans, wrapping; cursor and motions (`hjkl w b e W B E 0 ^ $ f t F T gg G { } ]] [[`,
  counts); anchor preserved across inserts above, block replacement and removal, and re-wraps; target under the
  cursor; styled row segments for rendering. 32 tests. Deferred: image lines (with M6), `H M L` (need the window's
  viewport, M1.6), `; ,` (the keymap repeats the last `f t F T`, M1.5).
- [x] **M1.3** Image lines (fixed-height rows); `/ ? n N * #` search; `v V` + `y`; folds `zo zc za zR zM`. 50 tests.
  ⏸ review the document model.
  - Search uses Rust `regex` syntax, case-sensitive by default (`CaseMode` carries `'ignorecase'`/`'smartcase'` once
    options exist, M1.5). Matches span wrap points, not line breaks.
  - `V` selects screen rows; its yank joins the rows of a wrapped line without a break and ends with `\n`.
  - Folds are declared by buffers per block (`Content { lines, folds }`), keep their state across block replacement,
    and count as one stop for text motions.
  - Known gaps: folds don't nest; fold state lives in the document, not per window. (`*` falls back to the
    non-blank word, as Vim does, since the v1 leftovers.)
- [x] **M1.4** Runtime: tokio (current-thread) loop, Elm core (`App::update(Msg) -> Vec<Effect>`), terminal
  setup/teardown with panic hook, focus events, bracketed paste (pastes never run commands), kitty keyboard protocol
  where supported (`<C-i>` ≠ `<Tab>`), `Key` type in Vim terms. Intro screen as a placeholder; `q` quits. Verified
  in a pty smoke test (modes entered and left, exit 0); the panic path is not exercised yet.
- [x] **M1.5** Pure building blocks, wired into the app in M1.6 (motions need windows to act on):
  - `key`: Vim key notation, parsed (unknown names taken literally, as Vim does) and printed.
  - `keymap`: built-in Normal/Visual tables from D5 with descriptions; counts (`0` is a motion unless a count
    started); `f t F T` with `; ,`; user `map`/`noremap` shadowing built-ins, with the count on the first action
    and E223 for recursion; a map's keys after a mode change come back as `Action::Keys` for the new mode;
    built-in prefixes wait indefinitely, ambiguous user maps resolve on `timeout`; `set_leader`; which-key
    continuations. Application keys run ex commands by name (`s` → `:star`).
  - `ex`: `:q :qa :map …noremap :unmap :set :hi :colorscheme :source :messages` with Vim's abbreviations and error
    numbers; blank lines and `"` comments are nothing. No ranges, no `|` chaining.
- [x] **M1.6** The app, wired: 130 tests, plus a pty smoke test of the real binary (modes, which-key timer, exit).
  ⏸ review.
  - Modes: Normal, Visual (`v`/`V`, switch and leave like Vim, highlight), command line (`:`, `/`, `?` with Vim's
    editing keys, history, paste), and the `:messages` "Press ENTER" prompt.
  - Ex: `:q :qa :set :map … :unmap :messages :split :vsplit :close :only :wincmd`; options `ignorecase`,
    `smartcase`, `scrolloff`, `timeoutlen`, `leader`. `:hi :colorscheme :source` answer E319 until M1.7.
  - Search: `/ ? n N * #` with Vim's wrap and not-found messages; a match in a closed fold opens it.
  - Yank: `y`, `yy` to the system clipboard over OSC 52; "N lines yanked" above `'report'`.
  - Windows: split tree, `<C-w>hjklwsvco`, `q` closes a window and quits with the last; a statusline per window
    (mode only on the current one); `│` separators.
  - Viewport: pinned to text like the cursor; `<C-e> <C-y> <C-d> <C-u> <C-f> <C-b> zt zz zb H M L`, `'scrolloff'`.
  - Timers: the app asks for wake-ups (`Effect::Wake`) for `'timeoutlen'` and the which-key popup (200 ms); stale
    ones are ignored.
  - Jumplist per window: searches, `gg G { } ]] [[ H M L` are jumps; `<C-o>`/`<C-i>`, one entry per row.
  - Known gaps: no `'incsearch'`/`'hlsearch'` yet; the separator column also crosses statusline rows. (A narrow
    statusline drops the account, then shortens the name as Vim does, `<` and its end, to keep the ruler; the
    jumplist spans buffers since M3.5.)
- [x] **M1.7** Highlight groups (`:hi` names, including `DevnullHandle` and `StatusLineMode*`); colorschemes
  `tokyonight` (= night), `tokyonight-night|storm|moon|day` and `default` (terminal colours); `:hi` (set, show, Vim's
  E411/E423/E254/E418), `:hi clear`, `:colorscheme`; `init.vim` from `$XDG_CONFIG_HOME/devnull/` or
  `~/.config/devnull/` at startup and `:source`, with Vim's per-line error report. 148 tests; pty smoke test of
  `init.vim` (error report, colorscheme applied).
  - The tokyonight table is generated from the plugin's resolved highlight groups (`extras/lua`, commit `cdc07ac`);
    its Apache-2.0 license is in `LICENSES/`.
  - `:set nofoo`/`invfoo` prefixes are resolved by the options, so unknown names report as typed.
  - `:hi link {from} {to}` (and `NONE`, as Vim; highlighting a linked group unlinks it) since the v1 leftovers.
  - Known gaps: `:colorscheme` drops `:hi` changes (as Vim's `:hi clear` does) with no `ColorScheme`
    autocommand to reapply them.
- [x] **M1.8** `dn` starts on a made-up `log/following` (`fake.rs`, invented people and posts): commit blocks with
  header, body (mentions and links carry targets, inline code), counts, and long commits folded ("… N more
  lines"); new groups `DevnullAuthor|Time|Hash|Stats|Link|Code`. Full-screen `insta` snapshot. 150 tests; both pty
  smoke tests pass. ⏸ try it in Ghostty.

### M2 Auth & logs

- [x] **M2.1** `devnull-api` signing: key file or ssh-agent (D9), canonical message; tested against the Go client's
  signatures from M0.2 (throwaway test key, never enrolled), including through a private ssh-agent and an encrypted
  key.
- [x] **M2.2** `devnull-api` client: async `reqwest` 0.13 (rustls), models for `/me` and `/log/*`, errors by what a
  caller does (`NotApproved`, `BadSignature`, `NotFound`, `RateLimited`, `Network`, `Refused`, `Invalid`); tested
  against a mock server that verifies every signature. Checked live against dev-null.sh (`/me`, three logs, a second
  page) and against all 89 commits captured in M0.4.
- [x] **M2.3** `devnull-api` enrollment: the M0.3 loopback flow as a library (`start`, `wait`, `code_from_url` for
  SSH, `redeem` with the identity check), tested with a mock server and the browser played by plain requests.
- [x] **M2.4** `dn` connection: the key from `~/.config/devnull/device_ed25519` (ssh-agent first), verified with
  `/me`; a `connect` buffer per state; `:connect` (retry, approve, or a pasted callback link), a masked
  `Passphrase:` prompt, terms with `:agree`; `@handle` in the statusline. Runtime `net` module holds the client.
  Verified in a pty: the stored key connects; without a key the `connect` buffer shows. Not verified end to end
  through `dn`: approving a new device (needs a browser; the library flow is mock-tested, and M0.3 did it live).
- [x] **M2.5** `dn` log buffers rendered from real commits, through `Effect::Fetch` / `Msg::Fetched` (stamped with
  the arrival time for ages). `render`: header (author, `(org)`, age, hash), text segments with mentions (only the
  commit's) and links as targets, inline code, code blocks (server HTML reduced to text), quotes under `┃`, link
  cards, image placeholders, counts and status, reply previews, "Commit deleted"; bodies over six lines fold.
  `fake.rs` retired: the invented log is now an API-shaped fixture. Verified in a pty against the real log.
  ⏸ try against dev-null.sh.
  - Not yet: `preview_segments` (the server's excerpt) is not used; the fold shows the first six lines instead.
- [x] **M2.6** Paging when the cursor is within a window's height of the end (one request at a time, a "Loading
  more…" line); `:e!`/`<C-l>` refresh merges the first page (new commits above, known ones updated in place, the
  cursor stays on its commit; no overlap or Trending replaces the log); following and latest polled every 15 s
  focused, 30 s unfocused, with "N new commits" when reading further down. Poll wake-ups have their own generation
  so typing does not stop them. `:e log/{following,latest,trending}` opens a log.
  - Differs from the original on purpose: a refresh keeps the cursor where it is instead of jumping to the top.
- [x] **M2.7** Buffers: `<leader>1/2/3` and `:e log/…` open the logs; the bufferline lists the listed buffers
  (`connect` is an unlisted status page); `:ls`/`:buffers`, `:b {N|name}` (unique partial names, E93/E94), `:bn`,
  `:bp`; a window returns to where it left each buffer (cursor, scroll, jumps). `:ls` numbers listed buffers in
  order, which is as stable as Vim's numbers while buffers are never deleted (there is no `:bdelete`); it marks the
  alternate buffer `#`, which `:b#` and `<C-^>` go to.

### M3 Navigation

- [x] **M3.1** `devnull-api`: thread (commit and ancestors), replies, user profile and its commits,
  followers/following, organization page and its commits and mentions. Handles and ids are validated before they go
  into a path. Checked live (thread, replies, profile, both lists, own commits, organization and tabs) and
  against the M0.4 captures.
- [x] **M3.2** Thread buffers: `<CR>` (`:open`) opens the thread of the commit under the cursor, or of a quote under
  it, and links in the browser; ancestors, the commit whole, then paged replies. A block can refer to something as a
  whole (`Content::target`), so any text of a commit without a target of its own refers to the commit, and the text of
  a quote to the quoted one. Threads are named `thread/{short hash}`, start with the cursor on their commit, and
  reload with `:e!`; they are not polled. Checked live: `<CR>` on the following log, then `:e!`.
- [x] **M3.3** Profile buffers (`gd`, `<CR>` on a mention): header, pinned commit, commits; followers/following
  lists; organization pages. Mention targets know user from organization; `Target::Buffer` links text to a buffer
  (a profile's counts open its lists). Buffers are `@ada`, `@ada/followers`, `@ada/following` and `@acme/mentions`,
  all opened with `:e` too; `:e profile` (`<leader>p`) is your own. `:profile [handle]` (`gd`) opens the account under
  the cursor, else the commit's author. Logs, threads and profiles are one kind of buffer, a feed: a head, then a list
  a page at a time, merged by the list's order (newest first, oldest first, ranked). Checked live: own profile, its
  followers, a follower's profile, an organization and its (empty) mentions.
- [x] **M3.4** `gx` (link under the cursor), `gX` (commit on the web), `gy` (yank its link); `K` hover card.
  `:browse link` (`gx`) opens only a link; `:browse` (`gX`) and `:yanklink` (`gy`) take the commit under the cursor (a
  quote's, else the one holding it), else the account (`https://dev-null.sh/@handle`, as every captured `html_url`).
  `:hover` (`K`) floats a card next to the cursor until the next key: a link card whole, an account's profile head
  (fetched), or elsewhere in a commit, its author's. Polls get theirs when polls are drawn. Checked live: `K` on a
  commit and `gy` (OSC 52); `gx`/`gX` only in tests, as they open the browser.
- [x] **M3.5** The jumplist spans buffers: `<C-o>` from a thread returns to where `<CR>` was pressed. Entries carry
  their buffer, and showing another buffer is a jump (as starting to edit another file is in Vim), except from a
  status page. `<C-t>` (`:pop`) is Vim's tag stack: `:open` and `:profile` push where they left from when they show
  another buffer. `<Tab>` also jumps forward: terminals without the kitty keyboard protocol send `<C-i>` as `<Tab>`
  (a gap since M1). Checked live without the kitty protocol: `<CR>`, `<C-o>`, `<Tab>`, `<C-t>`. `:jumps` lists the
  jumplist as Vim does (distance, row, column, the text there or the other buffer's name, `>` where one is).
- [x] **M3.6** Pickers: fuzzy buffer switching (`<leader><leader>`) and user search (`<leader>/`). `:pick buffers`
  lists the listed buffers, the last shown first, starts on the one before, and narrows fzf-style (smartcase, word
  starts and runs score higher); `:pick users` asks `/search/profiles` once typing pauses (120 ms, as the Go client)
  and ignores answers to older queries. `<C-n>`/`<C-p>` choose, `<CR>` opens, `<Esc>` leaves. Matched characters are
  not highlighted yet. Checked live: both pickers, opening `@jolicode` from a search.

### M4 Inbox

- [x] **M4.1** `devnull-api`: the inbox (categories, cursor, when it was read) and starred commits.
- [x] **M4.2** `starred` (`<leader>s`); the statusline counts unread notifications (`󰂚 3`), from `/me` on connecting
  and at each poll, which marks nothing read.
- [x] **M4.3** `notifications` (`<leader>n`), and filtered as `notifications/replies,mentions`: what happened, by
  whom, and the commit it is about; `●` marks what is new since the last visit; a group's accounts fold under it;
  `<CR>` opens the commit's thread or the follower's profile. Opening it clears the count. It is never polled and a
  refresh replaces the list (groups move up as they grow). Checked live: `starred` (empty on the account) and the
  count; not the inbox itself, as reading it marks it read.

### M5 Writing & actions

- [x] **M5.1** `devnull-api`: the body budget, preview, publish (commit, reply, quote), delete, star, follow. The preview
  was checked live (it changes nothing); the writes only against a fake server.
- [x] **M5.2** Actions: `s` (`:star`), `:follow`/`:unfollow`, `dd` (`:delete`, confirmed). They act on the commit or
  account under the cursor as `gX` finds it; a star redraws the commit wherever it shows (a thread's own, whole),
  following refreshes an open profile, and a deleted commit says so where it was. Only against a fake server: each
  writes to the account.
- [x] **M5.3** `$EDITOR` composer for commit/reply/quote with server preview and keep-draft (D3); `gy` is done (M3.4).
  `c`/`r`/`R` (`:commit`, `:reply`, `:quote`) open the kept draft or an empty one, the context below the scissors;
  the editor gets the terminal (the key reader stopped, the screen redrawn after without asking the terminal for
  its cursor). Saved: empty aborts as git does, the budget is checked, the server previews, and
  `reply 12/4000 → @lin? [y]es/[e]dit/[n]o` asks with the preview above; `n` (or a refusal) keeps the draft. A
  published reply joins its open thread. Shortcodes are the server's (see Open questions). Not done: posting as an
  organization. Checked live with a stand-in editor up to the preview, answering `n`: nothing was published.

### M6 Graphics

- [x] **M6.1** `devnull-api::media`: pictures downloaded unsigned as the Go client does: HTTPS, no proxy, only from
  the public internet (names resolved and IP literals checked, redirects too) or the API's origin; 10 MB, 12 s.
  Checked live (a GitHub avatar; loopback refused).
- [x] **M6.2** `graphics`: decode PNG, JPEG, WebP, GIF (first frame; 8 megapixels at most), fit to cells (2:1), send
  with the kitty protocol as a virtual placement shown by placeholder cells, or draw with half blocks.
- [x] **M6.3** Commit pictures in buffers: an image line (at most 48 × 14) above the 🖼 caption, asked for once in
  view, drawn by placeholders or half blocks; `:set graphics=kitty|halfblocks`, guessed from `$TERM`/`$TERM_PROGRAM`
  (half blocks in tmux), not by the `a=q` query D7 planned (its answer would come back among the keys). Pictures sent
  are freed when switching and on quitting. Multiplexers that rename `$TERM`/`$TERM_PROGRAM` defeat the guess (herdr,
  though it passes kitty placeholders through): there, `:set graphics=kitty` in `init.vim`. Checked live in a pty: 8 pictures sent and freed, placeholders drawn;
  3568 true-colour half blocks. Seen working in Ghostty (through herdr, with `graphics=kitty`).
- [x] **M6.4** Avatars and link-card pictures. An avatar sits beside each name (2 × 1 cells, in a line of text:
  `Span::picture`, braille blanks so it wraps as a word) for authors, people in lists and notification actors; a
  profile opens with its avatar (8 × 4); a link card's picture follows it (32 × 8, its size unknown until it
  arrives). Pictures keep their shape in their box (centred, transparent around). GitHub avatars are asked at the
  size shown. Small pictures say nothing while loading (it would cover the name). Checked live: 27 pictures sent
  and freed (kitty), 4139 half blocks.
- [ ] **M6.5** Animation: commit pictures are JPEG, PNG or WebP (the server refuses others), so only link cards could
  move.

### Then

- v1 release; v2: Remotes, polls, search, attaching images, settings.

## Open questions

- ~~Emoji shortcode data source (M5)~~: none needed. The server replaces `:shortcode:` outside code itself (checked
  live through the preview in M5.1); `dn` sends what was typed and shows the preview. That publishing does the same
  is likely but not checked (it would publish).
- ~~Server preview endpoint behavior and limits (M5)~~: see "Commit writing" in the API notes.
