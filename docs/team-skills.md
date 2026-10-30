# Team skill authoring

A team skill is a directory containing `SKILL.md` and optional role instruction
files. Installation registers role templates in `supervisor.db`; skill scripts
are mounted read-only in member containers and never run on the host.

Start from [`examples/skills/code-review-team`](../examples/skills/code-review-team):

```text
review-team/
  SKILL.md
  roles/
    researcher.md
    tester.md
```

The `SKILL.md` front matter declares its identity and roles:

```yaml
---
name: review-team
version: 1.0.0
description: Review a change and run its tests.
kind: team
roles:
  researcher:
    spawn_policy: ask
    instructions: roles/researcher.md
    scope:
      mounts:
        - { path: /work, mode: ro }
      credentials: []
      tools:
        - { server: microswarm-tools, tools: [fs.read, fs.list] }
    limits: { max_turns: 20, memory_mb: 256 }
---
When reviewing a change, use the researcher role to inspect the requested files.
```

Below the front matter is guidance for the lead. Each role's `instructions`
file contains that member's instructions. Keep the requested paths, credentials,
and tools as narrow as possible.

Install and inspect the registered roles:

```sh
microswarm skills install ./review-team
```

The skill scope is a request template, not a grant. A member receives only the
intersection of its requested scope, the lead scope, and the system ceiling.
The lead may narrow a template but cannot widen it. Host `policy.toml` may make
the skill's `spawn_policy` stricter. Members cannot spawn other members.

Installation validates the complete front matter before registering roles and
records a content hash. Reinstall after changing any file in the skill so the
registered hash and role definitions match the directory. Invalid front matter
does not leave a partial install.
