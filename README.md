# heyl

Use the passwords in your [heylogin](https://heylogin.com) account from a shell, a script or an AI
agent — approved on your phone, and never written to disk by heyl.

```sh
heyl get -l github.com -f password                              # print one password
heyl run --project github.com/me/app/prod -- terraform apply    # run a command with its secrets
```

> **Unofficial.** heyl is not affiliated with, authorised by, or endorsed by heylogin. "heylogin" is
> a trademark of its respective owner. This is an independent reimplementation of the client
> protocol, built for interoperability, containing no code from heylogin's own clients. Please do
> not report heylogin service issues here.

1. [What it does, and doesn't](#1-what-it-does-and-doesnt)
2. [Install](#2-install)
3. [Quick start](#3-quick-start)
4. [Reading logins: `heyl get`](#4-reading-logins-heyl-get)
5. [Running a command with a project's secrets: `heyl run`](#5-running-a-command-with-a-projects-secrets-heyl-run)
6. [Sessions](#6-sessions)
7. [Approving requests](#7-approving-requests)
8. [Scripting reference](#8-scripting-reference)
9. [Security](#9-security)
10. [Troubleshooting](#10-troubleshooting)
11. [Further reading, contributing, license](#11-further-reading-contributing-license)

## 1. What it does, and doesn't

heyl reads logins from your personal, team and organisation vaults and either prints them
(`heyl get`) or hands them to another program (`heyl run`). When it needs access, it asks your phone
— the same approval you give when heylogin unlocks a browser.

Not supported:

- **Creating, editing or deleting logins.** heyl only reads; use heylogin's own apps to change
  things.
- **Listing or searching logins.** You name the login you want.
- **One-time codes (TOTP).**
- **Unattended use.** Somebody has to approve on a phone, at least about once a day. Servers and CI
  machines without a person nearby are not a supported setup.

## 2. Install

The easiest way is [uv](https://docs.astral.sh/uv/):

```sh
uv tool install heyl      # put `heyl` on your PATH
uv tool upgrade heyl      # get a newer release later
uvx heyl --help           # or run it without installing
```

No Python code is involved; the package just carries the program. Plain archives with checksums are
attached to each [GitHub release](https://github.com/mrh1997/heyl/releases). There is a new release
every night that something changed — see the
[changelog](https://github.com/mrh1997/heyl/blob/main/CHANGELOG.md).

| System | Processors | Also needed |
|---|---|---|
| Windows | x86_64, ARM64 | – |
| macOS | Apple Silicon only | – |
| Linux | x86_64, ARM64 | A keychain offering the *Secret Service* (GNOME Keyring, KWallet, …). For the approval window: `zenity`, or `kdialog` on KDE |

Processors from before about 2014 and the Raspberry Pi 4 or older are not supported.

## 3. Quick start

Connect this computer to your heylogin account. heyl shows a QR code; scan it with the heylogin app
on your phone:

```sh
heyl session create
```

Then read something. The first time, your phone asks for approval; afterwards the session stays
unlocked for a while (8 hours, unless you [change that](#63-settings)):

```sh
heyl get -l github.com                  # the whole login
heyl get -l github.com -f password      # just the password
```

To undo it all: `heyl session unlock`, then `heyl session remove`.

## 4. Reading logins: `heyl get`

```
heyl get  -l <login>  [-f <field>]  [-v <vault>]
```

### 4.1 Choosing a login, a field and a vault

| | By name | By id | If left out |
|---|---|---|---|
| **Login** | `-l` / `--login-title` | `-L` / `--login-id` | required |
| **Field** | `-f` / `--field-name` | `-F` / `--field-id` (custom fields only) | the whole login is printed |
| **Vault** | `-v` / `--vault-name` | `-V` / `--vault-id` | all your vaults are searched |

- **Logins** match on their name, title or any of their websites. The match is exact, ignoring upper
  and lower case: `-l github.com` does not find `www.github.com`.
- **Fields** are `id`, `name`, `title`, `username`, `password`, `website`, `note`, `labels`,
  `created` and `edited`, plus the login's custom fields under their own names. Field names are case
  sensitive; if a custom field has the same name as a built-in one, `-f` means the built-in.
- **Vaults** are named `private`, `inbox`, or by the name of the organisation or team.

Without a field, every non-empty field is printed, *including the password*:

```
$ heyl get -l github.com
id: f1bcfdf7-c3ca-4fc6-925d-f57d5e8919bd
name: GitHub
username: me@example.com
password: correct-horse-battery
website: github.com
API_TOKEN: ghp_…
```

### 4.2 When several logins match

heyl does not stop and ask. It takes one, and says on stderr how many matched. The order of
preference is:

1. active logins before archived ones, and archived before deleted ones (deleted and archived logins
   can still be read);
2. the private vault, then organisation, team, and inbox;
3. the most recently changed.

To be certain which login you get, use its id with `-L`.

### 4.3 Using it in scripts

The value goes to stdout, exactly as stored and without a trailing newline (except on a terminal).
Messages and prompts go to stderr, so capturing is safe:

```sh
TOKEN=$(heyl get -l github.com -f API_TOKEN)          # sh, bash, zsh
$token = heyl get -l github.com -f API_TOKEN          # PowerShell
heyl --format json get -l github.com | jq -r .username
```

A field that exists but is empty prints nothing and succeeds. A login, field or vault that does not
exist fails with exit code 2.

When the secret is meant for another program, prefer `heyl run`: the value then never passes through
your shell, its variables or its history.

## 5. Running a command with a project's secrets: `heyl run`

`heyl run` reads **one login** and puts each of its **custom fields** into the environment of a
command. No `.env` file is needed, and nothing has to be listed in your repository.

### 5.1 Setting up the project's login

In the heylogin app, create one login per project — or one per environment of a project:

1. Give it a website that identifies the project, usually the repository address, optionally
   followed by the environment: `github.com/me/app/prod`.
2. Add one custom field per variable. The field's name is the variable's name, e.g. `DATABASE_URL`;
   mark secrets as protected if you like.

Only custom fields are used. The login's own username, password and note are ignored.

### 5.2 Running the command

```sh
heyl run --project github.com/me/app/prod -- npm run deploy
```

The command gets your current environment plus the project's variables. It uses your terminal as
usual, and its exit code becomes heyl's, so existing scripts and makefiles keep working. heyl keeps
the values in memory only: not on disk, not on the command line, not in its own output.

The variables are visible to the command, not to the shell you type in. To use one in an argument,
let a shell started by heyl expand it:

```sh
heyl run --project github.com/me/app -- sh -c 'curl -H "Authorization: $API_TOKEN" https://…'
```

### 5.3 Rules

- **Matching.** The project must equal the login's name, title or one of its websites, ignoring case
  and a leading `https://` or `http://`. `…/app` does not match `…/app/prod`.
- **Which vaults.** All except the inbox, which other people can send to. Limit the search to one
  vault with `-v` or `-V`.
- **Which logins.** Only active ones; archived and deleted logins are skipped. If several match, the
  order of [4.2](#42-when-several-logins-match) decides and heyl says so on stderr. They are never
  merged.
- **Variable names.** Letters, digits and `_`, not starting with a digit, and no name twice.
  Otherwise heyl stops before your command starts.
- **Already set variables** are replaced by the project's value, with a note on stderr.
- **No matching login** is an error, and the command does not run. A login *without custom fields*
  is not: the command runs with nothing added, and a note says so.

### 5.4 Letting an AI agent use it

Give the agent its **own session**, so that your phone tells you who is asking and the agent has to
ask every time:

```sh
heyl session create claude-code --display "Claude Code" --strict
```

Then set `HEYL_SESSION` in the environment the agent runs in. For Claude Code, that is
`.claude/settings.json`:

```json
{ "env": { "HEYL_SESSION": "claude-code" } }
```

From now on, whenever the agent uses heyl, your phone and a window on your screen say that *Claude
Code* requests passwords. You keep working in your own session, with your own settings.

- **Tell the agent to use `heyl run`, not `heyl get`.** What `get` prints ends up in the
  conversation, and so with the model's provider. With `run`, the value goes straight to the
  command — as long as the command does not print it.
- **Let it give up:** `heyl --wait 120 run …` ends with exit code 4 after two minutes without an
  approval. Without `--wait`, heyl waits indefinitely.
- **A separate session is not a sandbox.** The agent runs as you, so nothing stops it from selecting
  your session instead, and while that is unlocked, nobody is asked. See
  [9.3](#93-what-heyl-does-not-protect-against).

## 6. Sessions

### 6.1 What a session is

A session is this computer's connection to your heylogin account. Your phone lists it as a device,
and shows its name when it asks for approval. The name is all the approval screen says about who is
asking, so name sessions after whoever uses them.

Most people need only one, called `default`. Further sessions make sense for agents and scripts:
each has its own keys, its own settings and its own entry in the app, so an agent and you can hold
opposite policies at once. Select one with `--session <name>` or `HEYL_SESSION`.

### 6.2 Commands

| Command | What it does |
|---|---|
| `heyl session create [name]` | Connect a new session by scanning a QR code. It starts locked; with `--unlock`, the scan also unlocks it, until 02:00 the next day |
| `heyl session list` | Show this computer's sessions: locked or unlocked, and their settings |
| `heyl session unlock [name]` | Ask your phone now, and wait |
| `heyl session lock [name]` | Lock now, and withdraw a pending request |
| `heyl session get [name] [setting]` | Show the settings. Never asks your phone |
| `heyl session set [name] <setting> <value>` | Change a setting |
| `heyl session remove [name]` | Disconnect the session for good. It must be unlocked at that moment; `--force` removes it regardless, but may leave its entry in the app |

Without a name, commands act on the selected session. You can also remove a session as a device in
the heylogin app, for example when the computer is lost.

### 6.3 Settings

| Setting | Meaning | Default |
|---|---|---|
| `display-name` | The name your phone shows. Changing it needs an unlocked session | the session's name; `heyl CLI` for `default` |
| `timeout` | How long an approval lasts: `5m`, `8h`, … at least one minute | `8h`; `1m` with `create --strict` |
| `strict` | `on`: lock again as soon as a command has read its secrets, so every command asks | `off` |

```sh
heyl session set timeout 1h             # your default session
heyl session set claude-code strict on  # another one
```

Each can also be given when creating a session: `--display`, `--timeout`, `--strict`. However long
the timeout, every unlock ends at 02:00 the next day at the latest.

## 7. Approving requests

When a command needs access and the session is locked, three things happen at once: your phone gets
a request, the terminal says heyl is waiting, and **a small window** opens in front of everything,
naming the session. The window disappears when you approve; *Cancel* aborts the command. This way
you also notice requests from scripts and agents whose terminal nobody is watching.

- **Only the terminal:** `--ui terminal`, or `HEYL_UI=terminal`. Where no window can open, such as
  over SSH, heyl does this by itself.
- **Limit the wait:** `--wait <seconds>`, for `get`, `run` and `session unlock`.
- **Your own notification:** `--ui-cmd <command>`, or `HEYL_UI_CMD`, replaces the window for approval
  requests. Pairing always uses the built-in one.

```sh
HEYL_UI_CMD='notify-send "$HEYL_UI_HEADING" "$HEYL_UI_MESSAGE"' heyl get -l github.com
```

Such a command finds `HEYL_UI_SESSION`, `HEYL_UI_HEADING` and `HEYL_UI_MESSAGE` in its environment.
When the wait ends, it receives one line on stdin — `approved`, `timeout` or `failed` — and stdin is
closed; if it is still running five seconds later, it is terminated. It may print `cancel` to abort.
Simply exiting early is fine and cancels nothing. It runs in `sh`; on Windows in `cmd`, or PowerShell
for a `.ps1` file. `--ui-shell sh|cmd|powershell|pwsh` overrides that.

## 8. Scripting reference

### 8.1 JSON output

`--format json` prints one compact JSON document per line on stdout, `--format json-pretty` the same
indented. The exact shapes may still change.

| Command | Document |
|---|---|
| `get` | An object with the fields the login has: `id`, `displayHeadline`, `title`, `username`, `password`, `websites`, `note`, `tags`, `customFields` (a list of `name` and `value`), `creationTime`, `editTime`, `changeTime`, `isDeleted`, `isArchived` |
| `get -f` | The value, as a JSON string |
| `run` | Nothing; stdout belongs to your command |
| `session list` | A list of objects: `slot` (the session's name), `known`, `unlockedUntil`, `unlockRequested`, `policy` |
| `session create` | Two documents: `{"pairingUrl": …}` while waiting for the scan, then the result |
| other `session` commands | An object with `slot` and the result |

```sh
heyl --format json session list | jq -r '.[] | select(.unlockedUntil) | .slot'
```

Errors are always plain text on stderr, so rely on the exit code.

### 8.2 Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | General failure |
| 2 | Not found: no such session, vault, login, field or setting. Also: the command line could not be understood |
| 4 | Not approved: still locked when `--wait` ran out, or cancelled |
| 5 | heylogin or the network reported an error |
| 6 | A session of that name already exists |
| 7 | A value is not acceptable |
| 126, 127 | `run` only: the command could not be started, or was not found |

Once `run` has started your command, that command's own exit code is passed on.

### 8.3 Environment variables

| Variable | Same as | Meaning |
|---|---|---|
| `HEYL_SESSION` | `--session` | Which session to use |
| `HEYL_UI` | `--ui` | `window` or `terminal` |
| `HEYL_UI_CMD` | `--ui-cmd` | Your own approval notification |
| `HEYL_UI_SHELL` | `--ui-shell` | The shell that runs it |

`heyl <command> --help` describes every option.

## 9. Security

### 9.1 What is stored on your computer

Your account's **secret key is never stored**, and neither are your logins. For each session, the
system keychain holds an access token, the session's private key, its id and its display name; plus
one list of the sessions' names. None of these can decrypt your vaults on its own.

When you approve on your phone, it encrypts the secret key for this one session and leaves the
result with heylogin. Every heyl command that reads a secret fetches it, uses it in memory, and wipes
it before it ends. Both halves are needed: heylogin holds an encrypted key it cannot open, your
keychain holds a key with nothing to open. Memory holding key material is locked so that it is never
written to swap, and core dumps are disabled on Linux and macOS; if memory cannot be locked, heyl
refuses to run.

### 9.2 What an approval grants, and when it ends

An approval unlocks **the whole account** for that session — not one login, and not one command.
When the time is up, heylogin's server stops handing out the encrypted key. The limit therefore does
not depend on heyl, or on the computer it runs on: whoever steals your computer after the unlock has
ended gets nothing out of the keychain that decrypts your logins. While it lasts, they have the same
access as with an unlocked browser. Keep timeouts short, or use `strict` sessions, where that
matters, and remove a lost computer in the heylogin app.

A `strict` session locks itself once a command has read its secrets. If reading fails, for instance
because there is no such login, it stays unlocked until its timeout.

### 9.3 What heyl does not protect against

- **Malicious programs running as you.** They can run heyl, choose any of your sessions and read
  whatever an unlocked one can reach; they may also read your keychain. Sessions and approvals keep
  *well-behaved* programs in check and make their requests visible. They do not contain hostile
  ones.
- **A program that keeps the key.** While a session is unlocked, hostile software could extract the
  secret key and use it indefinitely. Locking or removing the session does not undo that; it takes
  renewing the keys of your heylogin account.
- **Whatever receives the secret.** `get` prints it, and `run` gives it to a command. Where it goes
  from there is up to them.
- **People who can write to your shared vaults.** In `run`, a custom field's name becomes a variable
  name. Anyone able to create a matching login can therefore set any variable for your command,
  including ones that load code, such as `PATH` or `NODE_OPTIONS`. Logins in your private vault take
  precedence and the inbox is not searched; if you do not trust everyone in a team, use
  `-v private`.

### 9.4 What heyl writes

heyl never creates, changes or deletes a login. The only thing it ever writes to your vaults is the
entry that lists its own session as a device. Logins are decrypted on your computer only; heylogin's
server holds nothing but their encrypted form.

The [design document](https://github.com/mrh1997/heyl/blob/main/DESIGN.md#3-security-design)
describes the concept in detail.

## 10. Troubleshooting

**`access_token not found`** — this session does not exist on this computer. Check
`heyl session list` and `HEYL_SESSION`; create it with `heyl session create <name>`.

**`this session is locked`** — run `heyl session unlock` and repeat the command. Commands that remove
or rename a session do not ask your phone by themselves.

**The QR code will not scan** — on a dark terminal it is drawn in inverted colours. Use the window,
switch the terminal to a light theme, or open the link printed below the code on your phone. If the
code looks distorted, try `--qr ascii`.

**The phone shows no request** — the device may have been removed in the heylogin app; heyl cannot
detect that. Run `heyl session remove --force`, then create the session again.

**A session is listed as `gone`**, or heyl says it no longer exists — it was deleted on heylogin's
side. Remove it with `--force` and create it again.

**The keychain cannot be opened (Linux)** — heyl needs a running Secret Service. Containers, minimal
servers and SSH logins often have none.

**`cannot lock memory`** — raise the limit: `ulimit -l unlimited`, or `LimitMEMLOCK=` in a systemd
unit.

**No window appears (Linux)** — install `zenity` or `kdialog`. Whether the window stays in front is
up to your desktop.

## 11. Further reading, contributing, license

| | |
|---|---|
| [CHANGELOG.md](https://github.com/mrh1997/heyl/blob/main/CHANGELOG.md) | What changed in each release |
| [DESIGN.md](https://github.com/mrh1997/heyl/blob/main/DESIGN.md) | Architecture, security design and plans |
| [HEYLOGIN_SPEC.md](https://github.com/mrh1997/heyl/blob/main/HEYLOGIN_SPEC.md) | heylogin's protocol, as far as it is understood |
| [tools/README.md](https://github.com/mrh1997/heyl/blob/main/tools/README.md) | Development tools |

Contributions are welcome as [pull requests](https://github.com/mrh1997/heyl/pulls).

Licensed under [Apache-2.0](https://github.com/mrh1997/heyl/blob/main/LICENSE); see also
[NOTICE](https://github.com/mrh1997/heyl/blob/main/NOTICE).
