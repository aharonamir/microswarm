---
name: code-review-team
version: 1.2.0
description: Reviews a change with a researcher and an isolated test runner.
kind: team
roles:
  researcher:
    spawn_policy: allow
    instructions: roles/researcher.md
    scope:
      mounts:
        - { path: /work, mode: ro }
      credentials: [model-key, web-search]
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
      credentials: [model-key]
      tools:
        - { server: microswarm-tools, tools: [shell.run, fs.read] }
    limits: { max_turns: 20, memory_mb: 512 }
---
Use the researcher to inspect the requested code and report concrete findings with file paths.
Use the tester when a change needs validation. Review its proposed command and grant only
its requested `/work/target` write access. The lead combines both reports and decides what
changes to make.
