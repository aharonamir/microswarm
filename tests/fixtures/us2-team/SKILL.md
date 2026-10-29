---
name: us2-test-team
version: 1.0.0
description: Deterministic roles used by MicroSwarm US2 tests.
kind: team
roles:
  researcher:
    spawn_policy: allow
    instructions: roles/researcher.md
    scope:
      mounts:
        - { path: /work, mode: ro }
      credentials: []
      tools:
        - { server: microswarm-tools, tools: [fs.read, fs.list] }
    limits: { max_turns: 30, memory_mb: 256 }
  tester:
    spawn_policy: ask
    instructions: roles/tester.md
    scope:
      mounts:
        - { path: /work, mode: ro }
        - { path: /work/target, mode: rw }
      credentials: []
      tools:
        - { server: microswarm-tools, tools: [shell.run, fs.read] }
  forbidden:
    spawn_policy: deny
    instructions: roles/forbidden.md
    scope:
      mounts: []
      credentials: []
      tools: []
---
Test-only role definitions for MicroSwarm's US2 runtime tests.
