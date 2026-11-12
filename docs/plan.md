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
| `<C-o>`, `<C-i>`, `<C-t>` | Jumplist back/forward | |
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
| `<leader><leader>` | Buffer picker | `:ls` |
| `<leader>/` | User picker | |

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
  `/organizations/{handle}/{commits,mentions}`.
- **Errors** are `{message, code?, errors?}`:
  - 404: `{message: "No query results for model [App\\Models\\Commit] …"}` (no `code`).
  - 401 `invalid_signature`: timestamp outside **±300 s** ("Signature expired. Check that your clock is right.").
    `dn` should say so, not ask to reconnect.
  - 401 `device_not_approved`: unknown or revoked key; offer `:connect`.
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
  - Known gaps: search landing in a closed fold doesn't open it yet (the window will, Vim's `'foldopen'`); `*` only
    picks keywords, not Vim's fallback to any non-blank string; folds don't nest; fold state lives in the document,
    not per window.
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
- [ ] **M1.6** Wire keymap and ex into the app (Normal, Visual and command-line modes, `:messages`); `window`
  split tree, `<C-w>` family, `:sp :vs :only :close`, `H M L` and scrolling; bufferline, statusline, cmdline,
  which-key popup; jumplist. ⏸ review.
- [ ] **M1.7** `hl` groups, tokyonight night/storm/moon/day, `~/.config/devnull/init.vim` sourcing.
- [ ] **M1.8** Fake log buffer wired end to end; `insta` screen snapshots. ⏸ try it in Ghostty.

### M2 Auth & logs

- `devnull-api`: signing, enrollment, terms consent, `/me`, `/log/*`.
- `dn`: login flow; Following, Latest, Trending buffers; auto-paging; `:e!`; polling with cursor anchoring.

### M3 Navigation

- Thread and profile buffers, followers/following, jumplist, `<CR>`/`gd`/`gx`/`K`, buffer and user pickers.

### M4 Inbox

- Notifications (filters, groups as folds), Starred, unread counts in the statusline.

### M5 Writing & actions

- `$EDITOR` composer for commit/reply/quote with server preview, shortcodes, keep-draft; star, follow, delete, `gy`.

### M6 Graphics

- Avatars, inline images, link-card pictures, animation; half-block fallback.

### Then

- v1 release; v2: Remotes, polls, search, attaching images, settings.

## Open questions

- Emoji shortcode data source (M5): match the original's `tools/emoji` generator so shortcodes agree with the web.
- Server preview endpoint (`/api/v1/commits/preview`) behavior and limits (M5).
