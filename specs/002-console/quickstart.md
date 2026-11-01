# Quickstart: MicroSwarm Console

This is the end-to-end acceptance script. Each section maps to a user story. Run it against Docker with the fake model (integration harness) and once with a real model.

## 0 · Start

```console
$ microswarmd &
$ microswarm console
MicroSwarm console on http://127.0.0.1:8790
Open this link once (valid 10 min): http://127.0.0.1:8790/login?token=4f1c…e9a2
```

Check:
- Opening the link lands on `/#/sessions`, and the URL no longer contains the token.
- Opening the same link again shows "This link was already used. Run `microswarm console` again for a new one."
- `microswarm console --listen 0.0.0.0:8790` exits with a non-zero code and the message "The console only listens on loopback. Use an SSH tunnel (`ssh -L 8790:127.0.0.1:8790 host`) or a TLS proxy."

## 1 · Chat with a lead (US1)

1. With one Telegram session active, check that Sessions lists it with its status in words.
2. Open it. The conversation shows your Telegram messages ("You · via telegram") and the lead's replies.
3. Type "summarise what you found" and press Send. "You" appears at once, then "Lead is working…", then the reply, all without a reload.
4. Send again while the lead is still answering. The composer is disabled with "The lead is still answering your last message."
5. Click **New session**, type "hello", and press Send. A new session appears in the list with channel `web`, and the lead replies.
6. Click **End session** and confirm. The members stop, then the lead, and the session moves to "Ended today · Ended by you".

## 2 · Approvals (US2)

1. Ask a session with `code-review-team` to "run the auth tests". The header badge shows 1, and the session banner reads "Waiting for you".
2. Open Approvals. Check that the detail shows `microswarm-tools / shell.run`, the args, `binding sha256:…`, "simple command · program: cargo", and the tester's mounts.
3. Click **Approve this call**. The item moves to Decided, and the session receives the follow-up reply.
4. Repeat, approving from Telegram instead. In the console, the item shows "Already decided on another channel".
5. At 390 px width, the approval is usable without horizontal scrolling.

## 3 · Live team (US3)

While the researcher is busy, its row reads "Working". When the tester asks for approval, the tester's row and the banner change to "Waiting for you" within 1 s. Restart the console server: the page shows "Reconnecting…", then catches up.

## 4 · Member scope (US4)

Open the researcher. `/data/finance` is struck through with "not in lead scope". The Granted column matches:

```console
$ sqlite3 ~/.microswarm/supervisor.db "select scope_json from sessions where id='<member>'"
```

The container profile lists "root read-only", "/session/control read-only" and "reaches: session proxy only".

## 5 · Trace (US5)

The trace page's events equal the output of `microswarm trace <root> --json`. Diff the result of Export JSON against it: no difference.

## 6 · Host (US6)

Doctor shows the same checks as `microswarm doctor`. Set `web-search` to `test-secret-123`, then confirm the value never comes back:

```console
$ curl -s -b cookies http://127.0.0.1:8790/api/v1/host | grep -c test-secret-123
0
```

## Adversarial

Make the fake model reply with `<img src=x onerror=alert(1)>` and with `[click](javascript:alert(1))`. Both show as literal text, no dialog opens, and no link is created.
