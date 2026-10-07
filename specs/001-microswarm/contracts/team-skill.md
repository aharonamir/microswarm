# Contract: team skill format

A team skill is a directory under `$MS_HOME/skills/<name>/` containing `SKILL.md` and, optionally, role instruction files and scripts. Installing it registers its roles in `supervisor.db.team_roles`. Nothing in a skill runs on the host.

## `SKILL.md` front-matter

```yaml
---
name: code-review-team          # [a-z0-9-], unique
version: 1.2.0
description: Reviews a change with a researcher and a test runner.
kind: team                      # 'team' adds roles; plain skills omit 'roles'
roles:
  researcher:
    spawn_policy: allow         # allow | ask | deny  (host policy.toml may tighten, never loosen)
    instructions: roles/researcher.md
    scope:                      # a REQUEST template — still intersected with the lead at spawn
      mounts:
        - { path: /work, mode: ro }
      credentials: [web-search]
      tools:
        - { server: microswarm-tools, tools: [fs.read, fs.list, web.fetch] }
    limits: { max_turns: 30, memory_mb: 256 }
  tester:
    spawn_policy: ask
    instructions: roles/tester.md
    scope:
      mounts:
        - { path: /work, mode: ro }
        - { path: /work/target, mode: rw }
      tools:
        - { server: microswarm-tools, tools: [shell.run, fs.read] }
---
```

Below the front-matter, the Markdown body is the **lead's** guidance for when and how to use the team. Each role's `instructions` file is mounted read-only into that member's container at `/skills/<name>/role.md` and prepended to its system prompt.

## Rules

1. Role scope is a **request template**, never a grant. At spawn the request is `lead's spawn_member.scope ∩ template` when the lead supplies one (the lead can narrow the template, never widen it), otherwise the template alone. The result is then intersected with `lead_scope ∩ ceiling`. Templates and lead requests are never unioned.
2. `spawn_policy` is the skill author's ceiling. The effective policy is the stricter of the skill and `policy.toml` (`allow < ask < deny`).
3. A role cannot request delegation tools; members cannot spawn members.
4. Paths in `scope.mounts` are lead-relative container paths (`/work/...`), translated by the supervisor.
5. The skill's content hash is recorded; changing any file under the skill directory requires reinstall, which re-parses roles.
6. Skills may contain scripts; they are mounted read-only and run only inside a container via an allowed tool.
7. Invalid front-matter rejects the install; no partial role registration.

## Install

```console
$ microswarm skills install ./code-review-team
installed code-review-team@1.2.0 (sha256:…)
  role researcher  policy=allow  tools=3  mounts=1  creds=1
  role tester      policy=ask    tools=2  mounts=2  creds=0
```
